//! TuneCraft — A production-grade cross-platform offline music player
//!
//! Main binary entry point. Launches the egui GUI by default,
//! or runs in headless/CLI mode when the `gui` feature is disabled
//! or `--headless` flag is passed.
//!
//! ## Architecture (v0.8.0)
//!
//! The application uses a **service-layer pattern** where backend subsystems
//! are encapsulated behind typed service objects. Both GUI and headless modes
//! share the same initialization logic via the `AppBuilder` pattern.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use log::{info, warn, error};

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    info!("TuneCraft v{}", env!("CARGO_PKG_VERSION"));

    // L18: Collect args once here; run_headless() also collects its own.
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless" || a == "--cli");

    if headless {
        run_headless()
    } else {
        #[cfg(feature = "gui")]
        {
            run_gui()
        }

        #[cfg(not(feature = "gui"))]
        {
            info!("GUI not compiled. Running in headless mode.");
            info!("Compile with --features gui to enable the graphical interface.");
            run_headless()
        }
    }
}

#[cfg(feature = "gui")]
fn run_gui() -> Result<()> {
    info!("Launching TuneCraft GUI...");
    tc_ui::run().map_err(|e| anyhow::anyhow!("GUI error: {}", e))
}

/// Shared application resource builder.

/// Encapsulates the common initialization sequence that was previously
/// duplicated between `AppContext::init()` and `run_headless()`.
/// Each step is independent and can fail without leaking resources.
struct AppResources {
    config: tc_config::AppConfig,
    db: Arc<tc_db::Database>,
    library: Arc<tc_library::LibraryManager>,
    #[cfg(feature = "audio-output")]
    engine: Option<tc_engine::AudioEngine>,
    platform: Option<tc_platform::PlatformIntegration>,
    media_key_rx: Option<tc_platform::MediaKeyReceiver>,
    tokio_runtime: Arc<tokio::runtime::Runtime>,
}

impl AppResources {
    /// Build application resources step by step.
    fn build() -> Result<Self> {
        let config = tc_config::ConfigPersistence::load_or_default();

        let db_path = dirs::data_dir()
            .unwrap_or_else(|| {
                warn!(
                    "Cannot determine user data directory. Falling back to ~/tunecraft. \
                     Set XDG_DATA_HOME or HOME to override."
                );
                dirs::home_dir()
                    .map(|h| h.join("tunecraft"))
                    .unwrap_or_else(|| {
                        // L22: /tmp is cleared on reboot on many systems, so
                        // the database would be lost. This is a last-resort
                        // fallback; users should set HOME or XDG_DATA_HOME.
                        warn!("Falling back to /tmp/tunecraft — data may be lost on reboot!");
                        PathBuf::from("/tmp/tunecraft")
                    })
            })
            .join("library.db");

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = tc_db::Database::open(&db_path)
            .context("Failed to open database")?;
        let db = Arc::new(db);
        info!("Database opened at {}", db_path.display());

        let library = Arc::new(tc_library::LibraryManager::new(
            Arc::clone(&db),
            config.library.clone(),
        ));

        #[cfg(feature = "audio-output")]
        let engine = {
            match tc_engine::AudioEngine::new(config.engine.clone()) {
                Ok(mut eng) => {
                    match eng.start() {
                        Ok(()) => {
                            info!("Audio engine started. Ready for playback.");
                            Some(eng)
                        }
                        Err(e) => {
                            warn!("Failed to start audio engine: {}", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to create audio engine: {}", e);
                    None
                }
            }
        };

        let (platform, media_key_rx) = match tc_platform::PlatformIntegration::new() {
            Ok((mut p, rx)) => {
                if let Err(e) = p.register_mpris("TuneCraft") {
                    // On non-Linux platforms, register_mpris returns NotAvailable,
                    // which is expected. Only warn for unexpected errors.
                    if !matches!(e, tc_platform::PlatformError::NotAvailable(_)) {
                        warn!("MPRIS registration failed: {}", e);
                    }
                }
                if let Err(e) = p.start_media_key_listener() {
                    warn!("Media key listener failed: {}", e);
                }
                (Some(p), Some(rx))
            }
            Err(e) => {
                warn!("Platform integration not available: {}", e);
                (None, None)
            }
        };

        let tokio_runtime = Arc::new(
            tokio::runtime::Runtime::new()
                .context("Failed to create tokio runtime")?
        );

        Ok(Self {
            config,
            db,
            library,
            #[cfg(feature = "audio-output")]
            engine,
            platform,
            media_key_rx,
            tokio_runtime,
        })
    }

    /// Run library scan if configured.
    fn scan_library(&self) -> Option<std::thread::JoinHandle<()>> {
        if self.config.library.scan_on_startup {
            info!("Starting library scan...");
            let lib = Arc::clone(&self.library);
            let handle = std::thread::spawn(move || {
                match lib.scan(|progress| {
                    if progress.files_processed % 100 == 0 {
                        info!(
                            "Scanning: {}/{} files processed",
                            progress.files_processed, progress.files_found
                        );
                    }
                }) {
                    Ok(progress) => {
                        info!(
                            "Scan complete: {} files found, {} added, {} updated",
                            progress.files_found, progress.files_added, progress.files_updated
                        );
                    }
                    Err(e) => {
                        error!("Library scan failed: {}", e);
                    }
                }
            });
            Some(handle)
        } else {
            None
        }
    }
}

fn run_headless() -> Result<()> {
    // L18: Collect command-line args once; previously collected three
    // separate times across main(), run_headless(), and run_with_audio().
    let args: Vec<String> = std::env::args().collect();

    let resources = AppResources::build()?;

    let skip_analysis = args.iter().any(|a| a == "--skip-analysis");
    let skip_scan = args.iter().any(|a| a == "--skip-scan");

    // Scan library (unless skipped).
    if !skip_scan {
        if let Some(handle) = resources.scan_library() {
            info!("Waiting for library scan to complete...");
            if let Err(e) = handle.join() {
                error!("Library scan thread panicked: {:?}", e);
            }
        }
    } else {
        info!("Skipping library scan (--skip-scan)");
    }

    // Print library stats
    match resources.db.track_count() {
        Ok(count) => info!("Library contains {} tracks", count),
        Err(e) => warn!("Failed to count tracks: {}", e),
    }

    if !skip_analysis {
        run_analysis(&resources.db)?;
    } else {
        info!("Skipping audio analysis (--skip-analysis)");
    }

    #[cfg(feature = "audio-output")]
    {
        if let Some(mut engine) = resources.engine {
            run_with_audio(&args, resources.config, resources.db, resources.library, &mut engine, resources.platform, resources.media_key_rx)?;
        } else {
            info!("Audio engine not available. Library management complete. Exiting.");
        }
    }

    #[cfg(not(feature = "audio-output"))]
    {
        info!("Audio output disabled (compile with --features audio-output to enable)");
        info!("TuneCraft library management complete. Exiting.");
    }

    Ok(())
}

/// Run audio analysis on tracks that haven't been analyzed yet
fn run_analysis(db: &tc_db::Database) -> Result<()> {
    use tc_analysis::analyze_file;
    use std::path::PathBuf;

    info!("Running audio analysis on unanalyzed tracks...");

    let unanalyzed = db.get_unanalyzed_tracks().context("Failed to get unanalyzed tracks")?;

    if unanalyzed.is_empty() {
        info!("All tracks already analyzed.");
        return Ok(());
    }

    info!("Analyzing {} tracks...", unanalyzed.len());
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for track in &unanalyzed {
        let path = PathBuf::from(&track.path);
        let lyrics_text: Option<String> = track.lyrics_synced.clone().or_else(|| track.lyrics_unsynced.clone());
        match analyze_file(&path, Some(60.0), lyrics_text.as_deref()) {
            Ok(analysis) => {
                if track.bpm.is_none() {
                    if let Err(e) = db.update_bpm(track.id, analysis.bpm.bpm) {
                        warn!("Failed to update BPM for track {}: {}", track.id, e);
                    }
                }
                if track.mood.is_none() {
                    if let Err(e) = db.update_mood(track.id, &analysis.mood.mood) {
                        warn!("Failed to update mood for track {}: {}", track.id, e);
                    }
                }
                succeeded += 1;
            }
            Err(e) => {
                warn!("Analysis failed for {}: {}", track.path, e);
                failed += 1;
            }
        }
    }

    info!("Analysis complete: {} succeeded, {} failed", succeeded, failed);
    Ok(())
}

#[cfg(feature = "audio-output")]
fn run_with_audio(
    args: &[String],
    config: tc_config::AppConfig,
    db: Arc<tc_db::Database>,
    _library: Arc<tc_library::LibraryManager>,
    engine: &mut tc_engine::AudioEngine,
    mut platform: Option<tc_platform::PlatformIntegration>,
    media_key_rx: Option<tc_platform::MediaKeyReceiver>,
) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    // If a file path was provided as argument, play it.
    // L18: Args are collected once by run_headless(); pass the slice here.
    let file_path = args.iter().skip(1)
        .find(|a| !a.starts_with('-'));
    if let Some(path_str) = file_path {
        let path = PathBuf::from(path_str);
        if path.exists() {
            info!("Loading file: {}", path.display());
            match engine.load_track(&path) {
                Ok(info) => {
                    info!(
                        "Track loaded: {} Hz, {} ch, {:.1}s",
                        info.sample_rate, info.channels, info.duration_secs
                    );
                    engine.send_command(tc_engine::buffer::EngineCommand::Play);
                }
                Err(e) => {
                    error!("Failed to load track: {}", e);
                }
            }
        }
    }

    // Main event loop
    info!("Entering main loop. Press Ctrl+C to exit.");
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C, shutting down...");
        running.store(false, Ordering::Relaxed);
    }).context("Failed to set Ctrl+C handler")?;

    let mut headless_shuffle = false;
    let mut headless_loop_status = "None".to_string();

    while r.load(Ordering::Relaxed) {
        engine.tick();

        // Process media key events via the separate MediaKeyReceiver
        if let Some(ref rx) = media_key_rx {
            while let Some(action) = rx.try_recv() {
                use tc_platform::MediaKeyAction;
                match action {
                    MediaKeyAction::Play => {
                        engine.send_command(tc_engine::buffer::EngineCommand::Play);
                    }
                    MediaKeyAction::Pause => {
                        engine.send_command(tc_engine::buffer::EngineCommand::Pause);
                    }
                    MediaKeyAction::PlayPause => {
                        let info = engine.playback_info();
                        if info.state == tc_engine::buffer::PlaybackState::Playing {
                            engine.send_command(tc_engine::buffer::EngineCommand::Pause);
                        } else {
                            engine.send_command(tc_engine::buffer::EngineCommand::Play);
                        }
                    }
                    MediaKeyAction::Next => {
                        engine.send_command(tc_engine::buffer::EngineCommand::NextTrack);
                    }
                    MediaKeyAction::Previous => {
                        engine.send_command(tc_engine::buffer::EngineCommand::PrevTrack);
                    }
                    MediaKeyAction::Stop => {
                        engine.send_command(tc_engine::buffer::EngineCommand::Stop);
                    }
                    MediaKeyAction::Quit => {
                        info!("Quit requested via MPRIS");
                        r.store(false, Ordering::Relaxed);
                    }

                    MediaKeyAction::Seek(offset_us) => {
                        let info = engine.playback_info();
                        let current_pos = info.position_secs;
                        let offset_secs = offset_us as f64 / 1_000_000.0;
                        let new_pos = (current_pos + offset_secs).max(0.0);
                        info!("MPRIS Seek: offset={:.1}s, new_pos={:.1}s", offset_secs, new_pos);
                        engine.send_command(tc_engine::buffer::EngineCommand::Seek(new_pos));
                    }

                    MediaKeyAction::SetPosition { track_id: _, position_us } => {
                        let pos_secs = position_us as f64 / 1_000_000.0;
                        info!("MPRIS SetPosition: {:.1}s", pos_secs);
                        engine.send_command(tc_engine::buffer::EngineCommand::Seek(pos_secs));
                    }

                    MediaKeyAction::SetVolume(vol) => {
                        info!("MPRIS SetVolume: {:.2}", vol);
                        engine.send_command(tc_engine::buffer::EngineCommand::SetVolume(vol.clamp(0.0, 1.0)));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_volume(vol);
                        }
                    }

                    MediaKeyAction::SetRate(rate) => {
                        info!("MPRIS SetRate: {:.2}", rate);
                        engine.send_command(tc_engine::buffer::EngineCommand::SetSpeed(rate));
                    }

                    MediaKeyAction::SetShuffle(shuffle) => {
                        info!("MPRIS SetShuffle: {}", shuffle);
                        headless_shuffle = shuffle;
                        engine.send_command(tc_engine::buffer::EngineCommand::SetShuffle(shuffle));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_shuffle(shuffle);
                        }
                    }

                    MediaKeyAction::SetLoopStatus(status) => {
                        info!("MPRIS SetLoopStatus: {}", status);
                        headless_loop_status = status.clone();
                        engine.send_command(tc_engine::buffer::EngineCommand::SetLoopStatus(status));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_loop_status(&headless_loop_status);
                        }
                    }

                    MediaKeyAction::OpenUri(uri) => {
                        info!("MPRIS OpenUri: {}", uri);
                        engine.send_command(tc_engine::buffer::EngineCommand::OpenUri(uri));
                    }
                    // VolumeUp/VolumeDown/Mute — helper actions
                    MediaKeyAction::VolumeUp => {
                        let vol = engine.playback_info().volume;
                        let new_vol = (vol + 0.05).min(1.0);
                        engine.send_command(tc_engine::buffer::EngineCommand::SetVolume(new_vol));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_volume(new_vol);
                        }
                    }
                    MediaKeyAction::VolumeDown => {
                        let vol = engine.playback_info().volume;
                        let new_vol = (vol - 0.05).max(0.0);
                        engine.send_command(tc_engine::buffer::EngineCommand::SetVolume(new_vol));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_volume(new_vol);
                        }
                    }
                    MediaKeyAction::Mute => {
                        engine.send_command(tc_engine::buffer::EngineCommand::SetVolume(0.0));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_volume(0.0);
                        }
                    }
                }
            }
        }

        if let Some(ref mut p) = platform {
            let info = engine.playback_info();
            // L15: Direct `as i64` cast overflows if position_secs is negative or
            // very large. Clamp to [0, i64::MAX] range before casting.
            let pos_us = (info.position_secs * 1_000_000.0).clamp(0.0, i64::MAX as f64) as i64;
            p.set_mpris_position(pos_us);
        }

        // Adaptive sleep
        let info = engine.playback_info();
        let sleep_ms = if info.state == tc_engine::buffer::PlaybackState::Playing {
            5
        } else {
            50
        };
        std::thread::sleep(Duration::from_millis(sleep_ms));
    }

    info!("Shutting down...");
    engine.stop();
    info!("Goodbye!");
    Ok(())
}

