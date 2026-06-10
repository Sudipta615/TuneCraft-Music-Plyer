//! AppContext — Service Registry
//!
//! The shared application context that connects UI to all backends via services.
//! Unlike the previous version which held `Arc<Mutex<AudioEngine>>`,
//! `Arc<Mutex<PlatformIntegration>>`, etc., this version holds typed service
//! objects that encapsulate synchronization internally. The UI never directly
//! locks a Mutex or RwLock — it calls service methods.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use log::{error, info, warn};
use tokio::runtime::Runtime;

#[cfg(feature = "audio-output")]
use crate::services::playback::EngineHandle;

use crate::services::{
    ConfigService, EqService, LibraryService, PlatformService, PlaybackService, ScrobbleService,
};

/// The shared application context that connects UI to all backends via services.
///
/// Implements Drop for cleanup on exit.
pub struct AppContext {
    pub playback: Arc<PlaybackService>,
    pub library: Arc<LibraryService>,
    pub eq: Arc<EqService>,
    pub scrobble: Arc<ScrobbleService>,
    pub config: Arc<ConfigService>,
    pub platform: Arc<PlatformService>,

    pub tokio_runtime: Arc<Runtime>,

    #[cfg(feature = "audio-output")]
    pub(crate) engine_running: Option<Arc<AtomicBool>>,
    #[cfg(feature = "audio-output")]
    pub(crate) engine_thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for AppContext {
    fn drop(&mut self) {
        info!("AppContext dropping — initiating graceful shutdown");

        #[cfg(feature = "audio-output")]
        if let Some(ref running) = self.engine_running {
            running.store(false, Ordering::Relaxed);
            info!("Engine stop signal sent");
        }

        #[cfg(feature = "audio-output")]
        {
            let state = self.playback.state();
            drop(state);
            self.playback.stop_engine();
        }

        #[cfg(feature = "audio-output")]
        if let Some(handle) = self.engine_thread_handle.take() {
            info!("Joining engine tick thread...");
            if let Err(e) = handle.join() {
                warn!("Engine tick thread join error: {:?}", e);
            }
        }

        // Local scrobble service: nothing to flush to network.
        // SQLite writes are synchronous — every record() call already committed.
        info!("Local scrobble journal is up to date — no flush needed.");

        if let Err(e) = self.config.force_save() {
            warn!("Failed to save config during shutdown: {}", e);
        }

        info!("AppContext shutdown complete");
    }
}

impl AppContext {
    /// Initialize all subsystems and return a fully wired AppContext.
    pub fn init() -> Result<Self, Box<dyn std::error::Error>> {
        let config = tc_config::ConfigPersistence::load_or_default();
        let config = Arc::new(std::sync::RwLock::new(config));

        let db_path = dirs::data_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
            .ok_or({
                "Cannot determine a suitable data directory. \
                 Set XDG_DATA_HOME or HOME so TuneCraft knows where to store \
                 its database. Refusing to fall back to /tmp \
                 because data written there does not survive a reboot."
            })?
            .join("tunecraft")
            .join("library.db");

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = tc_db::Database::open(&db_path)?;
        let db = Arc::new(db);
        info!("Database opened at {}", db_path.display());

        let tokio_runtime =
            Arc::new(Runtime::new().map_err(|e| format!("Failed to create tokio runtime: {}", e))?);

        let (mut platform, media_key_rx) = tc_platform::PlatformIntegration::new()?;
        if let Err(e) = platform.register_mpris("TuneCraft") {
            if !matches!(e, tc_platform::PlatformError::NotAvailable(_)) {
                warn!("MPRIS registration failed: {}", e);
            }
        }
        if let Err(e) = platform.start_media_key_listener() {
            warn!("Media key listener failed: {}", e);
        }
        let platform = Arc::new(PlatformService::new(platform, media_key_rx));

        // Local offline scrobble service — reads scrobble.enabled from config
        // to allow the user to disable play-count tracking entirely if they wish.
        let scrobble_enabled = config.read().map(|c| c.scrobble.enabled).unwrap_or(true); // default: enabled (local-only, no privacy concern)
        let scrobble = Arc::new(ScrobbleService::new(Arc::clone(&db), scrobble_enabled));

        let library_config = config
            .read()
            .map(|c| c.library.clone())
            .unwrap_or_else(|_| tc_config::LibraryConfig::with_system_defaults());
        let library = Arc::new(tc_library::LibraryManager::new(
            Arc::clone(&db),
            library_config,
        ));

        let scan_complete = Arc::new(AtomicBool::new(true));
        let scan_failed = Arc::new(AtomicBool::new(false));

        let (scan_progress_tx, scan_progress_rx) = crossbeam::channel::bounded(64);

        let should_scan = config
            .read()
            .map(|c| c.library.scan_on_startup)
            .unwrap_or(false);
        if should_scan {
            info!("Starting library scan...");
            let lib_clone = Arc::clone(&library);
            let scan_done = Arc::clone(&scan_complete);
            let scan_fail_flag = Arc::clone(&scan_failed);
            scan_done.store(false, Ordering::Relaxed);
            std::thread::spawn(move || {
                match lib_clone.scan(|progress| {
                    if progress.files_processed % 10 == 0
                        || progress.files_processed == progress.files_found
                    {
                        let _ = scan_progress_tx.send(progress.clone());
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
                        scan_fail_flag.store(true, Ordering::Relaxed);
                    },
                }
                scan_done.store(true, Ordering::Relaxed);
            });
        }

        let db_dirty = Arc::new(AtomicBool::new(false));

        let config_service = Arc::new(ConfigService::new(Arc::clone(&config)));

        let tracks_per_page = config
            .read()
            .map(|c| c.library.tracks_per_page)
            .unwrap_or(500);
        let library_service = Arc::new(LibraryService::new(
            db,
            library,
            scan_complete,
            scan_failed,
            db_dirty,
            tracks_per_page,
            scan_progress_rx,
        ));

        let (volume, shuffle, repeat, speed, eq_enabled, eq_preamp, eq_dither, eq_bands) = {
            let cfg = config.read().map(|c| c.clone()).unwrap_or_default();
            let mut bands = [0.0; 10];
            for (i, band) in cfg.engine.eq.bands.iter().enumerate() {
                if i < 10 {
                    bands[i] = band.gain_db;
                }
            }
            (
                cfg.playback.volume,
                cfg.playback.shuffle,
                cfg.playback.repeat,
                cfg.playback.speed,
                cfg.engine.eq.enabled,
                cfg.engine.eq.preamp_db,
                cfg.engine.dither_enabled,
                bands,
            )
        };

        #[cfg(feature = "audio-output")]
        let (engine_handle, engine_mutex, engine_running, cmd_tx, engine_thread_handle) = {
            let engine_config = config.read().map(|c| c.engine.clone()).unwrap_or_default();
            let mut engine = tc_engine::AudioEngine::new(engine_config)?;
            engine.start()?;
            let engine_running = Arc::new(AtomicBool::new(true));

            let cmd_tx = engine.send_command_channel();
            let playback_info = engine.playback_info_arc();

            let engine_handle =
                EngineHandle::new(cmd_tx.clone(), playback_info, Arc::clone(&engine_running));
            let engine_mutex = Arc::new(std::sync::Mutex::new(engine));

            let engine_clone = Arc::clone(&engine_mutex);
            let running_clone = Arc::clone(&engine_running);
            let engine_thread_handle: std::thread::JoinHandle<()> = std::thread::Builder::new()
                .name("tunecraft-engine-tick".to_string())
                .spawn(move || {
                    info!("Engine tick thread started");
                    while running_clone.load(Ordering::Relaxed) {
                        if let Ok(mut eng) = engine_clone.lock() {
                            eng.tick();
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    info!("Engine tick thread stopped");
                })
                .map_err(|e| format!("Failed to spawn engine tick thread: {}", e))?;

            (
                engine_handle,
                engine_mutex,
                engine_running,
                cmd_tx,
                Some(engine_thread_handle),
            )
        };

        #[cfg(feature = "audio-output")]
        let playback = Arc::new(PlaybackService::new(
            engine_handle,
            engine_mutex,
            Arc::clone(&platform),
            Arc::clone(&scrobble),
            Arc::clone(&tokio_runtime),
            volume,
            shuffle,
            repeat,
            speed,
        ));

        #[cfg(not(feature = "audio-output"))]
        let playback = Arc::new(PlaybackService::new(
            Arc::clone(&platform),
            Arc::clone(&scrobble),
            Arc::clone(&tokio_runtime),
            volume,
            shuffle,
            repeat,
            speed,
        ));

        #[cfg(feature = "audio-output")]
        let eq = Arc::new(EqService::new(
            cmd_tx, eq_enabled, eq_preamp, eq_dither, eq_bands,
        ));

        #[cfg(not(feature = "audio-output"))]
        let eq = Arc::new(EqService::new(eq_enabled, eq_preamp, eq_dither, eq_bands));

        Ok(Self {
            playback,
            library: library_service,
            eq,
            scrobble,
            config: config_service,
            platform,
            tokio_runtime,

            #[cfg(feature = "audio-output")]
            engine_running: Some(engine_running),
            #[cfg(feature = "audio-output")]
            engine_thread_handle,
        })
    }
}
