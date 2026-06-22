//! Library and polling delegation methods for TuneCraftApp.

use std::path::PathBuf;
use std::sync::Arc;

use super::{ToastLevel, TuneCraftApp};

impl TuneCraftApp {
    pub fn refresh_tracks(&mut self) {
        self.ctx.library.refresh_tracks();
        let snapshot = self.ctx.library.snapshot();
        self.tracks = snapshot.tracks.clone();
        self.cached_favorite_ids = snapshot.favorite_ids.clone();
        self.total_track_count = snapshot.total_track_count;
        self.badge_cache = self.ctx.library.compute_badge_counts();
    }

    pub fn reload_playlists(&mut self) {
        self.ctx.library.refresh_playlists();
        let snapshot = self.ctx.library.snapshot();
        self.playlists = snapshot.playlists.clone();
    }

    pub fn create_playlist(&mut self, name: &str) {
        match self.ctx.library.create_playlist(name) {
            Ok(id) => {
                self.selected_playlist_id = Some(id);
                self.push_toast(format!("Playlist '{}' created", name), ToastLevel::Success);
                let snapshot = self.ctx.library.snapshot();
                self.playlists = snapshot.playlists.clone();
            },
            Err(e) => {
                self.push_toast(e, ToastLevel::Error);
            },
        }
    }

    pub fn add_track_to_playlist(&mut self, playlist_id: i64, track_id: i64) {
        match self
            .ctx
            .library
            .add_track_to_playlist(playlist_id, track_id)
        {
            Ok(()) => self.push_toast("Track added to playlist", ToastLevel::Success),
            Err(e) => self.push_toast(format!("Failed to add track: {}", e), ToastLevel::Error),
        }
    }

    pub fn add_current_track_to_playlist(&mut self, playlist_id: i64) {
        if let Some(track_id) = self.current_track_id {
            self.add_track_to_playlist(playlist_id, track_id);
        }
    }

    /// Add a music folder to the library watch dirs and trigger a background scan.
    ///
    /// 1. Validates the path exists and is a directory.
    /// 2. Persists the path to config so it survives restarts.
    /// 3. Spawns a background thread that creates a temporary `LibraryManager`
    ///    with the updated config and calls `.scan()`.
    /// 4. After the scan completes, calls `library_service.refresh_tracks()` so
    ///    the UI picks up the new tracks on the next frame.
    pub fn add_music_folder(&mut self, folder_path: &str) {
        let path = PathBuf::from(folder_path);

        if !path.exists() || !path.is_dir() {
            self.push_toast(
                format!("Folder not found or is not a directory: {}", folder_path),
                ToastLevel::Error,
            );
            return;
        }

        // Persist the new dir to config
        let path_clone = path.clone();
        self.ctx.config.write(|c| {
            if !c.library.watch_dirs.contains(&path_clone) {
                c.library.watch_dirs.push(path_clone.clone());
            }
        });

        // Read the full updated library config (with the new dir)
        let new_lib_config = self
            .ctx
            .config
            .read(|c| c.library.clone())
            .unwrap_or_else(|| {
                let mut cfg = tc_config::LibraryConfig::default();
                cfg.watch_dirs.push(path.clone());
                cfg
            });

        // Build a temporary LibraryManager with updated config
        let db = self.ctx.library.db().clone();
        let scan_manager = Arc::new(tc_library::LibraryManager::new(db, new_lib_config));

        // Clone the library service handle so the scan thread can refresh after completion
        let library_svc = Arc::clone(&self.ctx.library);

        std::thread::Builder::new()
            .name("tunecraft-add-music-scan".into())
            .spawn(move || {
                log::info!("Starting add-music scan...");
                match scan_manager.scan(|_progress| {
                    // progress updates are handled by check_scan_state() via the
                    // existing scan_progress_rx channel; this scan uses a separate
                    // manager so we just ignore individual updates here.
                }) {
                    Ok(result) => {
                        log::info!(
                            "Add-music scan complete: {} added, {} updated",
                            result.files_added,
                            result.files_updated
                        );
                    },
                    Err(e) => {
                        log::warn!("Add-music scan failed: {}", e);
                    },
                }
                // Always refresh even on partial success
                library_svc.refresh_tracks();
                library_svc.refresh_favorite_ids();
                library_svc.mark_db_dirty();
            })
            .ok();

        self.push_toast(
            format!(
                "Scanning '{}'… new tracks will appear shortly.",
                folder_path
            ),
            ToastLevel::Success,
        );
    }

    /// Add individual music files to the library via native file picker.
    ///
    /// Unlike `add_music_folder`, this does NOT add the parent directory to
    /// `watch_dirs`. Each selected file is scanned and inserted directly.
    pub fn add_music_files(&mut self, paths: Vec<std::path::PathBuf>) {
        if paths.is_empty() {
            return;
        }

        let count = paths.len();
        let db = self.ctx.library.db().clone();
        let lib_config = self
            .ctx
            .config
            .read(|c| c.library.clone())
            .unwrap_or_default();
        let scan_manager = Arc::new(tc_library::LibraryManager::new(db, lib_config));
        let library_svc = Arc::clone(&self.ctx.library);

        std::thread::Builder::new()
            .name("tunecraft-add-files-scan".into())
            .spawn(move || {
                log::info!("Adding {} individual music file(s)...", count);
                let added = scan_manager.scan_files(&paths);
                log::info!("Add-files complete: {} of {} added", added, count);
                library_svc.refresh_tracks();
                library_svc.refresh_favorite_ids();
                library_svc.mark_db_dirty();
                // v3.1.1: Kick off background BPM / loudness analysis for
                // the newly-added tracks. Previously this was only done at
                // startup, so tracks added at runtime never got BPM / EBU R128
                // values until the user restarted the app — silently breaking
                // loudness normalization and BPM display.
                trigger_bg_analysis_via_service(&library_svc);
            })
            .ok();

        let label = if count == 1 {
            "Adding 1 file…".to_string()
        } else {
            format!("Adding {} files…", count)
        };
        self.push_toast(label, ToastLevel::Success);
    }

    /// Add multiple music folders to the library via native folder picker.
    ///
    /// Each folder is added to `watch_dirs` in config and scanned in a
    /// background thread.
    pub fn add_music_folders(&mut self, folders: Vec<std::path::PathBuf>) {
        if folders.is_empty() {
            return;
        }

        // Persist all folders to config
        let folders_clone = folders.clone();
        self.ctx.config.write(|c| {
            for folder in &folders_clone {
                if !c.library.watch_dirs.contains(folder) {
                    c.library.watch_dirs.push(folder.clone());
                }
            }
        });

        let new_lib_config = self
            .ctx
            .config
            .read(|c| c.library.clone())
            .unwrap_or_default();

        let db = self.ctx.library.db().clone();
        let scan_manager = Arc::new(tc_library::LibraryManager::new(db, new_lib_config));
        let library_svc = Arc::clone(&self.ctx.library);
        let count = folders.len();

        std::thread::Builder::new()
            .name("tunecraft-add-folders-scan".into())
            .spawn(move || {
                log::info!("Starting scan for {} folder(s)...", count);
                match scan_manager.scan(|_progress| {}) {
                    Ok(result) => {
                        log::info!(
                            "Add-folders scan complete: {} added, {} updated",
                            result.files_added,
                            result.files_updated
                        );
                    },
                    Err(e) => {
                        log::warn!("Add-folders scan failed: {}", e);
                    },
                }
                library_svc.refresh_tracks();
                library_svc.refresh_favorite_ids();
                library_svc.mark_db_dirty();
                // v3.1.1: Kick off background analysis for newly-added tracks.
                trigger_bg_analysis_via_service(&library_svc);
            })
            .ok();

        let label = if count == 1 {
            format!(
                "Scanning '{}'…",
                folders[0]
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )
        } else {
            format!("Scanning {} folders…", count)
        };
        self.push_toast(label, ToastLevel::Success);
    }

    /// Poll media key actions from the platform service.
    pub fn poll_media_keys(&mut self) {
        while let Some(action) = self.ctx.platform.try_recv_action() {
            use tc_platform::MediaKeyAction;
            match action {
                MediaKeyAction::Play if !self.is_playing => {
                    self.toggle_playback();
                },
                MediaKeyAction::Pause if self.is_playing => {
                    self.toggle_playback();
                },
                MediaKeyAction::Play | MediaKeyAction::Pause => {},
                MediaKeyAction::PlayPause => {
                    self.toggle_playback();
                },
                MediaKeyAction::Next => {
                    self.play_next();
                },
                MediaKeyAction::Previous => {
                    self.play_prev();
                },
                MediaKeyAction::Stop => {
                    self.stop_playback();
                },
                MediaKeyAction::SetVolume(vol) => {
                    self.set_volume(vol);
                },
                // v3.1.1: Handle the previously-dropped MediaKeyAction variants.
                // External MPRIS clients (playerctl, KDE Connect, GNOME Shell)
                // send these — previously they were silently swallowed by the
                // `_ => {}` catch-all.
                MediaKeyAction::VolumeUp => {
                    let new_vol = (self.volume + 0.05).min(1.0);
                    self.set_volume(new_vol);
                },
                MediaKeyAction::VolumeDown => {
                    let new_vol = (self.volume - 0.05).max(0.0);
                    self.set_volume(new_vol);
                },
                MediaKeyAction::Mute => {
                    self.toggle_mute();
                },
                MediaKeyAction::SetRate(rate) => {
                    self.set_speed(rate);
                },
                MediaKeyAction::SetShuffle(shuffle) => {
                    self.shuffle = shuffle;
                    self.ctx.playback.set_shuffle(shuffle);
                },
                MediaKeyAction::SetLoopStatus(status) => {
                    use tc_config::RepeatMode;
                    let mode = match status.as_str() {
                        "Track" => RepeatMode::One,
                        "Playlist" => RepeatMode::All,
                        _ => RepeatMode::Off,
                    };
                    self.set_repeat(mode);
                },
                MediaKeyAction::OpenUri(uri) => {
                    // Forward file:// URIs to the engine, which already
                    // implements path canonicalization + sandboxing against
                    // home/audio dirs.
                    if uri.starts_with("file://") {
                        use tc_engine::buffer::EngineCommand;
                        self.ctx.playback.send_command(EngineCommand::OpenUri(uri));
                    } else {
                        log::warn!("OpenUri: only file:// URIs are supported, got: {}", uri);
                    }
                },
                MediaKeyAction::Quit => {
                    log::info!("Quit requested via MPRIS / media key");
                    self.close_requested = true;
                },
                MediaKeyAction::Seek(offset_us) => {
                    let offset_secs = offset_us as f32 / 1_000_000.0;
                    let new_pos = (self.position_secs + offset_secs).max(0.0);
                    self.seek(new_pos);
                },
                MediaKeyAction::SetPosition {
                    track_id: _,
                    position_us,
                } => {
                    let pos_secs = position_us as f32 / 1_000_000.0;
                    self.seek(pos_secs);
                },
                MediaKeyAction::ToggleShuffle => {
                    self.toggle_shuffle();
                },
                MediaKeyAction::ToggleRepeat => {
                    self.toggle_repeat();
                },
                MediaKeyAction::GlobalSearch => {
                    self.focus_search = true;
                },
            }
        }
    }

    pub fn trigger_background_analysis(&self) {
        let db = self.ctx.library.db().clone();
        let library_svc = Arc::clone(&self.ctx.library);

        std::thread::Builder::new()
            .name("tunecraft-audio-analysis".into())
            .spawn(move || {
                use std::path::PathBuf;
                use tc_analysis::analyze_file;

                // v3.1.1: Use get_tracks_missing_analysis instead of
                // get_unanalyzed_tracks so we also backfill loudness for
                // tracks that have BPM but lack EBU R128 / ReplayGain.
                // This is the common case for libraries created before v3.0.0,
                // where the loudness columns existed but were never populated.
                let unanalyzed = match db.get_tracks_missing_analysis() {
                    Ok(tracks) => tracks,
                    Err(_) => return,
                };

                if unanalyzed.is_empty() {
                    return;
                }

                let mut changed = false;
                for track in &unanalyzed {
                    let path = PathBuf::from(&track.path);
                    if let Ok(analysis) = analyze_file(&path, Some(60.0)) {
                        if track.bpm.is_none() {
                            let _ = db.update_bpm(track.id, analysis.bpm.bpm);
                            changed = true;
                        }
                        // Persist loudness metadata if not yet computed. Since v3.0.0
                        // the analysis pass produces real EBU R128 / ReplayGain 2.0
                        // values; previously the DB columns existed but were never
                        // populated, so loudness normalization silently did nothing.
                        if track.replaygain_track_db.is_none() && track.ebu_r128_loudness.is_none()
                        {
                            let meta = tc_db::LoudnessMeta {
                                track_id: track.id,
                                replaygain_track_db: analysis.loudness.replaygain_track_db,
                                replaygain_album_db: None, // album gain computed separately on album batches
                                replaygain_track_peak: analysis.loudness.replaygain_track_peak,
                                replaygain_album_peak: None,
                                ebu_r128_loudness: analysis.loudness.ebu_r128_loudness,
                                ebu_r128_peak: analysis.loudness.ebu_r128_peak,
                            };
                            let _ = db.update_loudness_meta(track.id, &meta);
                            changed = true;
                        }
                    }
                }

                if changed {
                    library_svc.mark_db_dirty();
                    library_svc.refresh_tracks();
                }
            })
            .ok();
    }
}

/// Free function that spawns a background analysis thread given only an
/// `Arc<LibraryService>`. Used by `add_music_files` and `add_music_folders`
/// inside their scan-thread closures — they don't have access to `self` (the
/// `TuneCraftApp`) because they're already running on a spawned thread, so we
/// rebuild the analysis pipeline from the library service alone.
///
/// This is essentially `TuneCraftApp::trigger_background_analysis` extracted
/// into a standalone function so it can be called from thread contexts that
/// only hold an `Arc<LibraryService>`.
fn trigger_bg_analysis_via_service(library_svc: &Arc<crate::services::LibraryService>) {
    let db = library_svc.db().clone();
    let library_svc = Arc::clone(library_svc);

    // Defer slightly so the just-finished scan's refresh_tracks() call has
    // time to land and the new tracks are visible to get_tracks_missing_analysis.
    std::thread::Builder::new()
        .name("tunecraft-add-files-analysis".into())
        .spawn(move || {
            use std::path::PathBuf;
            use tc_analysis::analyze_file;

            let unanalyzed = match db.get_tracks_missing_analysis() {
                Ok(tracks) => tracks,
                Err(_) => return,
            };
            if unanalyzed.is_empty() {
                return;
            }

            let mut changed = false;
            for track in &unanalyzed {
                let path = PathBuf::from(&track.path);
                if let Ok(analysis) = analyze_file(&path, Some(60.0)) {
                    if track.bpm.is_none() {
                        let _ = db.update_bpm(track.id, analysis.bpm.bpm);
                        changed = true;
                    }
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
                        let _ = db.update_loudness_meta(track.id, &meta);
                        changed = true;
                    }
                }
            }

            if changed {
                library_svc.mark_db_dirty();
                library_svc.refresh_tracks();
            }
        })
        .ok();
}
