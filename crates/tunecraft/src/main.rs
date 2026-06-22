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

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use log::{error, info, warn};

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("TuneCraft v{}", env!("CARGO_PKG_VERSION"));

    // L18: Collect args once here; run_headless() also collects its own.
    let args: Vec<String> = std::env::args().collect();

    // v3.1.3: The man page (dist/tunecraft.1) documents --help and --version
    // but they were never wired up. Handle them up-front before any other
    // initialization so we exit immediately without opening the DB / audio
    // engine — that mirrors user expectations for these standard flags and
    // matches the behaviour documented in the man page.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        // Single-line semver-style version string. The man page promises
        // "version information"; include crate name + version + target OS
        // + architecture so users can identify the exact build.
        println!(
            "TuneCraft {} ({} {})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
        return Ok(());
    }

    let headless = args.iter().any(|a| a == "--headless" || a == "--cli");
    // v3.1.1: `--analyze` was documented in the README but never wired up.
    // It forces headless mode and ensures the analysis pass runs on every
    // track missing BPM, loudness, or ReplayGain — including tracks that
    // have BPM but lack loudness (the common case for pre-v3.0.0 libraries).
    let analyze = args.iter().any(|a| a == "--analyze");
    let headless = headless || analyze;

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

/// Print a short usage summary.
///
/// Mirrors the SYNOPSIS section of `dist/tunecraft.1`. Keep the two in
/// sync when adding/removing flags — there is no test that enforces this,
/// so manual review is required on any change here.
fn print_help() {
    println!("TuneCraft v{} — audiophile-grade offline music player", env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!("    tunecraft [FILE] [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    FILE                Open and play a specific audio file on launch");
    println!("    --headless, --cli   Run without the GUI (library management only)");
    println!("    --analyze           Force loudness/BPM analysis on unanalyzed tracks");
    println!("    --skip-scan         Skip the startup library scan (headless only)");
    println!("    --skip-analysis     Skip the startup audio analysis pass (headless only)");
    println!("    -h, --help          Print this help and exit");
    println!("    -V, --version       Print version information and exit");
    println!();
    println!("ENVIRONMENT:");
    println!("    RUST_LOG            Log level filter (info, debug, warn). Default: info");
    println!("    XDG_DATA_HOME       Override the SQLite DB / play-history location");
    println!("    XDG_CONFIG_HOME     Override the config.toml location");
    println!();
    println!("SUPPORTED FORMATS: MP3, FLAC, OGG/Vorbis, WAV, AAC");
    println!();
    println!("Full documentation: https://github.com/Sudipta615/TuneCraft-Music-Plyer");
}

#[cfg(feature = "gui")]
fn run_gui() -> Result<()> {
    info!("Launching TuneCraft GUI...");
    tc_ui::run().map_err(|e| anyhow::anyhow!("GUI error: {}", e))
}

/// Shared application resource builder.
///
/// Encapsulates the common initialization sequence that was previously
/// duplicated between `AppContext::init()` and `run_headless()`.
/// Each step is independent and can fail without leaking resources.
#[allow(dead_code)]
struct AppResources {
    config: tc_config::AppConfig,
    db: Arc<tc_db::Database>,
    library: Arc<tc_library::LibraryManager>,
    #[cfg(feature = "audio-output")]
    engine: Option<tc_engine::AudioEngine>,
    platform: Option<tc_platform::PlatformIntegration>,
    media_key_rx: Option<tc_platform::MediaKeyReceiver>,
    _tokio_runtime: Arc<tokio::runtime::Runtime>,
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

        let db = tc_db::Database::open(&db_path).context("Failed to open database")?;
        let db = Arc::new(db);
        info!("Database opened at {}", db_path.display());

        let library = Arc::new(tc_library::LibraryManager::new(
            Arc::clone(&db),
            config.library.clone(),
        ));

        #[cfg(feature = "audio-output")]
        let engine = {
            match tc_engine::AudioEngine::new(config.engine.clone()) {
                Ok(mut eng) => match eng.start() {
                    Ok(()) => {
                        info!("Audio engine started. Ready for playback.");
                        Some(eng)
                    },
                    Err(e) => {
                        warn!("Failed to start audio engine: {}", e);
                        None
                    },
                },
                Err(e) => {
                    warn!("Failed to create audio engine: {}", e);
                    None
                },
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
            },
            Err(e) => {
                warn!("Platform integration not available: {}", e);
                (None, None)
            },
        };

        let tokio_runtime =
            Arc::new(tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?);

        Ok(Self {
            config,
            db,
            library,
            #[cfg(feature = "audio-output")]
            engine,
            platform,
            media_key_rx,
            _tokio_runtime: tokio_runtime,
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
                    },
                    Err(e) => {
                        error!("Library scan failed: {}", e);
                    },
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
    let force_analyze = args.iter().any(|a| a == "--analyze");

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
        run_analysis(&resources.db, force_analyze)?;
    } else {
        info!("Skipping audio analysis (--skip-analysis)");
    }

    #[cfg(feature = "audio-output")]
    {
        if let Some(mut engine) = resources.engine {
            run_with_audio(
                &args,
                resources.config,
                resources.db,
                resources.library,
                &mut engine,
                resources.platform,
                resources.media_key_rx,
            )?;
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

/// Run audio analysis on tracks that haven't been analyzed yet.
///
/// `force_loudness_backfill`: when true (i.e. `--analyze` was passed), use
/// the broader `get_tracks_missing_analysis` query that also returns tracks
/// with BPM but no loudness. Otherwise use `get_unanalyzed_tracks` (BPM only)
/// to keep the default startup behaviour fast.
fn run_analysis(db: &tc_db::Database, force_loudness_backfill: bool) -> Result<()> {
    use std::path::PathBuf;

    use tc_analysis::analyze_file;

    info!("Running audio analysis on unanalyzed tracks...");

    // v3.1.1: Use get_tracks_missing_analysis when --analyze is passed so we
    // also backfill loudness for pre-v3.0.0 libraries where the loudness
    // columns existed but were never populated. Without this, upgrading to
    // v3.0+ would silently leave loudness normalization broken for all
    // tracks that existed before the upgrade.
    let unanalyzed = if force_loudness_backfill {
        db.get_tracks_missing_analysis()
            .context("Failed to get tracks missing analysis")?
    } else {
        db.get_unanalyzed_tracks()
            .context("Failed to get unanalyzed tracks")?
    };

    if unanalyzed.is_empty() {
        info!("All tracks already analyzed.");
        return Ok(());
    }

    info!("Analyzing {} tracks...", unanalyzed.len());
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for track in &unanalyzed {
        let path = PathBuf::from(&track.path);
        match analyze_file(&path, Some(60.0)) {
            Ok(analysis) => {
                if track.bpm.is_none() {
                    if let Err(e) = db.update_bpm(track.id, analysis.bpm.bpm) {
                        warn!("Failed to update BPM for track {}: {}", track.id, e);
                    }
                }
                // Persist loudness metadata (EBU R128 + ReplayGain 2.0) if not
                // already present. Since v3.0.0 the analysis pass produces real
                // loudness values; previously these DB columns existed but were
                // never populated, so loudness normalization did nothing.
                if track.replaygain_track_db.is_none() && track.ebu_r128_loudness.is_none() {
                    let meta = tc_db::LoudnessMeta {
                        track_id: track.id,
                        replaygain_track_db: analysis.loudness.replaygain_track_db,
                        replaygain_album_db: None,
                        replaygain_track_peak: analysis.loudness.replaygain_track_peak,
                        replaygain_album_peak: None,
                        ebu_r128_loudness: analysis.loudness.ebu_r128_loudness,
                        ebu_r128_peak: analysis.loudness.ebu_r128_peak,
                    };
                    if let Err(e) = db.update_loudness_meta(track.id, &meta) {
                        warn!(
                            "Failed to update loudness meta for track {}: {}",
                            track.id, e
                        );
                    }
                }
                succeeded += 1;
            },
            Err(e) => {
                warn!("Analysis failed for {}: {}", track.path, e);
                failed += 1;
            },
        }
    }

    info!(
        "Analysis complete: {} succeeded, {} failed",
        succeeded, failed
    );
    Ok(())
}

#[cfg(feature = "audio-output")]
fn run_with_audio(
    args: &[String],
    _config: tc_config::AppConfig,
    _db: Arc<tc_db::Database>,
    _library: Arc<tc_library::LibraryManager>,
    engine: &mut tc_engine::AudioEngine,
    mut platform: Option<tc_platform::PlatformIntegration>,
    media_key_rx: Option<tc_platform::MediaKeyReceiver>,
) -> Result<()> {
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };

    // If a file path was provided as argument, play it.
    // L18: Args are collected once by run_headless(); pass the slice here.
    let file_path = args.iter().skip(1).find(|a| !a.starts_with('-'));
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
                },
                Err(e) => {
                    error!("Failed to load track: {}", e);
                },
            }
        } else {
            // v3.1.1: Previously a typo'd path was silently ignored, leaving
            // the user wondering why nothing was playing. Log a warning so
            // the failure mode is visible.
            warn!("File not found: {}", path.display());
        }
    }

    // Main event loop
    info!("Entering main loop. Press Ctrl+C to exit.");
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C, shutting down...");
        running.store(false, Ordering::Relaxed);
    })
    .context("Failed to set Ctrl+C handler")?;

    let mut _headless_shuffle = false;

    // v3.1.1: Last time we pushed a Position update to MPRIS. Used to throttle
    // the D-Bus PropertiesChanged signal to 1 Hz (was ~200 Hz at the previous
    // 5 ms loop sleep — see the comment near set_mpris_position below).
    let mut last_mpris_position_update = std::time::Instant::now();

    while r.load(Ordering::Relaxed) {
        engine.tick();

        // Process media key events via the separate MediaKeyReceiver
        if let Some(ref rx) = media_key_rx {
            while let Some(action) = rx.try_recv() {
                use tc_platform::MediaKeyAction;
                match action {
                    MediaKeyAction::Play => {
                        engine.send_command(tc_engine::buffer::EngineCommand::Play);
                    },
                    MediaKeyAction::Pause => {
                        engine.send_command(tc_engine::buffer::EngineCommand::Pause);
                    },
                    MediaKeyAction::PlayPause => {
                        let info = engine.playback_info();
                        if info.state == tc_engine::buffer::PlaybackState::Playing {
                            engine.send_command(tc_engine::buffer::EngineCommand::Pause);
                        } else {
                            engine.send_command(tc_engine::buffer::EngineCommand::Play);
                        }
                    },
                    MediaKeyAction::Next => {
                        // v3.1.1: In headless mode there is no PlaybackService
                        // to manage the queue, and EngineCommand::NextTrack is
                        // an intentional no-op in the engine (queue management
                        // lives in the UI layer). Log a one-shot warning so
                        // users running headless daemon mode know MPRIS next/prev
                        // aren't supported here.
                        log::warn!(
                            "NextTrack ignored in headless mode — queue management \
                             requires the GUI (run without --headless/--cli)"
                        );
                    },
                    MediaKeyAction::Previous => {
                        log::warn!(
                            "PrevTrack ignored in headless mode — queue management \
                             requires the GUI (run without --headless/--cli)"
                        );
                    },
                    MediaKeyAction::Stop => {
                        engine.send_command(tc_engine::buffer::EngineCommand::Stop);
                    },
                    MediaKeyAction::Quit => {
                        info!("Quit requested via MPRIS");
                        r.store(false, Ordering::Relaxed);
                    },

                    MediaKeyAction::Seek(offset_us) => {
                        let info = engine.playback_info();
                        let current_pos = info.position_secs;
                        let offset_secs = offset_us as f32 / 1_000_000.0;
                        let new_pos = (current_pos + offset_secs).max(0.0);
                        info!(
                            "MPRIS Seek: offset={:.1}s, new_pos={:.1}s",
                            offset_secs, new_pos
                        );
                        engine.send_command(tc_engine::buffer::EngineCommand::Seek(new_pos));
                    },

                    MediaKeyAction::SetPosition {
                        track_id: _,
                        position_us,
                    } => {
                        let pos_secs = position_us as f32 / 1_000_000.0;
                        info!("MPRIS SetPosition: {:.1}s", pos_secs);
                        engine.send_command(tc_engine::buffer::EngineCommand::Seek(pos_secs));
                    },

                    MediaKeyAction::SetVolume(vol) => {
                        info!("MPRIS SetVolume: {:.2}", vol);
                        engine.send_command(tc_engine::buffer::EngineCommand::SetVolume(
                            vol.clamp(0.0, 1.0),
                        ));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_volume(vol);
                        }
                    },

                    MediaKeyAction::SetRate(rate) => {
                        info!("MPRIS SetRate: {:.2}", rate);
                        engine.send_command(tc_engine::buffer::EngineCommand::SetSpeed(rate));
                    },

                    MediaKeyAction::SetShuffle(shuffle) => {
                        info!("MPRIS SetShuffle: {}", shuffle);
                        _headless_shuffle = shuffle;
                        engine.send_command(tc_engine::buffer::EngineCommand::SetShuffle(shuffle));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_shuffle(shuffle);
                        }
                    },

                    MediaKeyAction::SetLoopStatus(status) => {
                        info!("MPRIS SetLoopStatus: {}", status);
                        engine.send_command(tc_engine::buffer::EngineCommand::SetLoopStatus(
                            status.clone(),
                        ));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_loop_status(&status);
                        }
                    },

                    MediaKeyAction::OpenUri(uri) => {
                        info!("MPRIS OpenUri: {}", uri);
                        engine.send_command(tc_engine::buffer::EngineCommand::OpenUri(uri));
                    },
                    // VolumeUp/VolumeDown/Mute — helper actions
                    MediaKeyAction::VolumeUp => {
                        let vol = engine.playback_info().volume;
                        let new_vol = (vol + 0.05).min(1.0);
                        engine.send_command(tc_engine::buffer::EngineCommand::SetVolume(new_vol));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_volume(new_vol);
                        }
                    },
                    MediaKeyAction::VolumeDown => {
                        let vol = engine.playback_info().volume;
                        let new_vol = (vol - 0.05).max(0.0);
                        engine.send_command(tc_engine::buffer::EngineCommand::SetVolume(new_vol));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_volume(new_vol);
                        }
                    },
                    MediaKeyAction::Mute => {
                        engine.send_command(tc_engine::buffer::EngineCommand::SetVolume(0.0));
                        if let Some(ref mut p) = platform {
                            p.set_mpris_volume(0.0);
                        }
                    },
                    _ => {},
                }
            }
        }

        // v3.1.1: Hoist the per-iteration playback_info() call once instead
        // of calling it twice (for MPRIS + for sleep). Each call clones an
        // ArcSwap'd PlaybackInfo, so doubling it was wasted work.
        let info = engine.playback_info();
        let has_pending = engine.has_pending_chunk();

        // v3.1.1: Throttle MPRIS position updates to 1 Hz (was 200 Hz at the
        // old 5 ms sleep interval). Each set_mpris_position call goes through
        // a D-Bus PropertiesChanged signal (1–5 ms IPC), so pushing on every
        // loop iteration was responsible for ~20–30% CPU usage on Linux.
        // MPRIS clients compute live position from Position + Rate + elapsed
        // time, so a 1-second refresh is indistinguishable from per-frame
        // updates. We also skip the update entirely when not Playing.
        if info.state == tc_engine::buffer::PlaybackState::Playing {
            if let Some(ref mut p) = platform {
                if last_mpris_position_update.elapsed()
                    >= std::time::Duration::from_millis(1000)
                {
                    // L15: Direct `as i64` cast overflows if position_secs is
                    // negative or very large. Clamp to [0, i64::MAX] range.
                    let pos_us =
                        (info.position_secs * 1_000_000.0).clamp(0.0, i64::MAX as f32) as i64;
                    p.set_mpris_position(pos_us);
                    last_mpris_position_update = std::time::Instant::now();
                }
            }
        }

        // v3.1.1: Match the v3.0.0 CHANGELOG sleep schedule (20 ms / 100 ms)
        // instead of the old 5 ms / 50 ms. The 5 ms rate was burning ~10–15 %
        // CPU on the headless daemon, exactly the issue the CHANGELOG claimed
        // to have fixed (but only fixed in the GUI path).
        let sleep_ms = if info.state == tc_engine::buffer::PlaybackState::Playing && !has_pending {
            20
        } else {
            100
        };
        std::thread::sleep(Duration::from_millis(sleep_ms));
    }

    info!("Shutting down...");
    engine.stop();
    info!("Goodbye!");
    Ok(())
}
