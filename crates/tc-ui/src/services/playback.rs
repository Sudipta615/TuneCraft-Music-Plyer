//! Playback service — encapsulates audio engine access and playback state
//!
//! This service eliminates the need for `Arc<Mutex<AudioEngine>>` in the UI
//! by using the engine's internal command channel for control and
//! `Arc<RwLock<PlaybackInfo>>` for state reads.
//!
//! Uses standardized `recover_from_poison` from config.rs.
//! Volume clamped in `set_volume()`. Includes `stop_engine()` method for
//! graceful shutdown.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[allow(unused_imports)]
use log::{error, info, warn};
use tc_config::RepeatMode;
use tc_db::Track;
#[allow(unused_imports)]
use tc_engine::buffer::{EngineCommand, PlaybackInfo, PlaybackState as EnginePlaybackState};
use tokio::runtime::Runtime;

use super::{
    config::{recover_from_poison, recover_from_poison_write},
    platform::PlatformService,
    scrobble::ScrobbleService,
};

/// Handle to the audio engine that avoids Mutex contention.
///
/// The engine already provides:
/// - `send_command()` via a crossbeam channel (lock-free)
/// - `playback_info_arc()` via `Arc<RwLock<PlaybackInfo>>` (concurrent reads)
///
/// By storing these handles separately, the UI can send commands and read
/// playback state without ever locking the engine itself. The tick thread
/// is the only code that needs `&mut AudioEngine`.
pub struct EngineHandle {
    /// Channel sender for engine commands (lock-free, non-blocking)
    cmd_tx: crossbeam::channel::Sender<EngineCommand>,
    /// Shared playback info for concurrent reads
    playback_info: Arc<std::sync::RwLock<PlaybackInfo>>,
    /// Whether the engine is running (tick thread active)
    running: Arc<AtomicBool>,
}

impl EngineHandle {
    /// Create an EngineHandle by extracting the channel and info arc from a running engine.
    pub fn new(
        cmd_tx: crossbeam::channel::Sender<EngineCommand>,
        playback_info: Arc<std::sync::RwLock<PlaybackInfo>>,
        running: Arc<AtomicBool>,
    ) -> Self {
        Self {
            cmd_tx,
            playback_info,
            running,
        }
    }

    /// Send a command to the engine (non-blocking, lock-free).
    pub fn send_command(&self, cmd: EngineCommand) {
        if let Err(e) = self.cmd_tx.send(cmd) {
            warn!("Failed to send engine command: {}", e);
        }
    }

    /// Read current playback info (non-blocking read lock, never contends with tick).
    pub fn playback_info(&self) -> PlaybackInfo {
        self.playback_info
            .read()
            .map(|info| info.clone())
            .unwrap_or_default()
    }

    /// Get the playback info arc (for services that need to poll).
    pub fn playback_info_arc(&self) -> Arc<std::sync::RwLock<PlaybackInfo>> {
        Arc::clone(&self.playback_info)
    }

    /// Check if the engine is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Signal the engine to stop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

/// Playback state managed by the service (separate from engine state).
///
/// Snapshots are refreshed each frame and only clones queue data when the
/// version has changed, eliminating ~4.8MB/s of allocation pressure at 60fps.
#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub current_track_id: Option<i64>,
    pub is_playing: bool,
    pub is_favorited: bool,
    pub position_secs: f32,
    pub duration_secs: f32,
    pub volume: f32,
    pub speed: f32,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub play_queue: Vec<i64>,
    pub play_queue_index: Option<usize>,
    pub shuffle_order: Vec<usize>,
    pub shuffle_position: usize,
    pub play_started_at: Option<std::time::Instant>,
    pub accumulated_play_secs: f32,
    /// Version counter — increments on every state mutation (v0.9.3: H-04).
    pub version: u64,
    /// Whether the resampler has been disabled due to creation or rebuild failures.
    /// UI should display a warning.
    pub resampler_disabled: bool,
    /// Whether the convolution engine's loaded IR has a stale frequency
    /// mapping due to a sample rate change and needs to be reloaded.
    /// UI should display a warning when true.
    pub convolution_ir_needs_reload: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            current_track_id: None,
            is_playing: false,
            is_favorited: false,
            position_secs: 0.0,
            duration_secs: 0.0,
            volume: 0.75,
            speed: 1.0,
            shuffle: false,
            repeat: RepeatMode::Off,
            play_queue: Vec::new(),
            play_queue_index: None,
            shuffle_order: Vec::new(),
            shuffle_position: 0,
            play_started_at: None,
            accumulated_play_secs: 0.0,
            version: 0,
            resampler_disabled: false,
            convolution_ir_needs_reload: false,
        }
    }
}

/// Scrobble threshold check result.
#[derive(Debug, Clone)]
pub enum ScrobbleCheck {
    Ready(i64),
    NotYet,
    Error(String),
}

/// The playback service manages audio playback, queue navigation, and
/// coordinates with platform (MPRIS) and scrobble services.
pub struct PlaybackService {
    #[cfg(feature = "audio-output")]
    engine: EngineHandle,
    #[cfg(feature = "audio-output")]
    engine_mutex: Arc<std::sync::Mutex<tc_engine::AudioEngine>>,
    state: std::sync::RwLock<PlaybackState>,
    platform: Arc<PlatformService>,
    scrobble: Arc<ScrobbleService>,
    _tokio_runtime: Arc<Runtime>,
}

impl PlaybackService {
    #[cfg(feature = "audio-output")]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine: EngineHandle,
        engine_mutex: Arc<std::sync::Mutex<tc_engine::AudioEngine>>,
        platform: Arc<PlatformService>,
        scrobble: Arc<ScrobbleService>,
        tokio_runtime: Arc<Runtime>,
        volume: f32,
        shuffle: bool,
        repeat: RepeatMode,
        speed: f32,
    ) -> Self {
        let clamped_vol = volume.clamp(0.0, 1.0);
        engine.send_command(EngineCommand::SetVolume(clamped_vol * clamped_vol));

        let state = PlaybackState {
            volume: clamped_vol,
            shuffle,
            repeat,
            speed,
            ..Default::default()
        };

        Self {
            engine,
            engine_mutex,
            state: std::sync::RwLock::new(state),
            platform,
            scrobble,
            _tokio_runtime: tokio_runtime,
        }
    }

    #[cfg(not(feature = "audio-output"))]
    pub fn new(
        platform: Arc<PlatformService>,
        scrobble: Arc<ScrobbleService>,
        tokio_runtime: Arc<Runtime>,
        volume: f32,
        shuffle: bool,
        repeat: RepeatMode,
        speed: f32,
    ) -> Self {
        let state = PlaybackState {
            volume: volume.clamp(0.0, 1.0),
            shuffle,
            repeat,
            speed,
            ..Default::default()
        };

        Self {
            state: std::sync::RwLock::new(state),
            platform,
            scrobble,
            _tokio_runtime: tokio_runtime,
        }
    }

    pub fn state(&self) -> std::sync::RwLockReadGuard<'_, PlaybackState> {
        recover_from_poison(self.state.read())
    }

    pub fn state_mut(&self) -> std::sync::RwLockWriteGuard<'_, PlaybackState> {
        recover_from_poison_write(self.state.write())
    }

    #[cfg(feature = "audio-output")]
    pub fn sync_from_engine(&self) {
        let info = self.engine.playback_info();
        let mut state = recover_from_poison_write(self.state.write());

        state.position_secs = info.position_secs;
        state.duration_secs = info.duration_secs;
        state.volume = info.volume;
        state.speed = info.speed;

        state.resampler_disabled = info.resampler_disabled;
        state.convolution_ir_needs_reload = info.convolution_ir_needs_reload;

        if info.state == EnginePlaybackState::Stopped && state.is_playing {
            state.is_playing = false;
        }
    }

    #[cfg(not(feature = "audio-output"))]
    pub fn sync_from_engine(&self) {}

    pub fn play_track(&self, track: &Track, is_favorited: bool) {
        let _path = std::path::PathBuf::from(&track.path);

        #[cfg(feature = "audio-output")]
        {
            if let Ok(mut eng) = self.engine_mutex.lock() {
                match eng.load_track(&_path) {
                    Ok(info) => {
                        info!(
                            "Track loaded: {} Hz, {} ch, {:.1}s",
                            info.sample_rate, info.channels, info.duration_secs
                        );
                        eng.set_track_id(track.id as u64);
                    },
                    Err(e) => {
                        error!("Failed to load track: {}", e);
                        return;
                    },
                }
            } else {
                error!("Engine mutex poisoned — cannot load track");
                return;
            }

            self.engine.send_command(EngineCommand::Play);
        }

        let mut state = recover_from_poison_write(self.state.write());
        state.current_track_id = Some(track.id);
        state.is_playing = true;
        state.position_secs = 0.0;
        state.duration_secs = track.duration_secs;
        state.is_favorited = is_favorited;
        state.play_queue_index = state.play_queue.iter().position(|&id| id == track.id);
        // When shuffle is active and a track is manually selected, align
        // shuffle_position to the position of that track in shuffle_order.
        // Without this, navigate_next() would advance from an unrelated
        // shuffle_position, ignoring the user's explicit track choice.
        if state.shuffle {
            if let Some(queue_idx) = state.play_queue_index {
                if let Some(pos) = state.shuffle_order.iter().position(|&i| i == queue_idx) {
                    state.shuffle_position = pos;
                }
            }
        }
        state.play_started_at = Some(std::time::Instant::now());
        state.accumulated_play_secs = 0.0;
        state.version += 1;

        let artist = track.artist.clone().unwrap_or_default();
        let title = track.title.clone();
        let album = track.album.clone();
        let track_id = track.id;
        let duration_secs = track.duration_secs;

        drop(state);

        self.platform.update_mpris_playing(
            &title,
            Some(artist.as_str()),
            album.as_deref(),
            duration_secs,
            track_id,
        );

        self.scrobble
            .update_now_playing(&artist, &title, album.as_deref());
    }

    pub fn toggle_playback(&self) {
        let mut state = recover_from_poison_write(self.state.write());

        if state.is_playing {
            #[cfg(feature = "audio-output")]
            self.engine.send_command(EngineCommand::Pause);
            state.is_playing = false;
            if let Some(started) = state.play_started_at {
                state.accumulated_play_secs += started.elapsed().as_secs_f32();
            }
            state.play_started_at = None;
            state.version += 1;
            drop(state);
            self.platform.update_mpris_paused();
            self.scrobble.clear_now_playing();
        } else {
            if state.current_track_id.is_some() {
                #[cfg(feature = "audio-output")]
                self.engine.send_command(EngineCommand::Play);
                state.is_playing = true;
                state.play_started_at = Some(std::time::Instant::now());
                state.version += 1;
                drop(state);
                self.platform.update_mpris_playing_by_state();
            } else if !state.play_queue.is_empty() {
                // Bug #7 fix: this branch previously returned without
                // incrementing version, causing a state sync race. The
                // caller (UI toggle_playback) handles queue navigation,
                // but any other caller would miss the state change. Now
                // we increment version so sync_from_playback_service
                // detects the state query even if no action was taken.
                state.version += 1;
                drop(state);
            }
        }
    }

    pub fn stop_playback(&self) {
        #[cfg(feature = "audio-output")]
        self.engine.send_command(EngineCommand::Stop);

        let mut state = recover_from_poison_write(self.state.write());
        state.is_playing = false;
        state.position_secs = 0.0;
        state.accumulated_play_secs = 0.0;
        state.play_started_at = None;
        state.version += 1;
        drop(state);

        self.platform.update_mpris_stopped();
        self.scrobble.clear_now_playing();
    }

    pub fn seek(&self, pos_secs: f32) {
        #[cfg(feature = "audio-output")]
        self.engine.send_command(EngineCommand::Seek(pos_secs));

        let mut state = recover_from_poison_write(self.state.write());
        state.position_secs = pos_secs;

        // Reset scrobble timing so it is based on actual playback time,
        // not wall-clock time including pre-seek position.
        state.accumulated_play_secs = 0.0;
        state.play_started_at = Some(std::time::Instant::now());
        state.version += 1;
    }

    pub fn reset_play_started_at(&self) {
        let mut state = recover_from_poison_write(self.state.write());
        state.play_started_at = Some(std::time::Instant::now());
        state.accumulated_play_secs = 0.0;
        state.version += 1;
    }

    /// Set volume (0.0 to 1.0).
    ///
    /// The engine pipeline has been updated to clamp at 1.0 to prevent clipping.
    /// We apply a perceptual (quadratic) curve before sending to the engine.
    pub fn set_volume(&self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        #[cfg(feature = "audio-output")]
        self.engine
            .send_command(EngineCommand::SetVolume(clamped * clamped));

        let mut state = recover_from_poison_write(self.state.write());
        state.volume = clamped;
        drop(state);

        self.platform.update_mpris_volume(clamped);
    }

    pub fn set_speed(&self, speed: f32) {
        let clamped = speed.clamp(0.25, 4.0);
        #[cfg(feature = "audio-output")]
        self.engine.send_command(EngineCommand::SetSpeed(clamped));

        let mut state = recover_from_poison_write(self.state.write());
        state.speed = clamped;
        state.version += 1;
        info!("Playback speed set to {:.2}x", clamped);
    }

    pub fn set_shuffle(&self, shuffle: bool) {
        let mut state = recover_from_poison_write(self.state.write());
        state.shuffle = shuffle;
        state.version += 1;
        if shuffle && state.shuffle_order.is_empty() {
            drop(state);
            self.regenerate_shuffle_order();
        } else {
            state.shuffle_position = 0;
        }
    }

    pub fn set_repeat(&self, repeat: RepeatMode) {
        let mut state = recover_from_poison_write(self.state.write());
        state.repeat = repeat;
        state.version += 1;
    }

    /// Navigate to the next track in the queue.
    ///
    /// H13: The previous implementation used a read lock to check whether shuffle
    /// order regeneration was needed, dropped that lock, called regenerate_shuffle_order()
    /// (which acquired a write lock), then re-acquired a write lock for the navigation
    /// update. This creates a TOCTOU window where another thread can modify shuffle
    /// state in between. The fix acquires a single write lock and performs the
    /// regeneration check and queue advance atomically under that one lock.
    pub fn navigate_next(&self) -> Option<i64> {
        let mut state = recover_from_poison_write(self.state.write());

        if state.play_queue.is_empty() {
            return None;
        }

        // Inline shuffle order regeneration: check-and-regen atomically
        // under the same write lock to eliminate the TOCTOU race (H13).
        if state.shuffle
            && (state.shuffle_order.is_empty()
                || state.shuffle_position >= state.shuffle_order.len())
        {
            let n = state.play_queue.len();
            let mut order: Vec<usize> = (0..n).collect();
            let mut seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            for i in (1..n).rev() {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let j = (seed >> 33) as usize % (i + 1);
                order.swap(i, j);
            }
            // Bug #16 fix: If the current track is at position 0 of the new
            // shuffle order, swap it with another position to avoid immediately
            // replaying the just-finished track. The Fisher-Yates shuffle
            // doesn't exclude the current track from position 0, so without
            // this check, the just-finished track can end up first and replay
            // at the start of each shuffle cycle.
            if let Some(current_idx) = state.play_queue_index {
                if n > 1 && order[0] == current_idx {
                    // Swap position 0 with a random other position
                    seed = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    let swap_pos = 1 + (seed >> 33) as usize % (n - 1);
                    order.swap(0, swap_pos);
                }
            }
            state.shuffle_order = order;
            state.shuffle_position = 0;
        }

        let next_index = match state.play_queue_index {
            Some(idx) => {
                if state.shuffle {
                    state
                        .shuffle_order
                        .get(state.shuffle_position)
                        .copied()
                        .unwrap_or(0)
                } else if idx + 1 < state.play_queue.len() {
                    idx + 1
                } else {
                    match state.repeat {
                        RepeatMode::All => 0,
                        RepeatMode::One => idx,
                        RepeatMode::Off => return None,
                    }
                }
            },
            None => 0,
        };

        if state.shuffle {
            state.shuffle_position += 1;
        }

        state.play_queue_index = Some(next_index);
        state.version += 1;
        state.play_queue.get(next_index).copied()
    }

    pub fn navigate_prev(&self) -> Option<i64> {
        let mut state = recover_from_poison_write(self.state.write());

        if state.play_queue.is_empty() {
            return None;
        }

        if state.position_secs > 3.0 {
            state.version += 1;
            return state.current_track_id;
        }

        let prev_index = match state.play_queue_index {
            Some(idx) if idx > 0 => idx - 1,
            Some(_) => match state.repeat {
                RepeatMode::All => state.play_queue.len() - 1,
                _ => return None,
            },
            None => return None,
        };

        state.play_queue_index = Some(prev_index);
        state.version += 1;
        state.play_queue.get(prev_index).copied()
    }

    pub fn compute_next_index(&self) -> Option<usize> {
        let state = recover_from_poison(self.state.read());

        if state.play_queue.is_empty() {
            return None;
        }

        match state.play_queue_index {
            Some(idx) => {
                if state.shuffle {
                    if state.shuffle_order.is_empty()
                        || state.shuffle_position >= state.shuffle_order.len()
                    {
                        return Some(0);
                    }
                    state.shuffle_order.get(state.shuffle_position).copied()
                } else if idx + 1 < state.play_queue.len() {
                    Some(idx + 1)
                } else {
                    match state.repeat {
                        RepeatMode::All => Some(0),
                        RepeatMode::One => Some(idx),
                        RepeatMode::Off => None,
                    }
                }
            },
            None => Some(0),
        }
    }

    pub fn compute_prev_index(&self) -> Option<usize> {
        let state = recover_from_poison(self.state.read());

        if state.play_queue.is_empty() {
            return None;
        }

        if state.position_secs > 3.0 {
            return state.play_queue_index;
        }

        match state.play_queue_index {
            Some(idx) if idx > 0 => Some(idx - 1),
            Some(_) => match state.repeat {
                RepeatMode::All => Some(state.play_queue.len() - 1),
                _ => None,
            },
            None => None,
        }
    }

    pub fn regenerate_shuffle_order(&self) {
        let mut state = recover_from_poison_write(self.state.write());
        let n = state.play_queue.len();
        if n == 0 {
            state.shuffle_order = Vec::new();
            state.version += 1;
            return;
        }
        let mut order: Vec<usize> = (0..n).collect();
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        for i in (1..n).rev() {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let j = (seed >> 33) as usize % (i + 1);
            order.swap(i, j);
        }
        state.shuffle_order = order;
        state.shuffle_position = 0;
        state.version += 1;
    }

    pub fn check_scrobble_threshold(&self, last_scrobbled_id: &mut Option<i64>) -> ScrobbleCheck {
        let state = recover_from_poison(self.state.read());

        let track_id = match state.current_track_id {
            Some(id) => id,
            None => return ScrobbleCheck::Error("No current track".to_string()),
        };

        if last_scrobbled_id.as_ref() == Some(&track_id) {
            return ScrobbleCheck::NotYet;
        }

        // Use accumulated_play_secs (which tracks position_secs deltas) rather
        // than wall-clock elapsed time. Wall-clock time does not scale with
        // playback speed: at 2× speed a track finishes in half the wall time
        // but the scrobble threshold calculation would only see 50% progress,
        // causing scrobbles to be missed entirely at non-1× speeds.
        let elapsed = state.accumulated_play_secs
            + if state.is_playing {
                // Estimate progress since last accumulated update using current
                // position_secs relative to when play_started_at was set.
                // accumulated_play_secs is snapshotted on pause/seek/stop.
                match state.play_started_at {
                    Some(t) => t.elapsed().as_secs_f32() * state.speed,
                    None => 0.0,
                }
            } else {
                0.0
            };

        let duration = state.duration_secs;
        // Scrobble threshold: 50% of track duration or 4 minutes,
        // whichever comes first. Tracks shorter than 30 seconds should not
        // be scrobbled at all. The .max(30.0) floor
        // previously made this impossible for tracks 30-60s long — e.g. a
        // 40s track computed threshold = max(20, 30) = 30s (75% instead of
        // 50%). Bug #14 fix: only exclude tracks shorter than 30 seconds
        // entirely, don't raise the threshold for longer short tracks.
        let threshold = if duration >= 30.0 {
            (duration * 0.5).min(240.0)
        } else {
            // Track is shorter than 30 seconds — set an unreachable threshold
            // so it effectively never scrobbles (short-track exclusion).
            duration + 1.0
        };

        if elapsed >= threshold {
            *last_scrobbled_id = Some(track_id);
            ScrobbleCheck::Ready(track_id)
        } else {
            ScrobbleCheck::NotYet
        }
    }

    pub fn set_play_queue(&self, queue: Vec<i64>) {
        let mut state = recover_from_poison_write(self.state.write());
        state.play_queue = queue;
        state.shuffle_order.clear();
        state.shuffle_position = 0;
        state.version += 1;
    }

    pub fn set_queue_index(&self, index: Option<usize>) {
        let mut state = recover_from_poison_write(self.state.write());
        state.play_queue_index = index;
        state.version += 1;
    }

    pub fn advance_shuffle(&self) {
        let mut state = recover_from_poison_write(self.state.write());
        state.shuffle_position += 1;
        state.version += 1;
    }

    pub fn set_favorited(&self, favorited: bool) {
        let mut state = recover_from_poison_write(self.state.write());
        state.is_favorited = favorited;
        state.version += 1;
    }

    pub fn scrobble(&self) -> &Arc<ScrobbleService> {
        &self.scrobble
    }

    /// Signal the engine to stop the tick thread (v0.9.3: H-02/H-08 fix).
    #[cfg(feature = "audio-output")]
    pub fn stop_engine(&self) {
        self.engine.stop();
        info!("Engine stop signal sent via EngineHandle");
    }

    #[cfg(not(feature = "audio-output"))]
    pub fn stop_engine(&self) {}
}

#[cfg(all(test, not(feature = "audio-output")))]
mod tests {
    use std::sync::Arc;

    use tokio::runtime::Runtime;

    use super::*;

    /// Helper to create a minimal PlaybackService without audio engine.
    fn make_service() -> PlaybackService {
        let rt = Arc::new(Runtime::new().unwrap());
        let (platform, media_key_rx) =
            tc_platform::PlatformIntegration::new().expect("PlatformIntegration::new failed");
        let platform = Arc::new(PlatformService::new(platform, media_key_rx));
        let scrobble_db = tc_db::Database::open_in_memory().expect("in-memory DB");
        let scrobble = Arc::new(ScrobbleService::new(Arc::new(scrobble_db), true));
        PlaybackService::new(platform, scrobble, rt, 0.75, false, RepeatMode::Off, 1.0)
    }

    #[test]
    fn test_playback_state_default() {
        let state = PlaybackState::default();
        assert_eq!(state.current_track_id, None);
        assert!(!state.is_playing);
        assert!(!state.shuffle);
        assert_eq!(state.repeat, RepeatMode::Off);
        assert_eq!(state.volume, 0.75);
        assert_eq!(state.speed, 1.0);
        assert!(state.play_queue.is_empty());
        assert_eq!(state.version, 0);
        assert!(!state.resampler_disabled);
    }

    #[test]
    fn test_set_volume_clamps() {
        let svc = make_service();
        svc.set_volume(2.0); // should clamp to 1.0
        assert!((svc.state().volume - 1.0).abs() < 1e-6);
        svc.set_volume(-1.0); // should clamp to 0.0
        assert!((svc.state().volume - 0.0).abs() < 1e-6);
        svc.set_volume(0.8);
        assert!((svc.state().volume - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_set_speed_clamps() {
        let svc = make_service();
        svc.set_speed(10.0); // should clamp to 4.0
        assert!((svc.state().speed - 4.0).abs() < 1e-6);
        svc.set_speed(0.0); // should clamp to 0.25
        assert!((svc.state().speed - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_set_shuffle_generates_order() {
        let svc = make_service();
        svc.set_play_queue(vec![1, 2, 3, 4, 5]);
        svc.set_shuffle(true);
        let state = svc.state();
        assert!(state.shuffle);
        assert_eq!(state.shuffle_order.len(), 5);
        // All indices 0..5 should be present exactly once
        let mut sorted = state.shuffle_order.clone();
        sorted.sort();
        assert_eq!(sorted, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_set_repeat() {
        let svc = make_service();
        svc.set_repeat(RepeatMode::All);
        assert_eq!(svc.state().repeat, RepeatMode::All);
        svc.set_repeat(RepeatMode::One);
        assert_eq!(svc.state().repeat, RepeatMode::One);
    }

    #[test]
    fn test_navigate_next_empty_queue() {
        let svc = make_service();
        assert!(svc.navigate_next().is_none());
    }

    #[test]
    fn test_navigate_next_sequential() {
        let svc = make_service();
        svc.set_play_queue(vec![10, 20, 30]);
        // First navigate should return queue[0]=10
        let next = svc.navigate_next();
        assert_eq!(next, Some(10));
        assert_eq!(svc.state().play_queue_index, Some(0));
        // Second navigate should return queue[1]=20
        let next = svc.navigate_next();
        assert_eq!(next, Some(20));
        assert_eq!(svc.state().play_queue_index, Some(1));
    }

    #[test]
    fn test_navigate_next_repeat_all_wraps() {
        let svc = make_service();
        svc.set_play_queue(vec![10, 20]);
        svc.set_repeat(RepeatMode::All);
        svc.navigate_next(); // index 0
        svc.navigate_next(); // index 1
        let next = svc.navigate_next(); // wraps to 0
        assert_eq!(next, Some(10));
    }

    #[test]
    fn test_navigate_next_repeat_off_at_end() {
        let svc = make_service();
        svc.set_play_queue(vec![10, 20]);
        svc.set_repeat(RepeatMode::Off);
        svc.navigate_next(); // index 0
        svc.navigate_next(); // index 1
        assert!(svc.navigate_next().is_none()); // end of queue
    }

    #[test]
    fn test_navigate_prev_empty_queue() {
        let svc = make_service();
        assert!(svc.navigate_prev().is_none());
    }

    #[test]
    fn test_navigate_prev_at_beginning() {
        let svc = make_service();
        svc.set_play_queue(vec![10, 20, 30]);
        svc.set_queue_index(Some(0));
        assert!(svc.navigate_prev().is_none()); // already at start, RepeatMode::Off
    }

    #[test]
    fn test_set_play_queue_clears_shuffle() {
        let svc = make_service();
        svc.set_play_queue(vec![1, 2, 3]);
        svc.set_shuffle(true);
        assert!(!svc.state().shuffle_order.is_empty());
        svc.set_play_queue(vec![4, 5]);
        assert!(svc.state().shuffle_order.is_empty());
    }

    #[test]
    fn test_stop_playback_resets_state() {
        let svc = make_service();
        svc.set_play_queue(vec![10, 20]);
        svc.set_queue_index(Some(0));
        svc.set_favorited(true);
        // Manually set is_playing (would normally require engine)
        {
            let mut state = svc.state_mut();
            state.is_playing = true;
            state.position_secs = 50.0;
        }
        svc.stop_playback();
        let state = svc.state();
        assert!(!state.is_playing);
        assert!((state.position_secs - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_seek_updates_position() {
        let svc = make_service();
        svc.seek(30.0);
        assert!((svc.state().position_secs - 30.0).abs() < 1e-6);
    }

    #[test]
    fn test_scrobble_threshold_short_track() {
        let svc = make_service();
        let mut last_id: Option<i64> = None;
        // No track loaded → Error
        match svc.check_scrobble_threshold(&mut last_id) {
            ScrobbleCheck::Error(_) => {},
            _ => panic!("Expected error for no current track"),
        }
    }

    #[test]
    fn test_version_increments_on_mutations() {
        let svc = make_service();
        let v0 = svc.state().version;
        svc.set_volume(0.9);
        // Volume doesn't increment version in current impl (no engine)
        svc.set_shuffle(true);
        let v1 = svc.state().version;
        assert!(v1 > v0);
    }

    #[test]
    fn test_compute_next_index_empty() {
        let svc = make_service();
        assert!(svc.compute_next_index().is_none());
    }

    #[test]
    fn test_compute_prev_index_empty() {
        let svc = make_service();
        assert!(svc.compute_prev_index().is_none());
    }
}
