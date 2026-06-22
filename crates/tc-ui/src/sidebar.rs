//! Sidebar bridge — syncs sidebar state to the Slint App component.
//!
//! In the egui version this was an imperative `draw()` function that
//! painted the sidebar every frame. With Slint, the sidebar is declared
//! in `ui/components/sidebar.slint` and we just push state into properties.

use slint::{ModelRc, SharedString};

use crate::{
    app::TuneCraftApp,
    converters::nav_item,
    App,
};

/// Which navigation section is active.
///
/// Moved here from the old egui sidebar.rs. The enum is UI-only — the
/// Slint side uses string identifiers ("all_tracks", "folders", etc.)
/// via `section_id()` / `parse_section()`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavSection {
    AllTracks,
    Albums,
    Artists,
    Folders,
    Favorites,
    RecentlyPlayed,
    MostPlayed,
    Settings,
}

impl NavSection {
    pub fn label(&self) -> &'static str {
        match self {
            Self::AllTracks => "All Tracks",
            Self::Albums => "Albums",
            Self::Artists => "Artists",
            Self::Folders => "Folders",
            Self::Favorites => "Favorites",
            Self::RecentlyPlayed => "Recently Played",
            Self::MostPlayed => "Most Played",
            Self::Settings => "Settings",
        }
    }

    /// Compute the badge count for this section based on the current tracks.
    /// Returns `None` if no badge should be shown.
    pub fn badge_count(&self, tracks: &[tc_db::Track]) -> Option<u32> {
        match self {
            Self::AllTracks => Some(tracks.len() as u32),
            Self::Favorites => None,
            Self::RecentlyPlayed => {
                let now = chrono::Utc::now().naive_utc();
                Some(
                    tracks
                        .iter()
                        .filter(|t| t.last_played.is_some_and(|dt| (now - dt).num_hours() <= 48))
                        .count() as u32,
                )
            },
            Self::MostPlayed => {
                Some(tracks.iter().filter(|t| t.play_count > 3).count().min(30) as u32)
            },
            _ => None,
        }
    }
}

/// Section identifier strings used by the Slint UI. These match the
/// `nav-clicked(section)` callback parameter.
pub fn section_id(s: NavSection) -> &'static str {
    match s {
        NavSection::AllTracks => "all_tracks",
        NavSection::Albums => "albums",
        NavSection::Artists => "artists",
        NavSection::Folders => "folders",
        NavSection::Favorites => "favorites",
        NavSection::RecentlyPlayed => "recently_played",
        NavSection::MostPlayed => "most_played",
        NavSection::Settings => "settings",
    }
}

/// Convert a section string back to the NavSection enum.
pub fn parse_section(s: &str) -> NavSection {
    match s {
        "all_tracks" => NavSection::AllTracks,
        "albums" => NavSection::Albums,
        "artists" => NavSection::Artists,
        "folders" => NavSection::Folders,
        "favorites" => NavSection::Favorites,
        "recently_played" => NavSection::RecentlyPlayed,
        "most_played" => NavSection::MostPlayed,
        "settings" => NavSection::Settings,
        _ => NavSection::AllTracks,
    }
}

/// Build the nav-items model for the sidebar.
pub fn build_nav_items(app: &TuneCraftApp) -> ModelRc<crate::NavItem> {
    let sections = [
        (NavSection::AllTracks, "All Tracks", "icons/music-notes.svg"),
        (NavSection::Albums, "Albums", "icons/squares-four.svg"),
        (NavSection::Artists, "Artists", "icons/users.svg"),
        (NavSection::Folders, "Folders", "icons/folder.svg"),
        (NavSection::Favorites, "Favorites", "icons/star.svg"),
        (NavSection::RecentlyPlayed, "Recently Played", "icons/clock-counter-clockwise.svg"),
        (NavSection::MostPlayed, "Most Played", "icons/chart-bar.svg"),
    ];

    let items: Vec<crate::NavItem> = sections
        .iter()
        .map(|(section, label, icon_path)| {
            // Bug fix: `compute_badge_counts` (services/library.rs) keys its
            // map with `format!("{:?}", NavSection)` (e.g. "AllTracks"), not
            // the human-readable label ("All Tracks"). Looking it up by
            // `label` here never matched, so badges silently always showed 0.
            let badge = app
                .badge_cache
                .get(&format!("{:?}", section))
                .copied()
                .unwrap_or(0);
            nav_item(
                section_id(*section),
                label,
                load_icon(icon_path),
                badge,
                app.nav == *section,
            )
        })
        .collect();

    ModelRc::new(slint::VecModel::from(items))
}

/// Build the playlists model for the sidebar.
pub fn build_playlists(app: &TuneCraftApp) -> ModelRc<crate::PlaylistItem> {
    let items: Vec<crate::PlaylistItem> = app
        .playlists
        .iter()
        .map(|pl| crate::converters::playlist_to_item(pl, 0))
        .collect();
    ModelRc::new(slint::VecModel::from(items))
}

/// Sync the sidebar's dynamic state (search query, scanning status, active section).
pub fn sync_sidebar(app: &TuneCraftApp, slint_app: &App) {
    slint_app.set_search_query(SharedString::from(app.search_query.clone()));
    slint_app.set_is_scanning(app.is_scanning);
    slint_app.set_status_message(SharedString::from(app.status_message.clone()));
    slint_app.set_sidebar_collapsed(app.sidebar_collapsed);
    slint_app.set_active_section(SharedString::from(section_id(app.nav)));
    slint_app.set_nav_items(build_nav_items(app));
    slint_app.set_playlists(build_playlists(app));
}

/// Load an SVG icon from the embedded icons directory.
///
/// `slint::Image` is not `Send`/`Sync`, so we can't cache it in a static.
/// Loading from disk is cheap (single file read + SVG parse) and only
/// happens on sidebar rebuild (every 200ms at most).
pub fn load_icon(path: &str) -> slint::Image {
    let icons_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let full_path = icons_dir.join(path);
    slint::Image::load_from_path(&full_path).unwrap_or_default()
}

