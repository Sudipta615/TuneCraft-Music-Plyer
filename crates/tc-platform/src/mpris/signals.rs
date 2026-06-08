//! MPRIS PropertiesChanged D-Bus signal emission
//!
//! Contains the logic for emitting `org.freedesktop.DBus.Properties.PropertiesChanged`
//! signals when MPRIS properties are updated.
//!
//! state and includes them in the `changed_properties` dictionary
//! of the signal (C2 fix). This allows MPRIS clients to update
//! their display immediately without making additional Get() calls.
//!
//! `Connection::emit_signal()` with `zbus::message::Builder::signal()`.

use std::sync::Arc;

use parking_lot::Mutex;
use zbus::zvariant::Value;

use super::MprisState;
use crate::types::{MprisPlaybackStatus, MprisPropertyChanged};

/// Emit a PropertiesChanged signal using the zbus v4 message builder API.
fn send_properties_changed(
    conn: &zbus::blocking::Connection,
    path: &str,
    iface_name: &str,
    changed_props: std::collections::HashMap<&str, Value>,
    invalidated: Vec<&str>,
) -> Result<(), zbus::Error> {
    #[allow(deprecated)] // zbus v4.4 deprecated Builder::signal; replacement API not yet stable
    let msg = zbus::message::Builder::signal(
        path,
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
    )?
    .build(&(iface_name, changed_props, invalidated))?;
    conn.send(&msg)?;
    Ok(())
}

/// Emit a PropertiesChanged D-Bus signal for the given property.
pub(crate) fn emit_properties_changed(
    conn: &zbus::blocking::Connection,
    state: &Arc<Mutex<MprisState>>,
    changed: MprisPropertyChanged,
) -> Result<(), zbus::Error> {
    let iface_name = "org.mpris.MediaPlayer2.Player";
    let path = "/org/mpris/MediaPlayer2";

    match changed {
        MprisPropertyChanged::PlaybackStatus => {
            let status_str = {
                let s = state.lock();
                match s.playback_status {
                    MprisPlaybackStatus::Playing => "Playing",
                    MprisPlaybackStatus::Paused => "Paused",
                    MprisPlaybackStatus::Stopped => "Stopped",
                }
                .to_string()
            };
            let mut map = std::collections::HashMap::<&str, Value>::new();
            map.insert("PlaybackStatus", Value::Str(status_str.into()));
            send_properties_changed(conn, path, iface_name, map, vec![])?;
        },
        MprisPropertyChanged::TrackMetadata => {
            // Invalidate so clients re-query the full metadata dict.
            send_properties_changed(
                conn,
                path,
                iface_name,
                std::collections::HashMap::new(),
                vec!["Metadata"],
            )?;
        },
        MprisPropertyChanged::Volume => {
            let vol = state.lock().volume;
            let mut map = std::collections::HashMap::<&str, Value>::new();
            map.insert("Volume", Value::F64(vol as f64));
            send_properties_changed(conn, path, iface_name, map, vec![])?;
        },
        MprisPropertyChanged::Shuffle => {
            let shuffle = state.lock().shuffle;
            let mut map = std::collections::HashMap::<&str, Value>::new();
            map.insert("Shuffle", Value::Bool(shuffle));
            send_properties_changed(conn, path, iface_name, map, vec![])?;
        },
        MprisPropertyChanged::LoopStatus => {
            let loop_status = state.lock().loop_status.clone();
            let mut map = std::collections::HashMap::<&str, Value>::new();
            map.insert("LoopStatus", Value::Str(loop_status.into()));
            send_properties_changed(conn, path, iface_name, map, vec![])?;
        },
    }

    Ok(())
}
