//! D-Bus interface implementations for MPRIS
//!
//! Contains the MediaPlayer2 and MediaPlayer2.Player interface definitions
//! and the main D-Bus server event loop.
//!
//! propagation instead of panicking. Fixed `set_loop_status` match to use
//! `status.as_str()` for proper pattern matching. Reverted method signatures
//! to use `&str` parameters (matching zbus v4 expectations) instead of
//! `String` where the v0.9.8 code used `&str`.

use std::sync::{
    mpsc::{Receiver, Sender},
    Arc,
};

use parking_lot::Mutex;

use super::{signals, MprisState};
use crate::types::{MediaKeyAction, MprisPlaybackStatus, MprisPropertyChanged};

/// Run the D-Bus server, serving MPRIS interfaces.
pub(crate) fn run_dbus_server(
    identity: &str,
    action_tx: &Sender<MediaKeyAction>,
    state: &Arc<Mutex<MprisState>>,
    notify_rx: &Receiver<MprisPropertyChanged>,
) -> Result<(), String> {
    use zbus::{blocking::connection::Builder, interface};

    let bus_name = format!("org.mpris.MediaPlayer2.{}", identity);
    let object_path = "/org/mpris/MediaPlayer2";

    struct MediaPlayer2Iface {
        identity: String,
        desktop_entry: String,
        action_tx: Sender<MediaKeyAction>,
    }

    #[interface(name = "org.mpris.MediaPlayer2")]
    impl MediaPlayer2Iface {
        fn raise(&self) -> zbus::fdo::Result<()> {
            Err(zbus::fdo::Error::NotSupported(
                "Raise is not supported".to_string(),
            ))
        }

        /// main application actually responds to quit requests (C4).
        fn quit(&self) -> zbus::fdo::Result<()> {
            log::info!("MPRIS Quit requested");
            let _ = self.action_tx.send(MediaKeyAction::Quit);
            Ok(())
        }

        #[zbus(property)]
        fn can_quit(&self) -> zbus::fdo::Result<bool> {
            Ok(true)
        }

        #[zbus(property)]
        fn can_raise(&self) -> zbus::fdo::Result<bool> {
            Ok(false)
        }

        #[zbus(property)]
        fn has_track_list(&self) -> zbus::fdo::Result<bool> {
            Ok(false)
        }

        #[zbus(property)]
        fn identity(&self) -> zbus::fdo::Result<String> {
            Ok(self.identity.clone())
        }

        #[zbus(property)]
        fn desktop_entry(&self) -> zbus::fdo::Result<String> {
            Ok(self.desktop_entry.clone())
        }

        #[zbus(property)]
        fn supported_uri_schemes(&self) -> zbus::fdo::Result<Vec<String>> {
            Ok(vec!["file".to_string()])
        }

        #[zbus(property)]
        fn supported_mime_types(&self) -> zbus::fdo::Result<Vec<String>> {
            Ok(vec![
                "audio/mpeg".to_string(),
                "audio/flac".to_string(),
                "audio/ogg".to_string(),
                "audio/wav".to_string(),
                "audio/aac".to_string(),
            ])
        }
    }

    struct MediaPlayer2PlayerIface {
        action_tx: Sender<MediaKeyAction>,
        state: Arc<Mutex<MprisState>>,
    }

    #[interface(name = "org.mpris.MediaPlayer2.Player")]
    impl MediaPlayer2PlayerIface {
        fn next(&self) -> zbus::fdo::Result<()> {
            let _ = self.action_tx.send(MediaKeyAction::Next);
            Ok(())
        }

        fn previous(&self) -> zbus::fdo::Result<()> {
            let _ = self.action_tx.send(MediaKeyAction::Previous);
            Ok(())
        }

        /// instead of PlayPause (C3 fix).
        fn pause(&self) -> zbus::fdo::Result<()> {
            let _ = self.action_tx.send(MediaKeyAction::Pause);
            Ok(())
        }

        fn play_pause(&self) -> zbus::fdo::Result<()> {
            let _ = self.action_tx.send(MediaKeyAction::PlayPause);
            Ok(())
        }

        fn stop(&self) -> zbus::fdo::Result<()> {
            let _ = self.action_tx.send(MediaKeyAction::Stop);
            Ok(())
        }

        /// instead of PlayPause (C3 fix).
        fn play(&self) -> zbus::fdo::Result<()> {
            let _ = self.action_tx.send(MediaKeyAction::Play);
            Ok(())
        }

        fn seek(&self, offset: i64) -> zbus::fdo::Result<()> {
            // to the main thread via MediaKeyAction::Seek. Positive = forward,
            // negative = backward. The engine handles clamping.
            let _ = self.action_tx.send(MediaKeyAction::Seek(offset));
            Ok(())
        }

        fn set_position(&self, track_id: &str, position: i64) -> zbus::fdo::Result<()> {
            // track_id matches the current track and forwards the absolute
            // position (microseconds) to the main thread.
            let current_track = {
                let state = self.state.lock();
                state
                    .track_info
                    .track_id
                    .clone()
                    .unwrap_or_else(|| "/org/mpris/MediaPlayer2/TrackList/NoTrack".to_string())
            };
            if track_id != current_track {
                // MPRIS spec: If the TrackId is not the current track, do nothing
                log::debug!(
                    "SetPosition ignored: track_id {} != current {}",
                    track_id,
                    current_track
                );
                return Ok(());
            }
            if position < 0 {
                log::debug!("SetPosition ignored: negative position {}", position);
                return Ok(());
            }
            let _ = self.action_tx.send(MediaKeyAction::SetPosition {
                track_id: track_id.to_string(),
                position_us: position,
            });
            Ok(())
        }

        fn open_uri(&self, uri: &str) -> zbus::fdo::Result<()> {
            // Non-file URIs return NotSupported per MPRIS spec.
            if !uri.starts_with("file://") {
                return Err(zbus::fdo::Error::NotSupported(
                    "Only file:// URIs are supported".to_string(),
                ));
            }
            let _ = self
                .action_tx
                .send(MediaKeyAction::OpenUri(uri.to_string()));
            Ok(())
        }

        #[zbus(property)]
        fn playback_status(&self) -> zbus::fdo::Result<String> {
            let state = self.state.lock();
            Ok(match state.playback_status {
                MprisPlaybackStatus::Playing => "Playing".to_string(),
                MprisPlaybackStatus::Paused => "Paused".to_string(),
                MprisPlaybackStatus::Stopped => "Stopped".to_string(),
            })
        }

        #[zbus(property)]
        fn loop_status(&self) -> zbus::fdo::Result<String> {
            let state = self.state.lock();
            Ok(state.loop_status.clone())
        }

        #[zbus(property)]
        fn set_loop_status(&self, status: &str) -> zbus::Result<()> {
            // Valid values per MPRIS spec: "None", "Track", "Playlist"
            //
            // NOTE: Returns zbus::Result (not zbus::fdo::Result) to work around
            // a zbus v4.4.0 macro bug: the generated set() method's async block
            // uses `e => e` for the Err branch, which doesn't convert
            // zbus::fdo::Error → zbus::Error. Using zbus::Result makes the
            // types align correctly.
            match status {
                "None" | "Track" | "Playlist" => {
                    let _ = self
                        .action_tx
                        .send(MediaKeyAction::SetLoopStatus(status.to_string()));
                    let mut state = self.state.lock();
                    state.loop_status = status.to_string();
                    Ok(())
                },
                _ => Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Invalid LoopStatus '{}'. Must be None, Track, or Playlist.",
                    status
                ))
                .into()),
            }
        }

        #[zbus(property)]
        fn rate(&self) -> zbus::fdo::Result<f64> {
            let state = self.state.lock();
            Ok(state.rate)
        }

        #[zbus(property)]
        fn set_rate(&self, rate: f64) -> zbus::Result<()> {
            if rate < 0.25 || rate > 4.0 {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "Rate {} out of range [0.25, 4.0]",
                    rate
                ))
                .into());
            }
            let _ = self.action_tx.send(MediaKeyAction::SetRate(rate));
            let mut state = self.state.lock();
            state.rate = rate;
            Ok(())
        }

        #[zbus(property)]
        fn shuffle(&self) -> zbus::fdo::Result<bool> {
            let state = self.state.lock();
            Ok(state.shuffle)
        }

        #[zbus(property)]
        fn set_shuffle(&self, shuffle: bool) -> zbus::Result<()> {
            let _ = self.action_tx.send(MediaKeyAction::SetShuffle(shuffle));
            let mut state = self.state.lock();
            state.shuffle = shuffle;
            Ok(())
        }

        #[zbus(property)]
        fn metadata(
            &self,
        ) -> zbus::fdo::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>
        {
            // Extract all values while holding the lock, then drop it before
            // doing ObjectPath conversions (fixes track_id borrow issue).
            let (track_id, length_us, title, artist, album, art_url) =
                {
                    let state = self.state.lock();
                    (
                        state.track_info.track_id.clone().unwrap_or_else(|| {
                            "/org/mpris/MediaPlayer2/TrackList/NoTrack".to_string()
                        }),
                        state.track_info.length_microseconds,
                        state.track_info.title.clone(),
                        state.track_info.artist.clone(),
                        state.track_info.album.clone(),
                        state.track_info.art_url.clone(),
                    )
                }; // MutexGuard dropped here

            let mut metadata = std::collections::HashMap::new();

            // would abort the process. Use safe error propagation instead (blocker #8 fix).
            const NOTRACK_PATH: &str = "/org/mpris/MediaPlayer2/TrackList/NoTrack";

            let trackid_path: zbus::zvariant::OwnedObjectPath =
                zbus::zvariant::ObjectPath::try_from(track_id.as_str())
                    .or_else(|_| zbus::zvariant::ObjectPath::try_from(NOTRACK_PATH))
                    .map_err(|e| zbus::fdo::Error::Failed(format!("Invalid track path: {}", e)))?
                    .into();

            metadata.insert(
                "mpris:trackid".to_string(),
                zbus::zvariant::Value::new(trackid_path)
                    .try_into()
                    .map_err(|e: zbus::zvariant::Error| zbus::fdo::Error::Failed(e.to_string()))?,
            );

            if let Some(us) = length_us {
                metadata.insert(
                    "mpris:length".to_string(),
                    zbus::zvariant::Value::new(us).try_into().map_err(
                        |e: zbus::zvariant::Error| zbus::fdo::Error::Failed(e.to_string()),
                    )?,
                );
            }
            if let Some(t) = title {
                metadata.insert(
                    "xesam:title".to_string(),
                    zbus::zvariant::Value::new(t).try_into().map_err(
                        |e: zbus::zvariant::Error| zbus::fdo::Error::Failed(e.to_string()),
                    )?,
                );
            }
            if let Some(a) = artist {
                metadata.insert(
                    "xesam:artist".to_string(),
                    zbus::zvariant::Value::new(vec![a]).try_into().map_err(
                        |e: zbus::zvariant::Error| zbus::fdo::Error::Failed(e.to_string()),
                    )?,
                );
            }
            if let Some(al) = album {
                metadata.insert(
                    "xesam:album".to_string(),
                    zbus::zvariant::Value::new(al).try_into().map_err(
                        |e: zbus::zvariant::Error| zbus::fdo::Error::Failed(e.to_string()),
                    )?,
                );
            }
            if let Some(url) = art_url {
                metadata.insert(
                    "mpris:artUrl".to_string(),
                    zbus::zvariant::Value::new(url).try_into().map_err(
                        |e: zbus::zvariant::Error| zbus::fdo::Error::Failed(e.to_string()),
                    )?,
                );
            }

            Ok(metadata)
        }

        #[zbus(property)]
        fn volume(&self) -> zbus::fdo::Result<f64> {
            let state = self.state.lock();
            Ok(state.volume)
        }

        #[zbus(property)]
        fn set_volume(&self, volume: f64) -> zbus::Result<()> {
            // MPRIS allows values > 1.0 for amplification but we cap at 1.0.
            // NOTE: Returns zbus::Result — see set_loop_status for rationale.
            let clamped = volume.clamp(0.0, 1.0);
            let _ = self.action_tx.send(MediaKeyAction::SetVolume(clamped));
            let mut state = self.state.lock();
            state.volume = clamped;
            Ok(())
        }

        #[zbus(property)]
        fn position(&self) -> zbus::fdo::Result<i64> {
            let state = self.state.lock();
            Ok(state.position_microseconds)
        }

        #[zbus(property)]
        fn minimum_rate(&self) -> zbus::fdo::Result<f64> {
            Ok(0.25)
        }

        #[zbus(property)]
        fn maximum_rate(&self) -> zbus::fdo::Result<f64> {
            Ok(4.0)
        }

        #[zbus(property)]
        fn can_go_next(&self) -> zbus::fdo::Result<bool> {
            Ok(true)
        }

        #[zbus(property)]
        fn can_go_previous(&self) -> zbus::fdo::Result<bool> {
            Ok(true)
        }

        #[zbus(property)]
        fn can_play(&self) -> zbus::fdo::Result<bool> {
            Ok(true)
        }

        #[zbus(property)]
        fn can_pause(&self) -> zbus::fdo::Result<bool> {
            Ok(true)
        }

        #[zbus(property)]
        fn can_seek(&self) -> zbus::fdo::Result<bool> {
            Ok(true)
        }

        #[zbus(property)]
        fn can_control(&self) -> zbus::fdo::Result<bool> {
            Ok(true)
        }
    }

    let root_iface = MediaPlayer2Iface {
        identity: identity.to_string(),
        desktop_entry: identity.to_lowercase(),
        action_tx: action_tx.clone(),
    };
    let player_iface = MediaPlayer2PlayerIface {
        action_tx: action_tx.clone(),
        state: Arc::clone(state),
    };

    let conn = Builder::session()
        .map_err(|e| format!("Failed to create D-Bus session: {}", e))?
        .name(bus_name.clone())
        .map_err(|e| format!("Failed to acquire bus name '{}': {}", bus_name, e))?
        .serve_at(object_path, root_iface)
        .map_err(|e| format!("Failed to serve MediaPlayer2 at {}: {}", object_path, e))?
        .serve_at(object_path, player_iface)
        .map_err(|e| {
            format!(
                "Failed to serve MediaPlayer2.Player at {}: {}",
                object_path, e
            )
        })?
        .build()
        .map_err(|e| format!("Failed to build D-Bus connection: {}", e))?;

    log::info!("MPRIS D-Bus service registered as {}", bus_name);

    // Event loop: listen for property change notifications
    // from the main thread and emit PropertiesChanged signals.
    loop {
        match notify_rx.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(changed) => {
                if let Err(e) = signals::emit_properties_changed(&conn, state, changed) {
                    log::debug!("Failed to emit PropertiesChanged: {}", e);
                }
            },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Normal timeout — continue loop
            },
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                log::info!("MPRIS notification channel closed, shutting down");
                break;
            },
        }
    }

    Ok(())
}
