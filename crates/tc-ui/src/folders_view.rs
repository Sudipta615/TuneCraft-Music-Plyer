//! Folders view bridge — syncs folder data to the Slint FoldersView component.

use std::collections::HashMap;

use slint::{ModelRc, SharedString};

use crate::{app::TuneCraftApp, converters::folder_to_item, App};

/// Build the folders list for the top-level Folders view.
pub fn build_folders(app: &TuneCraftApp) -> ModelRc<crate::FolderItem> {
    // Group tracks by their parent directory and count.
    let mut folder_counts: HashMap<std::path::PathBuf, u32> = HashMap::new();

    for track in &app.tracks {
        if let Some(parent) = std::path::Path::new(&track.path).parent() {
            *folder_counts.entry(parent.to_path_buf()).or_insert(0) += 1;
        }
    }

    // Also include configured watch dirs (even if empty).
    let watch_dirs: Vec<std::path::PathBuf> = app
        .ctx
        .config
        .read(|c| c.library.watch_dirs.clone())
        .unwrap_or_default();

    for dir in &watch_dirs {
        folder_counts.entry(dir.clone()).or_insert(0);
    }

    let items: Vec<crate::FolderItem> = folder_counts
        .iter()
        .map(|(path, count)| {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path.to_str().unwrap_or("Unknown"))
                .to_string();
            folder_to_item(path.to_str().unwrap_or(""), &name, *count, false)
        })
        .collect();

    ModelRc::new(slint::VecModel::from(items))
}

/// Build the folder-tracks model (tracks inside the currently-opened folder).
pub fn build_folder_tracks(app: &TuneCraftApp) -> ModelRc<crate::TrackItem> {
    let items: Vec<crate::TrackItem> = app
        .folder_tracks
        .iter()
        .map(|track| {
            let is_playing = app.current_track_id == Some(track.id);
            let is_favorite = app.cached_favorite_ids.contains(&track.id);
            crate::converters::track_to_item(
                track,
                is_playing,
                is_playing && !app.is_playing,
                is_favorite,
                slint::Image::default(),
                false,
            )
        })
        .collect();
    ModelRc::new(slint::VecModel::from(items))
}

/// Sync folders view state to Slint.
pub fn sync_folders_view(app: &TuneCraftApp, slint_app: &App) {
    slint_app.set_folders(build_folders(app));
    slint_app.set_folder_tracks(build_folder_tracks(app));
    slint_app.set_current_folder(SharedString::from(
        app.folder_view_path
            .as_ref()
            .and_then(|p| p.to_str())
            .unwrap_or(""),
    ));
    slint_app.set_in_folder(app.folder_view_path.is_some());
}
