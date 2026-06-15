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

    pub fn add_current_track_to_playlist(&mut self, playlist_id: i64) {
        if let Some(track_id) = self.current_track_id {
            match self
                .ctx
                .library
                .add_track_to_playlist(playlist_id, track_id)
            {
                Ok(()) => self.push_toast("Track added to playlist", ToastLevel::Success),
                Err(e) => self.push_toast(e, ToastLevel::Error),
            }
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

    /// Remove a music folder from the library watch dirs and delete its tracks.
    pub fn remove_music_folder(&mut self, folder_path: &str) {
        let path = PathBuf::from(folder_path);

        // Remove from config watch_dirs
        self.ctx.config.write(|c| {
            c.library.watch_dirs.retain(|p| p != &path);
        });

        // Delete tracks from DB
        match self.ctx.library.remove_folder(folder_path) {
            Ok(deleted) => {
                let folder_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.push_toast(
                    format!("Removed folder '{}' ({} tracks)", folder_name, deleted),
                    ToastLevel::Info,
                );
            },
            Err(e) => {
                self.push_toast(e, ToastLevel::Error);
            },
        }

        // We already refreshed tracks in library_service.remove_folder, but we need
        // to sync the UI's track list snapshot.
        self.refresh_tracks();
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
                MediaKeyAction::ToggleShuffle => {
                    self.toggle_shuffle();
                },
                MediaKeyAction::ToggleRepeat => {
                    self.toggle_repeat();
                },
                MediaKeyAction::GlobalSearch => {
                    self.focus_search = true;
                },
                _ => {},
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

                let unanalyzed = match db.get_unanalyzed_tracks() {
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
