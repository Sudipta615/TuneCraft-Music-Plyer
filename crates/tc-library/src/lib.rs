//! Library management — scanning, metadata, and cover art
//!
//! never stored), album `has_cover` flag now set correctly, eliminated
//! redundant file I/O by combining audio probe and cover art extraction into a
//! single file open, pre-loaded track path map for O(1) existence checks,
//! added progress throttling, fixed inaccurate `files_processed` counter,
//! removed dead code (`process_file`), fixed negative duration sentinel,
//! extracted `probe_file` DRY helper, added `files_failed` counter, improved
//! logging for batch failures and non-UTF-8 filenames, and unified atomic
//! orderings.

mod cover_art;
mod metadata;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant, UNIX_EPOCH},
};

pub use cover_art::{detect_image_mime, CoverArtData};
use log::{info, warn};
use tc_config::LibraryConfig;
use tc_db::{models::Track, Database};
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("Database error: {0}")]
    Database(#[from] tc_db::DbError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Scan cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

/// Supported audio file extensions
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "opus", "wav", "aac", "m4a", "wma", "aiff", "ape", "alac",
];

/// Minimum interval between progress callback invocations.
///
/// For large libraries, invoking the callback for every file can overwhelm
/// the UI thread. We throttle to at most one call per this duration.
const PROGRESS_THROTTLE: Duration = Duration::from_millis(100);

/// Progress information during a library scan
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub files_found: u32,
    pub files_processed: u32,
    pub files_added: u32,
    pub files_updated: u32,
    pub files_removed: u32,
    /// Number of files that failed to be processed (parse/IO errors, batch failures).
    pub files_failed: u32,
    pub current_path: String,
}

/// Result of processing a single file during a scan
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileChange {
    Added,
    Updated,
    Unchanged,
}

/// RAII guard that resets the scanning flag on drop
struct ScanGuard {
    scanning: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.scanning.store(false, Ordering::Release);
    }
}

/// Metadata tags extracted from an audio file via symphonia's metadata API.
#[derive(Debug, Clone, Default)]
pub struct FileTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
}

/// Library scanner and manager
pub struct LibraryManager {
    db: Arc<Database>,
    config: LibraryConfig,
    /// Cancellation flag — set by `cancel_scan()`, read inside the scan loop.
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    /// Guard against concurrent scans — set to true while a scan is running.
    scanning: Arc<std::sync::atomic::AtomicBool>,
}

impl LibraryManager {
    pub fn new(db: Arc<Database>, config: LibraryConfig) -> Self {
        Self {
            db,
            config,
            cancel_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            scanning: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Check if a file extension is a supported audio format
    pub fn is_audio_file(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    /// Scan the library directories for new and modified files.
    ///
    /// Uses a two-phase approach:
    /// 1. **Phase 1 — Metadata extraction**: Walk directories, extract metadata (and embedded cover
    ///    art) from each file in a single probe pass, and classify as Added/Updated/Unchanged.
    /// 2. **Phase 2 — Batch database writes**: Insert/update tracks in batches of 250 within
    ///    explicit transactions (~25× faster than per-file).
    /// 3. **Phase 3 — Album linkage**: After `refresh_albums_and_artists()`, resolve album IDs and
    ///    store accumulated cover art with correct links.
    ///
    /// v0.8.10 improvements:
    /// - Pre-loads existing track paths into a `HashMap` for O(1) lookups.
    /// - Combined probe: cover art extracted in the same symphonia pass.
    /// - Progress callback throttled to ≤10 Hz.
    /// - `files_processed` incremented unconditionally.
    /// - `files_failed` counter added to `ScanProgress`.
    pub fn scan<F: Fn(ScanProgress)>(
        &self,
        progress_callback: F,
    ) -> Result<ScanProgress, LibraryError> {
        self.cancel_flag.store(false, Ordering::Release);

        // Prevent concurrent scans
        if self
            .scanning
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(LibraryError::Other(
                "A scan is already in progress".to_string(),
            ));
        }
        let _guard = ScanGuard {
            scanning: Arc::clone(&self.scanning),
        };

        let mut progress = ScanProgress {
            files_found: 0,
            files_processed: 0,
            files_added: 0,
            files_updated: 0,
            files_removed: 0,
            files_failed: 0,
            current_path: String::new(),
        };

        // Pre-load existing (path → file_modified) for O(1) lookups
        let existing_tracks: HashMap<String, i64> = self
            .db
            .get_tracks_with_mtime()?
            .into_iter()
            .map(|(_id, path, mtime)| (path, mtime))
            .collect();

        let mut audio_files: Vec<PathBuf> = Vec::new();
        for dir in &self.config.watch_dirs {
            for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
                if self.cancel_flag.load(Ordering::Acquire) {
                    return Err(LibraryError::Cancelled);
                }
                let path = entry.path();
                if path.is_file() && Self::is_audio_file(path) {
                    audio_files.push(path.to_path_buf());
                    progress.files_found += 1;
                }
            }
        }

        info!("Found {} audio files", audio_files.len());

        const BATCH_SIZE: usize = 250;
        let mut new_tracks: Vec<Track> = Vec::with_capacity(BATCH_SIZE);
        let mut updated_tracks: Vec<Track> = Vec::with_capacity(BATCH_SIZE);
        // Cover art pre-extracted alongside new_tracks; indices match
        let mut pending_cover_art: Vec<Option<CoverArtData>> = Vec::with_capacity(BATCH_SIZE);
        // Cover art ready to persist after album IDs are known
        let mut cover_art_queue: Vec<(PathBuf, i64, CoverArtData)> = Vec::new();

        let mut last_callback = Instant::now();

        for path in &audio_files {
            if self.cancel_flag.load(Ordering::Acquire) {
                return Err(LibraryError::Cancelled);
            }

            // Throttled progress callback
            let now = Instant::now();
            if now.duration_since(last_callback) >= PROGRESS_THROTTLE {
                progress.current_path = path.to_string_lossy().into_owned();
                progress_callback(progress.clone());
                last_callback = now;
            }

            let file_metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(e) => {
                    warn!("Failed to read metadata for {}: {}", path.display(), e);
                    progress.files_processed += 1;
                    progress.files_failed += 1;
                    continue;
                },
            };
            let file_size = file_metadata.len() as i64;
            let file_modified = file_metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let path_str = path.to_string_lossy().into_owned();

            if let Some(&existing_mtime) = existing_tracks.get(&path_str) {
                if existing_mtime >= file_modified {
                    progress.files_processed += 1;
                    continue;
                }
                // Modified file — update, skip cover re-extraction
                match self.extract_track_info(path, file_size, file_modified) {
                    Ok(track) => updated_tracks.push(track),
                    Err(e) => {
                        warn!("Failed to extract info for {}: {}", path.display(), e);
                        progress.files_processed += 1;
                        progress.files_failed += 1;
                        continue;
                    },
                }
            } else {
                // New file — combined probe for metadata + cover art
                match self.extract_track_info_with_cover(path, file_size, file_modified) {
                    Ok((track, cover)) => {
                        new_tracks.push(track);
                        pending_cover_art.push(cover);
                    },
                    Err(e) => {
                        warn!("Failed to extract info for {}: {}", path.display(), e);
                        progress.files_processed += 1;
                        progress.files_failed += 1;
                        continue;
                    },
                }
            }

            progress.files_processed += 1;

            if new_tracks.len() >= BATCH_SIZE {
                self.flush_new_batch(
                    &mut new_tracks,
                    &mut pending_cover_art,
                    &mut cover_art_queue,
                    &mut progress,
                );
            }
            if updated_tracks.len() >= BATCH_SIZE {
                self.flush_updated_batch(&mut updated_tracks, &mut progress);
            }
        }

        if !new_tracks.is_empty() {
            self.flush_new_batch(
                &mut new_tracks,
                &mut pending_cover_art,
                &mut cover_art_queue,
                &mut progress,
            );
        }
        if !updated_tracks.is_empty() {
            self.flush_updated_batch(&mut updated_tracks, &mut progress);
        }

        // Refresh album/artist tables so album IDs exist before Phase 3
        if let Err(e) = self.db.refresh_albums_and_artists() {
            warn!("Failed to refresh albums/artists after scan: {}", e);
        }

        for (path, track_id, art) in &cover_art_queue {
            if self.cancel_flag.load(Ordering::Acquire) {
                return Err(LibraryError::Cancelled);
            }

            let album_id = self.db.get_track(*track_id).ok().flatten().and_then(|t| {
                let album = t.album.as_deref()?;
                self.db
                    .get_album_id(album, t.album_artist.as_deref())
                    .ok()
                    .flatten()
            });

            match self.db.insert_cover_art(
                album_id,
                Some(*track_id),
                None,
                Some(&art.data),
                Some(&art.data_hash),
                art.width,
                art.height,
                &art.mime_type,
            ) {
                Ok(_) => {
                    info!(
                        "Stored cover art for track {} ({} bytes, {}×{})",
                        track_id,
                        art.data.len(),
                        art.width,
                        art.height
                    );
                },
                Err(e) => {
                    warn!("Failed to persist cover art for {}: {}", path.display(), e);
                },
            }
        }

        let existing_paths: Vec<String> = audio_files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let existing_refs: Vec<&str> = existing_paths.iter().map(|s| s.as_str()).collect();
        match self.db.cleanup_missing_tracks(&existing_refs) {
            Ok(removed) => {
                progress.files_removed = removed as u32;
                info!("Removed {} tracks with missing files", removed);
            },
            Err(e) => warn!("Failed to cleanup missing tracks: {}", e),
        }

        info!(
            "Scan complete: {} added, {} updated, {} removed, {} failed",
            progress.files_added,
            progress.files_updated,
            progress.files_removed,
            progress.files_failed
        );

        // Final callback so UI reaches 100%
        progress.current_path = String::new();
        progress_callback(progress.clone());

        Ok(progress)
    }

    fn flush_new_batch(
        &self,
        new_tracks: &mut Vec<Track>,
        pending_cover_art: &mut Vec<Option<CoverArtData>>,
        cover_art_queue: &mut Vec<(PathBuf, i64, CoverArtData)>,
        progress: &mut ScanProgress,
    ) {
        match self.db.insert_tracks_batch(new_tracks) {
            Ok(ids) => {
                progress.files_added += ids.len() as u32;
                let failed = new_tracks.len().saturating_sub(ids.len());
                if failed > 0 {
                    warn!(
                        "Batch insert: {} of {} tracks failed",
                        failed,
                        new_tracks.len()
                    );
                    progress.files_failed += failed as u32;
                }
                for (idx, track_id) in ids {
                    if let Some(Some(_)) = pending_cover_art.get(idx) {
                        // take out the CoverArtData
                        if let Some(slot) = pending_cover_art.get_mut(idx) {
                            if let Some(art) = slot.take() {
                                let path = PathBuf::from(&new_tracks[idx].path);
                                cover_art_queue.push((path, track_id, art));
                            }
                        }
                    }
                }
            },
            Err(e) => {
                warn!(
                    "Batch insert failed (all {} tracks lost): {}",
                    new_tracks.len(),
                    e
                );
                progress.files_failed += new_tracks.len() as u32;
            },
        }
        new_tracks.clear();
        pending_cover_art.clear();
    }

    fn flush_updated_batch(&self, updated_tracks: &mut Vec<Track>, progress: &mut ScanProgress) {
        match self
            .db
            .update_tracks_batch_preserving_userdata(updated_tracks)
        {
            Ok(count) => progress.files_updated += count as u32,
            Err(e) => {
                warn!(
                    "Batch update failed (all {} tracks lost): {}",
                    updated_tracks.len(),
                    e
                );
                progress.files_failed += updated_tracks.len() as u32;
            },
        }
        updated_tracks.clear();
    }

    /// Cancel an in-progress scan.
    pub fn cancel_scan(&self) {
        self.cancel_flag.store(true, Ordering::Release);
    }

    /// Update the configuration. Must not be called while a scan is in progress.
    pub fn set_config(&mut self, config: LibraryConfig) {
        self.config = config;
    }

    /// Scan a list of individual audio files and insert them into the database.
    ///
    /// Unlike `scan()`, this does not walk directories or clean up missing
    /// tracks. It is intended for "Add Music" (individual file selection).
    /// Returns the number of files successfully added.
    pub fn scan_files(&self, paths: &[std::path::PathBuf]) -> usize {
        use std::time::UNIX_EPOCH;

        let mut added = 0usize;
        let mut new_tracks: Vec<Track> = Vec::new();
        let mut pending_cover_art: Vec<Option<CoverArtData>> = Vec::new();

        for path in paths {
            if !path.is_file() || !Self::is_audio_file(path) {
                warn!("Skipping non-audio file: {}", path.display());
                continue;
            }

            let file_metadata = match std::fs::metadata(path) {
                Ok(m) => m,
                Err(e) => {
                    warn!("Failed to read metadata for {}: {}", path.display(), e);
                    continue;
                },
            };
            let file_size = file_metadata.len() as i64;
            let file_modified = file_metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            match self.extract_track_info_with_cover(path, file_size, file_modified) {
                Ok((track, cover)) => {
                    new_tracks.push(track);
                    pending_cover_art.push(cover);
                },
                Err(e) => {
                    warn!("Failed to extract info for {}: {}", path.display(), e);
                },
            }
        }

        if !new_tracks.is_empty() {
            match self.db.insert_tracks_batch(&new_tracks) {
                Ok(ids) => {
                    added = ids.len();
                    // Store cover art for successfully inserted tracks
                    for (idx, track_id) in ids {
                        if let Some(Some(art)) = pending_cover_art.get(idx) {
                            let album_id = self.db.get_track(track_id).ok().flatten().and_then(|t| {
                                let album = t.album.as_deref()?;
                                self.db
                                    .get_album_id(album, t.album_artist.as_deref())
                                    .ok()
                                    .flatten()
                            });
                            let _ = self.db.insert_cover_art(
                                album_id,
                                Some(track_id),
                                None,
                                Some(&art.data),
                                Some(&art.data_hash),
                                art.width,
                                art.height,
                                &art.mime_type,
                            );
                        }
                    }
                },
                Err(e) => {
                    warn!("Batch insert failed: {}", e);
                },
            }

            // Refresh albums/artists
            if let Err(e) = self.db.refresh_albums_and_artists() {
                warn!("Failed to refresh albums/artists: {}", e);
            }
        }

        added
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_audio_file() {
        assert!(LibraryManager::is_audio_file(Path::new("song.mp3")));
        assert!(LibraryManager::is_audio_file(Path::new("song.FLAC")));
        assert!(LibraryManager::is_audio_file(Path::new("track.m4a")));
        assert!(!LibraryManager::is_audio_file(Path::new("document.pdf")));
        assert!(!LibraryManager::is_audio_file(Path::new("no_extension")));
    }

    #[test]
    fn test_detect_image_mime() {
        assert_eq!(detect_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(
            detect_image_mime(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            "image/png"
        );
        assert_eq!(
            detect_image_mime(&[0x47, 0x49, 0x46, 0x38, 0x39, 0x61]),
            "image/gif"
        );
        assert_eq!(detect_image_mime(&[0x42, 0x4D, 0x00, 0x00]), "image/bmp");
        assert_eq!(
            detect_image_mime(&[0x00, 0x01, 0x02]),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_scan_progress_has_files_failed() {
        let p = ScanProgress {
            files_found: 10,
            files_processed: 9,
            files_added: 7,
            files_updated: 1,
            files_removed: 0,
            files_failed: 1,
            current_path: String::new(),
        };
        assert_eq!(p.files_failed, 1);
    }
}
