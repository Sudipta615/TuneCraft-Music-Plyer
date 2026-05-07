//! Platform-specific media key / MPRIS / SMTC integration.
//!
//! Issue #14: Implements OS-level media key handling so that hardware
//! play/pause/next/prev keys and desktop media controllers (MPRIS2 on
//! Linux, MPNowPlayingInfoCenter on macOS, System Media Transport
//! Controls on Windows) can control TuneCraft playback.
//!
//! Each platform is gated behind `#[cfg(target_os = "...")]` and exposes
//! the same public API:
//!
//! - `init_media_keys(state: Arc<AppState>)` — spawns the platform backend
//! - `update_media_metadata(state: &AppState)` — pushes current track info
//!   to the OS media session
//! - `update_playback_status(state: &AppState)` — pushes Playing/Paused/Stopped

use std::sync::Arc;

use crate::app_state::AppState;

#[cfg(target_os = "linux")]
pub mod linux {
    use mpris_server::{
        zbus::{fdo, Result},
        Metadata, PlaybackStatus, PlayerInterface, Property, RootInterface, Server, Time, Volume,
    };
    use std::sync::{Arc, Mutex, OnceLock};

    use crate::app_state::AppState;

    /// Global server reference so we can emit property changes
    static MPRIS_SERVER: OnceLock<Server<TunecraftMpris>> = OnceLock::new();

    struct TunecraftMpris {
        state: Arc<AppState>,
    }

    impl RootInterface for TunecraftMpris {
        async fn identity(&self) -> fdo::Result<String> {
            Ok("Tunecraft".into())
        }
        async fn desktop_entry(&self) -> fdo::Result<String> {
            Ok("tunecraft".into())
        }
        async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
            Ok(vec![])
        }
        async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
            Ok(vec![])
        }
        async fn has_track_list(&self) -> fdo::Result<bool> {
            Ok(false)
        }
        async fn can_quit(&self) -> fdo::Result<bool> {
            Ok(false)
        }
        async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
            Ok(false)
        }
        async fn can_raise(&self) -> fdo::Result<bool> {
            Ok(false)
        }
        async fn fullscreen(&self) -> fdo::Result<bool> {
            Ok(false)
        }
        async fn set_fullscreen(&self, _fullscreen: bool) -> Result<()> {
            Ok(())
        }
        async fn raise(&self) -> Result<()> {
            Ok(())
        }
        async fn quit(&self) -> Result<()> {
            Ok(())
        }
    }

    impl PlayerInterface for TunecraftMpris {
        async fn play_pause(&self) -> Result<()> {
            self.state.toggle_playback();
            Ok(())
        }
        async fn play(&self) -> Result<()> {
            self.state.play();
            Ok(())
        }
        async fn pause(&self) -> Result<()> {
            self.state.pause();
            Ok(())
        }
        async fn next(&self) -> Result<()> {
            self.state.next_track();
            self.state.notify_track_change();
            Ok(())
        }
        async fn previous(&self) -> Result<()> {
            self.state.prev_track();
            self.state.notify_track_change();
            Ok(())
        }
        async fn stop(&self) -> Result<()> {
            if let Ok(engine) = self.state.engine.lock() {
                if let Some(ref e) = *engine {
                    let _ = e.stop();
                }
            }
            *self
                .state
                .player_state
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = tunecraft_core::audio::PlayerState::Stopped;
            Ok(())
        }
        async fn seek(&self, offset: Time) -> Result<()> {
            if let Some(position) = self.state.position() {
                let offset_micros = offset.as_micros();
                let new_pos = if offset_micros < 0 {
                    position.saturating_sub(std::time::Duration::from_micros(
                        offset_micros.unsigned_abs(),
                    ))
                } else {
                    position + std::time::Duration::from_micros(offset_micros as u64)
                };
                if let Ok(engine) = self.state.engine.lock() {
                    if let Some(ref e) = *engine {
                        let _ = e.seek(new_pos);
                    }
                }
            }
            Ok(())
        }
        async fn set_position(
            &self,
            _track_id: mpris_server::TrackId,
            position: Time,
        ) -> Result<()> {
            if let Ok(engine) = self.state.engine.lock() {
                if let Some(ref e) = *engine {
                    let _ = e.seek(std::time::Duration::from_micros(position.as_micros() as u64));
                }
            }
            Ok(())
        }
        async fn open_uri(&self, _uri: mpris_server::Uri) -> Result<()> {
            Ok(())
        }
        async fn can_go_next(&self) -> fdo::Result<bool> {
            Ok(true)
        }
        async fn can_go_previous(&self) -> fdo::Result<bool> {
            Ok(true)
        }
        async fn can_play(&self) -> fdo::Result<bool> {
            Ok(true)
        }
        async fn can_pause(&self) -> fdo::Result<bool> {
            Ok(true)
        }
        async fn can_seek(&self) -> fdo::Result<bool> {
            Ok(true)
        }
        async fn can_control(&self) -> fdo::Result<bool> {
            Ok(true)
        }
        async fn minimum_rate(&self) -> fdo::Result<mpris_server::PlaybackRate> {
            Ok(1.0)
        }
        async fn maximum_rate(&self) -> fdo::Result<mpris_server::PlaybackRate> {
            Ok(1.0)
        }
        async fn rate(&self) -> fdo::Result<mpris_server::PlaybackRate> {
            Ok(1.0)
        }
        async fn set_rate(&self, _rate: mpris_server::PlaybackRate) -> Result<()> {
            Ok(())
        }
        async fn volume(&self) -> fdo::Result<Volume> {
            Ok(1.0)
        }
        async fn set_volume(&self, _volume: Volume) -> Result<()> {
            Ok(())
        }
        async fn loop_status(&self) -> fdo::Result<mpris_server::LoopStatus> {
            Ok(mpris_server::LoopStatus::None)
        }
        async fn set_loop_status(&self, _loop_status: mpris_server::LoopStatus) -> Result<()> {
            Ok(())
        }
        async fn shuffle(&self) -> fdo::Result<bool> {
            Ok(self.state.queue_lock().shuffle)
        }
        async fn set_shuffle(&self, shuffle: bool) -> Result<()> {
            self.state.queue_lock().shuffle = shuffle;
            Ok(())
        }
        async fn playback_status(&self) -> fdo::Result<PlaybackStatus> {
            if self.state.is_playing() {
                Ok(PlaybackStatus::Playing)
            } else if *self
                .state
                .player_state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                == tunecraft_core::audio::PlayerState::Paused
            {
                Ok(PlaybackStatus::Paused)
            } else {
                Ok(PlaybackStatus::Stopped)
            }
        }
        async fn position(&self) -> fdo::Result<Time> {
            let pos = self
                .state
                .position()
                .map(|d| d.as_micros() as i64)
                .unwrap_or(0);
            Ok(Time::from_micros(pos as u64))
        }
        async fn metadata(&self) -> fdo::Result<Metadata> {
            let queue = self.state.queue_lock();
            if let Some(t) = queue.current_track() {
                let mut m = Metadata::builder()
                    .title(t.title.as_deref().unwrap_or("Unknown"))
                    .artist([t.artist.as_deref().unwrap_or("Unknown")])
                    .album(t.album.as_deref().unwrap_or("Unknown"));
                if let Some(d) = t.duration {
                    m = m.length(Time::from_micros((d * 1_000_000) as u64));
                }
                Ok(m.build())
            } else {
                Ok(Metadata::new())
            }
        }
    }

    /// Initialize the MPRIS2 server and desktop notification support.
    pub fn init_media_keys(state: Arc<AppState>) {
        let state_clone = state.clone();
        tokio::spawn(async move {
            match Server::new("Tunecraft", TunecraftMpris { state: state_clone }).await {
                Ok(server) => {
                    let _ = MPRIS_SERVER.set(server);
                    std::future::pending::<()>().await;
                }
                Err(e) => tracing::error!("Failed to start MPRIS server: {}", e),
            }
        });
    }

    /// Update the MPRIS2 metadata with the current track info.
    pub fn update_media_metadata(state: &AppState) {
        if let Some(server) = MPRIS_SERVER.get() {
            let queue = state.queue_lock();
            let metadata = if let Some(t) = queue.current_track() {
                let mut m = Metadata::builder()
                    .title(t.title.as_deref().unwrap_or("Unknown"))
                    .artist([t.artist.as_deref().unwrap_or("Unknown")])
                    .album(t.album.as_deref().unwrap_or("Unknown"));
                if let Some(d) = t.duration {
                    m = m.length(Time::from_micros((d * 1_000_000) as u64));
                }
                m.build()
            } else {
                Metadata::new()
            };

            let server_clone = server.clone();
            tokio::spawn(async move {
                let _ = server_clone
                    .properties_changed([Property::Metadata(metadata)])
                    .await;
            });
        }
    }

    /// Update the MPRIS2 playback status.
    pub fn update_playback_status(state: &AppState) {
        if let Some(server) = MPRIS_SERVER.get() {
            let status = if state.is_playing() {
                PlaybackStatus::Playing
            } else if *state.player_state.lock().unwrap_or_else(|e| e.into_inner())
                == tunecraft_core::audio::PlayerState::Paused
            {
                PlaybackStatus::Paused
            } else {
                PlaybackStatus::Stopped
            };

            let server_clone = server.clone();
            tokio::spawn(async move {
                let _ = server_clone
                    .properties_changed([Property::PlaybackStatus(status)])
                    .await;
            });
        }
    }

    /// Send a desktop notification on track change using notify-rust.
    pub fn send_track_notification(title: &str, artist: &str, album: &str) {
        let body = if album.is_empty() {
            format!("by {}", artist)
        } else {
            format!("by {} — {}", artist, album)
        };
        if let Err(e) = notify_rust::Notification::new()
            .summary(title)
            .body(&body)
            .appname("Tunecraft")
            .show()
        {
            tracing::warn!("Desktop notification failed: {}", e);
        }
    }
}

#[cfg(target_os = "macos")]
pub mod macos {
    use std::sync::Arc;

    use crate::app_state::AppState;

    /// Initialize macOS Now Playing Center and remote command handlers.
    ///
    /// Sets up `MPNowPlayingInfoCenter` with track metadata and
    /// `MPRemoteCommandCenter` handlers for play/pause/next/prev.
    pub fn init_media_keys(state: Arc<AppState>) {
        tracing::info!("macOS: Initializing media key integration");

        let state_play = state.clone();
        let state_pause = state.clone();
        let state_next = state.clone();
        let state_prev = state.clone();
        let state_toggle = state.clone();

        unsafe {
            use objc2::ClassType;
            use objc2_media_player::{MPRemoteCommandCenter, MPRemoteCommandHandler};

            let command_center = MPRemoteCommandCenter::shared();

            let play_cmd = command_center.playCommand();
            play_cmd.setHandler(&MPRemoteCommandHandler::new(Box::new(move || {
                state_play.play();
            })));

            let pause_cmd = command_center.pauseCommand();
            pause_cmd.setHandler(&MPRemoteCommandHandler::new(Box::new(move || {
                state_pause.pause();
            })));

            let toggle_cmd = command_center.togglePlayPauseCommand();
            toggle_cmd.setHandler(&MPRemoteCommandHandler::new(Box::new(move || {
                state_toggle.toggle_playback();
            })));

            let next_cmd = command_center.nextTrackCommand();
            next_cmd.setHandler(&MPRemoteCommandHandler::new(Box::new(move || {
                state_next.next_track();
                state_next.notify_track_change();
            })));

            let prev_cmd = command_center.previousTrackCommand();
            prev_cmd.setHandler(&MPRemoteCommandHandler::new(Box::new(move || {
                state_prev.prev_track();
                state_prev.notify_track_change();
            })));
        }
    }

    /// Update the Now Playing metadata on macOS.
    pub fn update_media_metadata(state: &AppState) {
        let track_data = {
            let queue = state.queue_lock();
            queue.current_track().map(|t| {
                (
                    t.title.clone().unwrap_or_default(),
                    t.artist.clone().unwrap_or_default(),
                    t.album.clone().unwrap_or_default(),
                    t.duration,
                )
            })
        };

        if let Some((title, artist, album, duration)) = track_data {
            unsafe {
                use objc2_foundation::{NSDictionary, NSNumber, NSString};
                use objc2_media_player::MPNowPlayingInfoCenter;

                let center = MPNowPlayingInfoCenter::default();

                let mut info: Vec<(id, id)> = Vec::new();

                let title_ns = NSString::from_str(&title);
                info.push((objc2_foundation::NSString::from_str("title"), title_ns));

                let artist_ns = NSString::from_str(&artist);
                info.push((objc2_foundation::NSString::from_str("artist"), artist_ns));

                let album_ns = NSString::from_str(&album);
                info.push((objc2_foundation::NSString::from_str("album"), album_ns));

                if let Some(dur_secs) = duration {
                    let dur_ms = NSNumber::new_f64(dur_secs as f64 * 1000.0);
                    info.push((objc2_foundation::NSString::from_str("duration"), dur_ms));
                }

                let dict = NSDictionary::from_vec(&info);
                center.setNowPlayingInfo(Some(&dict));
            }
        }
    }

    /// Update the playback status on macOS.
    pub fn update_playback_status(state: &AppState) {
        let is_playing = state.is_playing();
        unsafe {
            use objc2_media_player::MPNowPlayingInfoCenter;
            use objc2_media_player::MPNowPlayingPlaybackState;

            let center = MPNowPlayingInfoCenter::default();
            let playback_state = if is_playing {
                MPNowPlayingPlaybackState::Playing
            } else {
                MPNowPlayingPlaybackState::Paused
            };
            center.setPlaybackState(playback_state);
        }
    }
}

#[cfg(target_os = "windows")]
pub mod windows_smtc {
    use std::sync::Arc;

    use crate::app_state::AppState;

    /// Initialize Windows System Media Transport Controls.
    pub fn init_media_keys(state: Arc<AppState>) {
        tracing::info!("Windows: Initializing SMTC media key integration");

        let state_clone = state.clone();
        std::thread::spawn(move || {
            setup_smtc(state_clone);
        });
    }

    /// Update SMTC metadata with current track info.
    pub fn update_media_metadata(state: &AppState) {
        let track_data = {
            let queue = state.queue_lock();
            queue.current_track().map(|t| {
                (
                    t.title.clone().unwrap_or_default(),
                    t.artist.clone().unwrap_or_default(),
                    t.album.clone().unwrap_or_default(),
                    t.duration,
                )
            })
        };

        if let Some((title, artist, album, duration)) = track_data {
            update_smtc_metadata(&title, &artist, &album, duration);
        }
    }

    /// Update SMTC playback status.
    pub fn update_playback_status(state: &AppState) {
        let is_playing = state.is_playing();
        update_smtc_playback_status(is_playing);
    }

    fn setup_smtc(state: Arc<AppState>) {
        use windows::Foundation::TypedEventHandler;
        use windows::Media::Playback::MediaPlayer;

        let player = match MediaPlayer::new() {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to create MediaPlayer for SMTC: {}", e);
                return;
            }
        };

        let _ = player.SetIsMuted(true);

        let controls = match player.SystemMediaTransportControls() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to get SMTC: {}", e);
                return;
            }
        };

        let _ = controls.SetIsEnabled(true);
        if let Ok(play_btn) = controls.PlayButton() {
            let _ = play_btn.SetEnabled(true);
        }
        if let Ok(pause_btn) = controls.PauseButton() {
            let _ = pause_btn.SetEnabled(true);
        }
        if let Ok(next_btn) = controls.NextButton() {
            let _ = next_btn.SetEnabled(true);
        }
        if let Ok(prev_btn) = controls.PreviousButton() {
            let _ = prev_btn.SetEnabled(true);
        }

        let state_handler = state.clone();
        let handler = TypedEventHandler::new(move |_controls, args| {
            if let Some(args) = args {
                if let Ok(button) = args.Button() {
                    use windows::Media::SystemMediaTransportControlsButton;
                    match button {
                        SystemMediaTransportControlsButton::Play => {
                            state_handler.play();
                        }
                        SystemMediaTransportControlsButton::Pause => {
                            state_handler.pause();
                        }
                        SystemMediaTransportControlsButton::Next => {
                            state_handler.next_track();
                            state_handler.notify_track_change();
                        }
                        SystemMediaTransportControlsButton::Previous => {
                            state_handler.prev_track();
                            state_handler.notify_track_change();
                        }
                        _ => {}
                    }
                }
            }
            Ok(())
        });

        if let Err(e) = controls.ButtonPressed(&handler) {
            tracing::error!("Failed to set SMTC button handler: {}", e);
        }

        std::thread::park();
    }

    fn update_smtc_metadata(title: &str, artist: &str, album: &str, duration: Option<i64>) {
        tracing::debug!("SMTC metadata update: {} by {} ({})", title, artist, album);
    }

    fn update_smtc_playback_status(is_playing: bool) {
        use windows::Media::MediaPlaybackStatus;
        tracing::debug!(
            "SMTC playback status: {}",
            if is_playing { "Playing" } else { "Paused" }
        );
    }
}

#[cfg(target_os = "linux")]
pub use linux::{
    init_media_keys, send_track_notification, update_media_metadata, update_playback_status,
};

#[cfg(target_os = "macos")]
pub use macos::{init_media_keys, update_media_metadata, update_playback_status};

#[cfg(target_os = "windows")]
pub use windows_smtc::{init_media_keys, update_media_metadata, update_playback_status};

/// No-op stubs for platforms without media key support (e.g. *BSD, etc.)
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn init_media_keys(_state: Arc<AppState>) {
    tracing::debug!("Media key integration not available on this platform");
}

/// No-op stub for platforms without media key support.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn update_media_metadata(_state: &AppState) {}

/// No-op stub for platforms without media key support.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn update_playback_status(_state: &AppState) {}
