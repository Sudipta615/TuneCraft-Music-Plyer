//! Lyrics service — lyrics search, caching, and async result handling
//!
//! Encapsulates lyrics fetching logic, removing it from TuneCraftApp
//! and providing a clean async interface with result polling.
//!
//! Consistent with PlaybackService and EqService. Uses the
//! standardized `recover_from_poison` pattern from config.rs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use log::{info, warn};
use tokio::runtime::Runtime;

use super::config::{recover_from_poison, recover_from_poison_write};

/// Lyrics state managed by the service.
pub struct LyricsState {
    /// Whether the lyrics panel is visible
    pub show_panel: bool,
    /// Current lyrics (if loaded)
    pub current_lyrics: Option<Vec<tc_lyrics::SyncedLyricLine>>,
    /// Whether lyrics are currently being fetched
    pub loading: bool,
    /// The track ID for the currently loaded lyrics (used for DB caching)
    pub current_track_id: Option<i64>,
    /// Result sink from the async fetch task
    result: Arc<std::sync::Mutex<Option<Result<Vec<tc_lyrics::SyncedLyricLine>, String>>>>,
}

impl Default for LyricsState {
    fn default() -> Self {
        Self {
            show_panel: false,
            current_lyrics: None,
            loading: false,
            current_track_id: None,
            result: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

/// The lyrics service manages lyrics search and display.

/// H12: A monotonic fetch-generation counter (`fetch_gen`) is shared between
/// `fetch()` and spawned async tasks via `Arc<AtomicU64>`. Each call to
/// `fetch()` increments the counter. Spawned tasks capture the counter value
/// at spawn time and discard their result if a newer fetch has since started,
/// preventing a slow/stale HTTP response from overwriting fresh lyrics.
pub struct LyricsService {
    client: Arc<tc_lyrics::LyricsClient>,
    state: std::sync::RwLock<LyricsState>,
    tokio_runtime: Arc<Runtime>,
    /// Shared generation counter. Incremented by fetch(), read by tasks.
    fetch_gen: Arc<AtomicU64>,
}

impl LyricsService {
    /// Create a new LyricsService.
    pub fn new(client: Arc<tc_lyrics::LyricsClient>, tokio_runtime: Arc<Runtime>) -> Self {
        Self {
            client,
            state: std::sync::RwLock::new(LyricsState::default()),
            tokio_runtime,
            fetch_gen: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get a reference to the lyrics state.
    pub fn state(&self) -> std::sync::RwLockReadGuard<'_, LyricsState> {
        recover_from_poison(self.state.read())
    }

    /// Get a mutable reference to the lyrics state.
    pub fn state_mut(&self) -> std::sync::RwLockWriteGuard<'_, LyricsState> {
        recover_from_poison_write(self.state.write())
    }

    /// Toggle the lyrics panel visibility.
    pub fn toggle_panel(&self) {
        let mut state = self.state_mut();
        state.show_panel = !state.show_panel;
    }

    /// Fetch lyrics for a track (async, non-blocking).
    ///
    /// H12: Increments the shared `fetch_gen` counter and passes the captured
    /// value into the async task. If the task's captured generation is no longer
    /// the current generation when the HTTP response arrives, the result is
    /// silently discarded instead of overwriting more recent lyrics.
    pub fn fetch(&self, artist: &str, title: &str, track_id: i64) {
        let client = Arc::clone(&self.client);
        let fetch_gen = Arc::clone(&self.fetch_gen);

        // Atomically bump the generation. SeqCst ensures the store is visible
        // to the async task before it reads back the counter.
        let my_generation = fetch_gen.fetch_add(1, Ordering::SeqCst) + 1;

        let result_sink = {
            let mut state = self.state_mut();
            state.loading = true;
            state.current_lyrics = None;
            state.current_track_id = Some(track_id);
            // Clear any pending stale result from a previous fetch.
            *state.result.lock().unwrap_or_else(|e| e.into_inner()) = None;
            Arc::clone(&state.result)
        };

        let artist = artist.to_string();
        let title = title.to_string();

        self.tokio_runtime.spawn(async move {
            let outcome = match client.search(&artist, &title).await {
                Ok(result) => {
                    if let Some(synced) = result.synced {
                        info!("Found {} synced lyric lines", synced.len());
                        Ok(synced)
                    } else if let Some(unsynced) = result.unsynced {
                        info!("Found unsynced lyrics ({} chars)", unsynced.len());
                        let lines: Vec<tc_lyrics::SyncedLyricLine> = unsynced
                            .lines()
                            .map(|line| tc_lyrics::SyncedLyricLine {
                                timestamp_ms: 0,
                                text: line.to_string(),
                            })
                            .collect();
                        Ok(lines)
                    } else {
                        Err("No lyrics found".to_string())
                    }
                }
                Err(e) => {
                    warn!("Lyrics search failed: {}", e);
                    Err(format!("Lyrics search failed: {}", e))
                }
            };

            // H12: Only commit the result if no newer fetch() has started
            // since we were spawned. If fetch_gen has advanced past our
            // captured generation, discard this stale result.
            let current_gen = fetch_gen.load(Ordering::SeqCst);
            if current_gen == my_generation {
                *result_sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
            } else {
                info!(
                    "Discarding stale lyrics result (task gen={}, current gen={})",
                    my_generation, current_gen
                );
            }
        });
    }

    /// Poll for async lyrics results (called every frame from UI update).
    pub fn poll_results(&self) -> bool {
        let result = {
            let state = self.state();
            let result_sink = Arc::clone(&state.result);
            drop(state);
            let mut guard = result_sink.lock().unwrap_or_else(|e| e.into_inner());
            guard.take()
        };

        if let Some(result) = result {
            let mut state = self.state_mut();
            state.loading = false;
            match result {
                Ok(lines) => {
                    state.current_lyrics = Some(lines);
                    true
                }
                Err(_e) => {
                    state.current_lyrics = None;
                    true
                }
            }
        } else {
            false
        }
    }
}
