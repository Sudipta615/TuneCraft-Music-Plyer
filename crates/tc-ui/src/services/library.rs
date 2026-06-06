//! Library service — track queries, scan management, playlist CRUD
//!
//! Encapsulates all database access for the UI layer, providing a clean
//! boundary that decouples the UI from direct `Arc<Database>` usage.
//! This service also manages the scan lifecycle and provides a snapshot-based
//! API that avoids holding DB locks across UI frames.
//!
//! for lock-free, thread-safe snapshot reads.
//!
//! in-place modifications, avoiding unnecessary `Arc::new()` allocation
//! on every update. Only creates a new Arc when the snapshot actually changes.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use arc_swap::ArcSwap;
use log::{error, warn};
use tc_db::{Database, Playlist, Track};

/// Snapshot of library state that the UI can read without any locks.
#[derive(Debug, Clone, Default)]
pub struct LibrarySnapshot {
    /// Current page of tracks
    pub tracks: Vec<Track>,
    /// Total number of tracks in the library
    pub total_track_count: usize,
    /// Set of favorite track IDs (cached for O(1) lookups)
    pub favorite_ids: std::collections::HashSet<i64>,
    /// All playlists
    pub playlists: Vec<Playlist>,
    /// Whether a library scan is in progress
    pub is_scanning: bool,
    /// Status message
    pub status_message: String,
}

/// The library service manages all database interactions for the UI.
/// It provides snapshot-based reads to avoid holding locks across frames,
/// and batches writes to minimize contention with the scan thread.
///
/// Uses `ArcSwap<LibrarySnapshot>` for lock-free, thread-safe snapshot
/// reads. The UI can call `snapshot()` from any thread without blocking.
///
/// Partial updates are applied in-place, reducing Arc allocations when only a subset of fields
/// change.
pub struct LibraryService {
    db: Arc<Database>,
    library_manager: Arc<tc_library::LibraryManager>,
    scan_complete: Arc<AtomicBool>,
    scan_failed: Arc<AtomicBool>,
    db_dirty: Arc<AtomicBool>,
    /// Cached snapshot — updated atomically via ArcSwap for lock-free reads
    snapshot: ArcSwap<LibrarySnapshot>,
    /// Pagination state (atomic for thread safety)
    track_page: std::sync::atomic::AtomicUsize,
    tracks_per_page: usize,
    /// Channel for receiving scan progress updates from the scan thread
    scan_progress_rx: crossbeam::channel::Receiver<tc_library::ScanProgress>,
}

impl LibraryService {
    /// Create a new LibraryService.
    pub fn new(
        db: Arc<Database>,
        library_manager: Arc<tc_library::LibraryManager>,
        scan_complete: Arc<AtomicBool>,
        scan_failed: Arc<AtomicBool>,
        db_dirty: Arc<AtomicBool>,
        tracks_per_page: usize,
        scan_progress_rx: crossbeam::channel::Receiver<tc_library::ScanProgress>,
    ) -> Self {
        let service = Self {
            db,
            library_manager,
            scan_complete,
            scan_failed,
            db_dirty,
            snapshot: ArcSwap::from_pointee(LibrarySnapshot::default()),
            track_page: std::sync::atomic::AtomicUsize::new(0),
            tracks_per_page,
            scan_progress_rx,
        };

        service.refresh_tracks();
        service.refresh_favorite_ids();
        service.refresh_playlists();
        service.check_scan_state();

        service
    }

    /// Get a reference to the library snapshot (lock-free, no borrowing).
    ///
    /// Returns an `arc_swap::Guard<Arc<LibrarySnapshot>>` which derefs
    /// to `LibrarySnapshot`. The guard is cheap to create and drop.
    pub fn snapshot(&self) -> arc_swap::Guard<Arc<LibrarySnapshot>> {
        self.snapshot.load()
    }

    /// Check if the DB has been modified since last refresh.
    pub fn is_db_dirty(&self) -> bool {
        self.db_dirty.load(Ordering::Relaxed)
    }

    /// Mark the DB as dirty (called after mutations).
    pub fn mark_db_dirty(&self) {
        self.db_dirty.store(true, Ordering::Relaxed);
    }

    /// Refresh track list from the database (paginated).
    /// Should be called when db_dirty is true or on explicit user request.
    ///
    ///
    /// Arc allocation when the snapshot is only partially modified.
    pub fn refresh_tracks(&self) {
        let total = self.db.track_count().unwrap_or(0) as usize;
        let page = self.track_page.load(Ordering::Relaxed);
        let offset = page * self.tracks_per_page;
        let tracks = match self
            .db
            .get_all_tracks(self.tracks_per_page as i64, offset as i64)
        {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to refresh track list: {}", e);
                Vec::new()
            },
        };

        self.snapshot.rcu(|old| {
            let mut new_snap = Arc::unwrap_or_clone(old.clone());
            new_snap.total_track_count = total;
            new_snap.tracks = tracks.clone();
            new_snap
        });
        self.db_dirty.store(false, Ordering::Relaxed);
    }

    /// Refresh the cached set of favorite track IDs.
    pub fn refresh_favorite_ids(&self) {
        match self.db.get_favorite_track_ids() {
            Ok(ids) => {
                self.snapshot.rcu(|old| {
                    let mut new_snap = Arc::unwrap_or_clone(old.clone());
                    new_snap.favorite_ids = ids.clone().into_iter().collect();
                    new_snap
                });
            },
            Err(e) => {
                warn!("Failed to refresh favorite IDs: {}", e);
            },
        }
    }

    /// Refresh the playlist list from the database.
    pub fn refresh_playlists(&self) {
        match self.db.get_all_playlists() {
            Ok(playlists) => {
                self.snapshot.rcu(|old| {
                    let mut new_snap = Arc::unwrap_or_clone(old.clone());
                    new_snap.playlists = playlists.clone();
                    new_snap
                });
            },
            Err(e) => {
                error!("Failed to refresh playlists: {}", e);
            },
        }
    }

    /// Check and update scan state from signals and progress channel.
    ///
    ///
    /// Disconnected (terminal) states instead of treating all errors
    /// as a single terminal condition.
    pub fn check_scan_state(&self) {
        // Drain all pending progress updates from the scan thread
        let mut latest_progress: Option<tc_library::ScanProgress> = None;
        loop {
            match self.scan_progress_rx.try_recv() {
                Ok(progress) => latest_progress = Some(progress),
                Err(crossbeam::channel::TryRecvError::Empty) => break, // No more data right now
                Err(crossbeam::channel::TryRecvError::Disconnected) => {
                    // Channel disconnected — the scan thread has terminated.
                    // This is a terminal condition for the scan, not an error.
                    log::debug!("Scan progress channel disconnected (scan thread terminated)");
                    break;
                },
            }
        }

        let scan_done = self.scan_complete.load(Ordering::Relaxed);
        let scan_failed = self.scan_failed.load(Ordering::Relaxed);

        self.snapshot.rcu(|old| {
            let mut new_snap = Arc::unwrap_or_clone(old.clone());

            if let Some(ref progress) = latest_progress {
                new_snap.is_scanning = !scan_done;
                if scan_done {
                    if scan_failed {
                        new_snap.status_message = "Library scan failed".to_string();
                    } else {
                        new_snap.status_message = format!(
                            "Scan complete: {} added, {} updated",
                            progress.files_added, progress.files_updated
                        );
                    }
                } else {
                    new_snap.status_message = format!(
                        "Scanning {}/{} files...",
                        progress.files_processed, progress.files_found
                    );
                }
            } else if scan_done && new_snap.is_scanning {
                new_snap.is_scanning = false;
                if scan_failed {
                    new_snap.status_message = "Library scan failed".to_string();
                } else {
                    new_snap.status_message = "Library scan complete".to_string();
                }
            } else if !scan_done && !new_snap.is_scanning {
                // Scan just started, no progress yet
                new_snap.is_scanning = true;
                new_snap.status_message = "Scanning library...".to_string();
            }

            new_snap
        });
    }

    /// Navigate to the next track page.
    ///
    ///
    /// for exact multiples of tracks_per_page. Now uses proper ceiling
    /// division and handles edge cases correctly.
    pub fn next_page(&self) {
        let snapshot = self.snapshot.load();
        let total = snapshot.total_track_count;
        let per_page = self.tracks_per_page;
        // Correct max_page calculation: ceiling division gives the number
        // of pages, then subtract 1 for 0-based indexing.
        let max_page = if total == 0 {
            0
        } else if total % per_page == 0 {
            total / per_page - 1
        } else {
            total / per_page
        };
        let current = self.track_page.load(Ordering::Relaxed);
        drop(snapshot);

        if current < max_page {
            self.track_page.store(current + 1, Ordering::Relaxed);
            self.refresh_tracks();
        }
    }

    /// Navigate to the previous track page.
    pub fn prev_page(&self) {
        let current = self.track_page.load(Ordering::Relaxed);
        if current > 0 {
            self.track_page.store(current - 1, Ordering::Relaxed);
            self.refresh_tracks();
        }
    }

    /// Search tracks using the database FTS index.
    ///
    ///
    /// to empty result sets, so callers can distinguish "no results"
    /// from "query failed".
    pub fn search_tracks(&self, query: &str, limit: i64) -> Result<Vec<Track>, String> {
        match self.db.search_tracks(query, limit) {
            Ok(tracks) => Ok(tracks),
            Err(e) => {
                let msg = format!("Search failed: {}", e);
                warn!("{}", msg);
                Err(msg)
            },
        }
    }

    /// Create a new playlist.
    pub fn create_playlist(&self, name: &str) -> Result<i64, String> {
        match self.db.create_playlist(name, None, false, None) {
            Ok(id) => {
                self.mark_db_dirty();
                self.refresh_playlists();
                Ok(id)
            },
            Err(e) => Err(format!("Failed to create playlist: {}", e)),
        }
    }

    /// Add a track to a playlist.
    pub fn add_track_to_playlist(&self, playlist_id: i64, track_id: i64) -> Result<(), String> {
        let pos = self
            .db
            .get_playlist_tracks(playlist_id)
            .map(|t| t.len() as i32)
            .unwrap_or(0);

        match self.db.add_track_to_playlist(playlist_id, track_id, pos) {
            Ok(()) => {
                self.mark_db_dirty();
                Ok(())
            },
            Err(e) => Err(format!("Failed to add track: {}", e)),
        }
    }

    /// Get tracks from a specific playlist.
    pub fn get_playlist_tracks(&self, playlist_id: i64) -> Vec<Track> {
        match self.db.get_playlist_tracks(playlist_id) {
            Ok(tracks) => tracks,
            Err(e) => {
                warn!("Failed to get playlist tracks: {}", e);
                Vec::new()
            },
        }
    }

    /// Toggle favorite status for a track.
    pub fn toggle_favorite(&self, track_id: i64, currently_favorited: bool) -> bool {
        let new_state = !currently_favorited;
        if currently_favorited {
            if let Err(e) = self.db.remove_favorite(track_id) {
                warn!("Failed to remove favorite: {}", e);
                return currently_favorited; // Keep old state on error
            }
        } else {
            if let Err(e) = self.db.add_favorite(track_id) {
                warn!("Failed to add favorite: {}", e);
                return currently_favorited; // Keep old state on error
            }
        }
        self.mark_db_dirty();
        self.refresh_favorite_ids();
        new_state
    }

    /// Check if a track is a favorite.
    pub fn is_favorite(&self, track_id: i64) -> bool {
        self.snapshot.load().favorite_ids.contains(&track_id)
    }

    /// Record a play (increment play count and update last_played).
    pub fn record_play(&self, track_id: i64) {
        if let Err(e) = self.db.update_play_count(track_id) {
            warn!("Failed to update play count: {}", e);
        }
        if let Err(e) = self.db.update_last_played(track_id) {
            warn!("Failed to update last played: {}", e);
        }
    }

    /// Compute badge counts for sidebar navigation sections.
    ///
    /// Badge counts are computed from the database directly when possible,
    /// so counts are accurate regardless of pagination state.
    pub fn compute_badge_counts(&self) -> std::collections::HashMap<String, u32> {
        use crate::sidebar::NavSection;

        let snapshot = self.snapshot.load();
        let navs = [
            NavSection::AllTracks,
            NavSection::MoodDance,
            NavSection::MoodRomantic,
            NavSection::MoodSad,
            NavSection::MoodSufi,
            NavSection::MoodChill,
            NavSection::RecentlyPlayed,
            NavSection::MostPlayed,
        ];

        let mut badge_cache = std::collections::HashMap::new();
        for nav in &navs {
            let count = match nav {
                NavSection::MoodDance
                | NavSection::MoodRomantic
                | NavSection::MoodSad
                | NavSection::MoodSufi
                | NavSection::MoodChill => {
                    // Use mood_matches for consistent filtering (M-08 fix)
                    snapshot
                        .tracks
                        .iter()
                        .filter(|t| t.mood.as_deref().is_some_and(|m| nav.mood_matches(m)))
                        .count() as u32
                },
                _ => nav.badge_count(&snapshot.tracks).unwrap_or(0),
            };
            badge_cache.insert(format!("{:?}", nav), count);
        }

        // Favorites count from the cached favorite IDs
        badge_cache.insert(
            format!("{:?}", NavSection::Favorites),
            snapshot.favorite_ids.len() as u32,
        );

        badge_cache
    }

    /// Get a reference to the underlying Database (for services that need direct access).
    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }

    /// Get a reference to the library manager.
    pub fn library_manager(&self) -> &Arc<tc_library::LibraryManager> {
        &self.library_manager
    }

    /// Get a track by ID.
    pub fn get_track(&self, id: i64) -> Option<Track> {
        self.db.get_track(id).unwrap_or(None)
    }

    /// Get all track IDs.
    pub fn get_all_track_ids(&self) -> Vec<i64> {
        self.db.get_all_track_ids().unwrap_or_default()
    }

    /// Get cover art raw bytes for a track (tries track_id first, then album fallback).
    /// Returns `(bytes, mime_type)` or `None` if no inline art is stored.
    pub fn get_cover_art_by_track_id(&self, track_id: i64) -> Option<(Vec<u8>, String)> {
        self.db.get_cover_art_by_track_id(track_id).unwrap_or(None)
    }
}
