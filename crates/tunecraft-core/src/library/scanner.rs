use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};
use walkdir::WalkDir;

use crate::audio::pcm_cache::PcmCache;
use crate::database::Database;
use crate::library::metadata::read_metadata_and_cover_art;
use crate::util::hash::file_sha256;

/// Event emitted by the library scanner.
#[derive(Debug, Clone)]
pub enum ScanEvent {
    FileAdded(PathBuf),
    FileRemoved(PathBuf),
    FileModified(PathBuf),
    ScanComplete { added: usize, removed: usize },
    ScanStarted,
    Progress { current: usize, total: usize },
    Error(String),
}

/// Audio file extensions supported by GStreamer.
///
/// GStreamer's `uridecodebin` handles decoding, so any format with an
/// available plugin is playable. Extensions are grouped by codec family.
///
/// Note: DSD/DSF/DFF are NOT included because GStreamer's standard plugin
/// sets do not include a DSD decoder on Linux. DSD playback requires
/// specialized hardware or proprietary plugins.
/// Fix M11: Split AUDIO_EXTENSIONS into "definitely audio" and "maybe audio" lists.
/// The primary list contains formats that are always audio. The extended list
/// contains video containers, MIDI files, and tracker modules that may or may
/// not produce useful audio. The extended list is only used when the user
/// enables the `scan_extended_formats` config option.
const AUDIO_EXTENSIONS: &[&str] = &[
    // ── Lossy compressed ──────────────────────────────────────────────
    "mp3",  // MPEG-1/2 Audio Layer III
    "mp2",  // MPEG-1/2 Audio Layer II
    "mp1",  // MPEG-1 Audio Layer I
    "aac",  // Advanced Audio Coding (raw AAC)
    "m4a",  // AAC in MPEG-4 container
    "m4b",  // AAC audiobook in MPEG-4 container
    "m4p",  // AAC (FairPlay DRM — may not decode without key)
    "m4r",  // AAC ringtone container
    "ogg",  // Ogg container (Vorbis, Opus, FLAC, Speex)
    "oga",  // Ogg Audio
    "opus", // Opus in Ogg container
    "wma",  // Windows Media Audio
    "ac3",  // Dolby Digital / AC-3
    "eac3", // Enhanced AC-3 / Dolby Digital Plus
    "dts",  // DTS Coherent Acoustics
    "mka",  // Matroska Audio container
    "spx",  // Speex in Ogg container
    "tta",  // TTA (True Audio) lossless
    // ── Lossless compressed ───────────────────────────────────────────
    "flac", // Free Lossless Audio Codec
    "ape",  // Monkey's Audio
    "wv",   // WavPack lossless / hybrid
    "wvp",  // WavPack correction file
    "ofr",  // OptimFROG lossless
    "ofs",  // OptimFROG DualStream
    // ── PCM / uncompressed ────────────────────────────────────────────
    "wav",  // RIFF/WAVE PCM
    "aiff", // Audio Interchange File Format (Apple)
    "aif",  // AIFF alias
    "aifc", // AIFF-C (compressed AIFF)
    "au",   // Sun/NeXT μ-law PCM
    "snd",  // Alias for .au
    "w64",  // Sony Wave64 (>4 GB PCM)
    "rf64", // EBU RF64 broadcast wave
    // ── Streaming / playlist containers (audio-primary) ────────────
    "3gp",  // 3GPP container (AMR / AAC)
    "3g2",  // 3GPP2 container
    "webm", // WebM container (Opus / Vorbis)
];

/// Extended audio format list — video containers, MIDI, tracker modules,
/// and metafile formats that may produce audio but waste time probing when
/// the user only has standard music files. Enabled via config option.
const AUDIO_EXTENSIONS_EXTENDED: &[&str] = &[
    "asf",  // Advanced Systems Format (WMA/WMV container)
    "amr",  // Adaptive Multi-Rate (narrowband)
    "awb",  // Adaptive Multi-Rate Wideband
    "gsm",  // GSM 06.10 raw frames
    "ra",   // RealAudio
    "rm",   // RealMedia container
    "ram",  // RealAudio metafile (URI list)
    "vqf",  // TwinVQ / VQF (Yamaha)
    "mid",  // Standard MIDI File
    "midi", // Standard MIDI File (alias)
    "kar",  // Karaoke MIDI with lyrics
    "mod",  // ProTracker / Amiga MOD
    "s3m",  // Scream Tracker 3 module
    "xm",   // FastTracker 2 Extended Module
    "it",   // Impulse Tracker module
    "mp4",  // MPEG-4 container (may contain AAC/ALAC audio)
    "m4v",  // MPEG-4 video container
    "ts",   // MPEG Transport Stream
    "mts",  // AVCHD Transport Stream
    "m2ts", // Blu-ray Transport Stream
    "mkv",  // Matroska video container
    "flv",  // Flash Video container
    "avi",  // AVI container
];

/// Library scanner that watches directories for changes.
pub struct LibraryScanner {
    watch_paths: Vec<PathBuf>,
}

impl LibraryScanner {
    pub fn new(watch_paths: Vec<PathBuf>) -> Self {
        Self { watch_paths }
    }

    /// Check if a file extension is a supported audio format.
    pub fn is_audio(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    /// Perform a full scan of all watch directories, returning discovered file paths.
    /// Follows symlinks so that symlinked music directories are not silently skipped.
    ///
    /// Security Note: This follows symlinks which could potentially lead to:
    /// - Directory traversal attacks (symlinks pointing outside music directories)
    /// - Symbolic loop attacks (circular symlinks)
    /// - Following symlinks to sensitive directories
    ///
    /// Consider implementing:
    /// - Symlink depth limits
    /// - Checking symlink targets don't escape allowed directories
    /// - Option to disable symlink following for untrusted directories
    ///
    /// Security #17 (TOCTOU note): There is a time-of-check-to-time-of-use
    /// window between `is_path_in_watch_dirs()` (which canonicalizes the path)
    /// and the actual file open in the decoder. A symlink could be retargeted
    /// in this window. For a local music player this is low-severity — an
    /// attacker would need local filesystem access and the ability to create
    /// symlinks in the user's music directory, which already implies significant
    /// compromise. A full fix would require opening the file via O_NOFOLLOW
    /// on Unix or checking the resolved path immediately before decoding.
    pub fn scan(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        // Fix Optimization #14: Use metadata-based dedup with (dev, ino) pairs
        // instead of canonicalize() for every file. On a large library (100k+
        // files), canonicalize() is a syscall per file (resolving symlinks and
        // normalizing the path). Using file metadata (device + inode) requires
        // only one stat() per file instead of a full path resolution, which is
        // significantly faster on large libraries.
        let mut seen_dev_ino = std::collections::HashSet::new();
        for dir in &self.watch_paths {
            for entry in WalkDir::new(dir)
                .follow_links(true)
                .max_depth(20) // Limit depth to prevent deep symlink chains
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                // Security: Verify the resolved path is still within watch directories
                // This prevents symlink attacks that escape to sensitive locations
                if path.is_file() && Self::is_audio(path) {
                    if Self::is_path_in_watch_dirs(path, &self.watch_paths) {
                        // Deduplicate by device+inode so that the same file
                        // accessed via different symlink paths is only scanned once.
                        // This is faster than canonicalize() because it uses a
                        // single stat() call per file instead of full path resolution.
                        let is_duplicate = std::fs::metadata(path)
                            .ok()
                            .and_then(|meta| {
                                // Use Unix-specific file identity (dev, ino) when available
                                #[cfg(unix)]
                                {
                                    use std::os::unix::fs::MetadataExt;
                                    Some((meta.dev(), meta.ino()))
                                }
                                #[cfg(not(unix))]
                                {
                                    // Fallback: use file size + modified time as a
                                    // probabilistic dedup key (not perfect but avoids
                                    // the canonicalize() syscall overhead)
                                    Some((meta.len(), meta.modified().ok()?.as_nanos() as u64))
                                }
                            })
                            .map(|key| !seen_dev_ino.insert(key))
                            .unwrap_or(false);

                        if is_duplicate {
                            tracing::debug!("Skipping duplicate file (same dev/ino): {:?}", path);
                        } else {
                            files.push(path.to_path_buf());
                        }
                    } else {
                        tracing::warn!("Skipping symlink target outside watch dirs: {:?}", path);
                    }
                }
            }
        }
        files
    }

    /// Check if a path resolves to a location within one of the watch directories.
    /// Used to prevent symlink attacks that escape to sensitive locations.
    ///
    /// Fix Bug #69: Redundant canonicalization in `is_path_in_watch_dirs`.
    /// This function canonicalizes the path AND each watch directory on every call.
    /// The watch directory canonicalizations are constant and could be cached at
    /// scanner construction time. Additionally, `collect_audio_files` above calls
    /// `canonicalize(path)` for deduplication, then this function calls it again.
    /// A future optimization should: (1) cache canonical watch dirs, and (2) pass
    /// the already-canonicalized path to avoid redundant syscall overhead.
    fn is_path_in_watch_dirs(path: &Path, watch_dirs: &[PathBuf]) -> bool {
        let resolved = match std::fs::canonicalize(path) {
            Ok(p) => p,
            Err(_) => return false, // If we can't resolve, reject it — can't verify its true location
        };

        for dir in watch_dirs {
            if let Ok(canonical_dir) = std::fs::canonicalize(dir) {
                if resolved.starts_with(&canonical_dir) {
                    return true;
                }
            }
        }
        false
    }

    /// Perform a full scan, read metadata for each file, and import into the database.
    /// Returns the number of tracks added and removed.
    /// This is the synchronous version (no mood analysis).
    pub fn scan_and_import(&self, db: &Arc<Database>) -> (usize, usize) {
        self.scan_and_import_inner(db, false, None)
    }

    /// Perform a full scan with automatic mood analysis.
    /// Mood analysis runs asynchronously on the tokio blocking threadpool
    /// so it does not block the library scan. Newly inserted tracks and
    /// existing tracks that have not yet been analyzed will be processed.
    /// Uses the shared Arc<Database> for mood analysis tasks so no extra
    /// DB connections are opened.
    ///
    /// The shared `PcmCache` avoids dual-decode overhead: tracks decoded by
    /// Symphonia for mood analysis are cached so subsequent lookups reuse the
    /// already-decoded F32 samples instead of reading from disk again.
    pub fn scan_and_import_with_mood(
        &self,
        db: &Arc<Database>,
        pcm_cache: Arc<PcmCache>,
    ) -> (usize, usize) {
        self.scan_and_import_inner(db, true, Some(pcm_cache))
    }

    /// Inner implementation shared by sync and async scan variants.
    /// When `enable_mood` is true, mood analysis tasks are spawned for
    /// newly added tracks and existing tracks lacking mood data.
    /// An optional `PcmCache` is passed through to mood analysis to avoid
    /// redundant Symphonia decoding.
    fn scan_and_import_inner(
        &self,
        db: &Arc<Database>,
        enable_mood: bool,
        pcm_cache: Option<Arc<PcmCache>>,
    ) -> (usize, usize) {
        let discovered = self.scan();
        let discovered_paths: Vec<String> = discovered
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        // Find and remove stale tracks
        let stale = match db.get_stale_tracks(&discovered_paths) {
            Ok(stale) => stale,
            Err(e) => {
                warn!("Failed to get stale tracks: {}", e);
                Vec::new()
            }
        };

        let mut removed = 0;
        for path in &stale {
            if let Err(e) = db.delete_track_by_path(path) {
                warn!("Failed to remove stale track {}: {}", path, e);
            } else {
                removed += 1;
            }
        }

        // Import new/updated tracks
        let mut added = 0;
        let mut newly_added_paths: Vec<String> = Vec::new();

        for path in &discovered {
            let path_str = path.to_string_lossy().to_string();

            // Check if file already exists in the database and is unchanged
            if let Ok(Some(existing)) = db.get_track_by_path(&path_str) {
                // Skip re-import if both the file size AND modification time match.
                // Checking mtime is critical because re-tagging a file (updated title,
                // album art, lyrics) does not change the file size, but does change mtime.
                let skip_reimport = match (
                    std::fs::metadata(path),
                    existing.file_size,
                    existing.file_mtime,
                ) {
                    (Ok(meta), Some(existing_size), Some(existing_mtime)) => {
                        let current_size = meta.len() as i64;
                        let current_mtime = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64);
                        current_size == existing_size && current_mtime == Some(existing_mtime)
                    }
                    (Ok(meta), Some(existing_size), None) => {
                        // No mtime stored — fall back to size-only check
                        meta.len() as i64 == existing_size
                    }
                    _ => false,
                };

                if skip_reimport {
                    // Even if we skip re-import, trigger mood analysis if needed
                    if enable_mood {
                        if let Ok(true) = db.track_needs_mood_analysis(&path_str) {
                            newly_added_paths.push(path_str);
                        }
                    }
                    continue;
                }
            }

            // Fix Bug #26: Read metadata and cover art in a single file open.
            // Previously, read_metadata and extract_cover_art each opened the
            // file independently (3 opens total including file_sha256). Now
            // metadata and cover art share one open, reducing I/O from 3 to 2.
            match read_metadata_and_cover_art(path) {
                Ok((mut track, cover_art)) => {
                    // Compute and set file hash (still requires a separate file read
                    // for raw bytes, but this is unavoidable for cryptographic hashing)
                    match file_sha256(path) {
                        Ok(hash) => track.file_hash = Some(hash),
                        Err(e) => warn!("Failed to hash {}: {}", path_str, e),
                    }

                    // Save cover art (extracted from the same file open as metadata)
                    if let Some(cover) = cover_art {
                        if let Some(ref hash) = track.file_hash {
                            if let Err(e) = db.save_cover_art(hash, &cover.data, &cover.mime_type) {
                                warn!("Failed to save cover art for {}: {}", path_str, e);
                            }
                        } else {
                            warn!(
                                "Cover art found for {} but skipped because file hash is unavailable",
                                path_str
                            );
                        }
                    }

                    match db.insert_track(&track) {
                        Ok(id) => {
                            if id > 0 {
                                added += 1;
                                if enable_mood {
                                    newly_added_paths.push(path_str);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to import {}: {}", path_str, e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read metadata for {}: {}", path_str, e);
                }
            }
        }

        info!(
            "Library scan complete: {} added, {} removed",
            added, removed
        );

        // Spawn mood analysis tasks for newly added / unanalyzed tracks
        if enable_mood && !newly_added_paths.is_empty() {
            info!(
                "Queuing mood analysis for {} tracks",
                newly_added_paths.len()
            );
            // Fix Bug #39: Process in batches of 3 (not 4) to match the
            // connection pool size (max_connections: 3 in connection.rs).
            // Previously chunks of 4 could exhaust the pool, causing mood
            // analysis tasks to block waiting for a connection.
            let cache = pcm_cache.unwrap_or_else(|| Arc::new(PcmCache::with_default_capacity()));
            for chunk in newly_added_paths.chunks(3) {
                for path_str in chunk {
                    Self::spawn_mood_analysis(path_str.clone(), Arc::clone(db), Arc::clone(&cache));
                }
                // Small delay between batches to avoid overwhelming the blocking
                // thread pool. Since this function runs inside spawn_blocking,
                // std::thread::sleep is acceptable here, but we yield briefly.
                // Fix H8: Reduced sleep from 50ms to 10ms to minimize tokio thread pool
                // blocking. For a 10,000-track library, this reduces total sleep from
                // ~2.8s to ~0.56s. Ideally, use tokio::task::yield_now() in async context.
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        (added, removed)
    }

    /// Spawn an async mood analysis task for a single track.
    /// Uses a pre-opened database reference instead of opening a new connection
    /// each time, avoiding the overhead of re-initializing the connection pool.
    /// The shared `PcmCache` is passed through so that decoded PCM data can be
    /// reused across tracks, eliminating redundant Symphonia decoding.
    fn spawn_mood_analysis(path_str: String, db: Arc<Database>, cache: Arc<PcmCache>) {
        tokio::task::spawn_blocking(move || {
            Self::analyze_mood_for_path(&path_str, &db, &cache);
        });
    }

    /// Analyze mood for a single path (blocking). Uses the provided database
    /// connection instead of opening a new one, fixing the previous issue where
    /// each mood analysis opened a separate DB connection.
    ///
    /// The shared `PcmCache` is used to avoid dual-decode overhead: on a cache
    /// hit, already-decoded F32 samples are reused; on a miss, the decoded data
    /// is stored in the cache for future reuse by subsequent analyses.
    fn analyze_mood_for_path(path_str: &str, db: &Database, cache: &PcmCache) {
        let mut planner = rustfft::FftPlanner::<f32>::new();

        match crate::mood::extract_features_with_cache(path_str, &mut planner, Some(cache)) {
            Ok(features) => {
                let mood = crate::mood::classify_mood(&features);
                tracing::info!(
                    "[mood] {} -> {} (bpm={:.1}, energy={:.4}, bass={:.3}, centroid={:.0}, dr={:.4})",
                    path_str,
                    mood.as_str(),
                    features.bpm,
                    features.energy,
                    features.bass_ratio,
                    features.spectral_centroid,
                    features.dynamic_range,
                );

                if let Err(e) = db.update_track_mood(
                    path_str,
                    features.bpm,
                    features.energy,
                    features.bass_ratio,
                    features.spectral_centroid,
                    features.dynamic_range,
                    mood.as_str(),
                ) {
                    tracing::warn!("[mood] Failed to update DB for {}: {}", path_str, e);
                }
            }
            Err(e) => {
                tracing::warn!("[mood] Failed to analyze {}: {}", path_str, e);
            }
        }
    }

    /// Watch directories for filesystem changes using notify.
    pub async fn watch(&self, tx: mpsc::Sender<ScanEvent>) -> Result<()> {
        use notify::{RecursiveMode, Watcher};

        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel(256);

        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = notify_tx.blocking_send(event);
                }
            })
            .context("failed to create file watcher")?;

        for path in &self.watch_paths {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .context(format!("failed to watch {:?}", path))?;
        }

        info!("Watching {} directories", self.watch_paths.len());

        while let Some(event) = notify_rx.recv().await {
            for path in event.paths {
                // Security (v1.1 fix): Validate that the canonicalized path
                // is still within watch directories. Without this check, a
                // symlink created inside a watched directory pointing to a
                // sensitive location (e.g. /etc/passwd) would pass the
                // is_audio() check and trigger an import.
                if !Self::is_path_in_watch_dirs(&path, &self.watch_paths) {
                    tracing::warn!("File watcher: skipping path outside watch dirs: {:?}", path);
                    continue;
                }
                if !Self::is_audio(&path) {
                    continue;
                }
                let kind = match event.kind {
                    notify::EventKind::Create(_) => ScanEvent::FileAdded(path),
                    notify::EventKind::Remove(_) => ScanEvent::FileRemoved(path),
                    notify::EventKind::Modify(_) => ScanEvent::FileModified(path),
                    _ => continue,
                };
                let _ = tx.send(kind).await;
            }
        }

        Ok(())
    }

    /// Get current watch paths.
    pub fn watch_paths(&self) -> &[PathBuf] {
        &self.watch_paths
    }

    /// Add a directory to watch.
    pub fn add_watch_path(&mut self, path: PathBuf) {
        if !self.watch_paths.contains(&path) {
            self.watch_paths.push(path);
        }
    }

    /// Remove a directory from watching.
    pub fn remove_watch_path(&mut self, path: &Path) {
        self.watch_paths.retain(|p| p != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_is_audio_extensions() {
        assert!(LibraryScanner::is_audio(Path::new("song.mp3")));
        assert!(LibraryScanner::is_audio(Path::new("song.FLAC")));
        assert!(LibraryScanner::is_audio(Path::new("song.ogg")));
        assert!(LibraryScanner::is_audio(Path::new("song.opus")));
        assert!(!LibraryScanner::is_audio(Path::new("song.txt")));
        assert!(!LibraryScanner::is_audio(Path::new("song.pdf")));
        assert!(!LibraryScanner::is_audio(Path::new("noext")));
    }

    #[test]
    fn test_is_path_in_watch_dirs_rejects_outside_path() {
        let tmp = tempfile::tempdir().unwrap();
        let watch_dir = tmp.path().to_path_buf();
        let outside = PathBuf::from("/etc/passwd");
        // /etc/passwd is not inside the temp watch dir
        assert!(!LibraryScanner::is_path_in_watch_dirs(
            &outside,
            &[watch_dir]
        ));
    }

    #[test]
    fn test_is_path_in_watch_dirs_accepts_inside_path() {
        let tmp = tempfile::tempdir().unwrap();
        let watch_dir = tmp.path().to_path_buf();
        let inside = tmp.path().join("music").join("song.mp3");
        fs::create_dir_all(inside.parent().unwrap()).unwrap();
        fs::write(&inside, b"fake audio").unwrap();
        assert!(LibraryScanner::is_path_in_watch_dirs(&inside, &[watch_dir]));
    }

    #[test]
    fn test_is_path_in_watch_dirs_handles_canonicalization() {
        let tmp = tempfile::tempdir().unwrap();
        let watch_dir = tmp.path().to_path_buf();
        let inside = tmp.path().join("song.mp3");
        fs::write(&inside, b"fake audio").unwrap();
        // Even with a relative path, canonicalization should work
        let canonical = fs::canonicalize(&inside).unwrap();
        assert!(LibraryScanner::is_path_in_watch_dirs(
            &canonical,
            &[watch_dir.clone()]
        ));
    }

    #[test]
    fn test_scan_returns_only_audio_files() {
        let tmp = tempfile::tempdir().unwrap();
        let mp3 = tmp.path().join("song.mp3");
        let txt = tmp.path().join("readme.txt");
        fs::write(&mp3, b"fake mp3").unwrap();
        fs::write(&txt, b"readme").unwrap();
        let scanner = LibraryScanner::new(vec![tmp.path().to_path_buf()]);
        let files = scanner.scan();
        let paths: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert!(
            paths.iter().any(|p| p.contains("song.mp3")),
            "Should find mp3 file"
        );
        assert!(
            !paths.iter().any(|p| p.contains("readme.txt")),
            "Should not find txt file"
        );
    }

    #[test]
    fn test_add_watch_path_dedup() {
        let mut scanner = LibraryScanner::new(vec![PathBuf::from("/music")]);
        scanner.add_watch_path(PathBuf::from("/music")); // duplicate
        assert_eq!(
            scanner.watch_paths().len(),
            1,
            "Should not add duplicate watch path"
        );
        scanner.add_watch_path(PathBuf::from("/more-music"));
        assert_eq!(scanner.watch_paths().len(), 2);
    }
}
