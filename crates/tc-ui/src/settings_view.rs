//! Settings view bridge — syncs settings state to the Slint SettingsView component.

use slint::SharedString;

use crate::app::TuneCraftApp;
use crate::App;

/// Sync settings view state to Slint.
pub fn sync_settings_view(app: &TuneCraftApp, slint_app: &App) {
    // Read all settings from config in one pass to minimize lock contention.
    let (
        volume,
        speed,
        crossfade_enabled,
        crossfade_secs,
        replaygain_mode,
        resampler_quality,
        performance_mode,
        dither_enabled,
        tracks_per_page,
        scan_on_startup,
        watch_dirs,
        lyrics_enabled,
        lyrics_fetch_on_play,
        lyrics_base_url,
        scrobble_enabled,
        scrobble_threshold_pct,
        scrobble_threshold_sec,
        theme,
        show_spectrum,
        show_waveform,
        minimize_to_tray,
    ) = app
        .ctx
        .config
        .read(|c| {
            (
                c.playback.volume,
                c.playback.speed,
                c.engine.crossfade.enabled,
                c.engine.crossfade.duration_ms as f32 / 1000.0,
                format!("{:?}", c.engine.loudness.mode).to_lowercase(),
                format!("{:?}", c.engine.resampler_quality).to_lowercase(),
                format!("{:?}", c.engine.performance_mode).to_lowercase(),
                c.engine.dither_enabled,
                c.library.tracks_per_page,
                c.library.scan_on_startup,
                c.library.watch_dirs.clone(),
                c.lyrics.enabled,
                c.lyrics.fetch_on_play,
                c.lyrics.base_url.clone(),
                c.scrobble.enabled,
                c.scrobble.scrobble_threshold_pct,
                c.scrobble.scrobble_threshold_sec as f32,
                format!("{:?}", c.ui.theme).to_lowercase(),
                c.ui.show_spectrum,
                c.ui.show_waveform,
                c.ui.minimize_to_tray,
            )
        })
        .unwrap_or((
            0.0,
            1.0,
            false,
            0.0,
            "off".to_string(),
            "balanced".to_string(),
            "balanced".to_string(),
            true,
            500,
            true,
            Vec::new(),
            true,
            true,
            String::new(),
            true,
            0.5,
            240.0,
            "dark".to_string(),
            true,
            true,
            false,
        ));

    slint_app.set_playback_settings(crate::PlaybackSettings {
        volume,
        speed,
        crossfade_secs,
        crossfade_enabled,
        replaygain_mode: SharedString::from(replaygain_mode),
        replaygain_preamp_db: 0.0,
    });

    slint_app.set_engine_settings(crate::EngineSettings {
        resampler_quality: SharedString::from(resampler_quality),
        performance_mode: SharedString::from(performance_mode),
        dither_enabled,
        limiter_enabled: false,
    });

    let watch_dirs_model: slint::ModelRc<SharedString> =
        slint::ModelRc::new(slint::VecModel::from(
            watch_dirs
                .iter()
                .map(|d| SharedString::from(d.to_string_lossy().to_string()))
                .collect::<Vec<_>>(),
        ));
    slint_app.set_library_settings(crate::LibrarySettings {
        tracks_per_page: tracks_per_page as i32,
        scan_on_startup,
        watch_dirs: watch_dirs_model,
    });

    slint_app.set_lyrics_settings(crate::LyricsSettings {
        enabled: lyrics_enabled,
        fetch_on_play: lyrics_fetch_on_play,
        base_url: SharedString::from(lyrics_base_url),
    });

    slint_app.set_scrobble_settings(crate::ScrobbleSettings {
        enabled: scrobble_enabled,
        min_duration_secs: scrobble_threshold_sec,
        min_percent: scrobble_threshold_pct,
    });

    slint_app.set_ui_settings(crate::UiSettings {
        theme: SharedString::from(theme),
        show_spectrum,
        show_waveform,
        minimize_to_tray,
    });
}
