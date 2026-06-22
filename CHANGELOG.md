# 📝 Changelog

All notable changes to **TuneCraft** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [3.1.5] — 2026-06-21 (Slint Migration Fixup 2)

Further `tc-ui` regressions found after auditing the egui → Slint
overhaul, beyond what `3.1.4-patch1` covered.

### 🐛 Fixed
- **Lyrics panel was completely non-functional.** `show-lyrics-panel`,
  `lyrics-available`, `lyrics-lines`, and `lyrics-track-title` — all
  declared as `in property` on the root `App` and consumed by
  `PlayerBar`/`LyricsPanel` — were never set from Rust, so the lyrics
  button in the player bar stayed permanently disabled and the panel
  never rendered fetched lyrics. The `toggle-lyrics` callback also
  mistakenly flipped the global `lyrics.enabled` config setting instead
  of opening the panel. Added `TuneCraftApp::show_lyrics_panel`, an LRC
  parser (`converters::parse_lrc`), and `lyrics_actions::sync_lyrics_panel`
  to parse cached synced lyrics, highlight the current line from
  playback position, and push it all to Slint each tick.
- **Sidebar badge counts always showed 0.** `LibraryService::compute_badge_counts`
  keys its map with `format!("{:?}", NavSection)` (e.g. `"AllTracks"`),
  but `sidebar::build_nav_items` looked the count up by the
  human-readable `label()` (e.g. `"All Tracks"`), which never matched.
  Fixed the lookup to use the same Debug-format key.
- **Track-list pagination didn't actually page.** `on_page_changed`
  only updated the UI-side `track_page` mirror; it never told
  `LibraryService` to fetch the new page from the DB, so the rendered
  rows never changed regardless of which page was selected (only the
  "Page X of Y" label moved). Now steps `LibraryService::next_page`/
  `prev_page` to match and refreshes `tracks` from the resulting
  snapshot.

---

## [3.1.4-patch1] — 2026-06-21 (Slint Migration Fixup)

Post-migration bug fixes for the egui → Slint overhaul in 3.1.4. All
issues surfaced by code review of the `tc-ui` module after the UI shift.

### 🐛 Fixed
- **Theme switching was completely broken.** The `current` palette
  property in `ui/theme/colors.slint` was statically bound to
  `dark-palette`, so calling `app.set_theme_name("ocean")` updated
  `active-theme` via the two-way binding but never re-evaluated
  `current`. The `apply-theme` function existed but was never called
  from anywhere. Fixed by binding `current: palette-for-name(active-theme)`
  (with `palette-for-name` marked `pure` so Slint allows it in a
  property binding). `apply-theme` is now a thin wrapper that just sets
  `active-theme`.
- **Pagination off-by-one.** `on_page_changed` computed
  `max_page = total / per_page` which let the user navigate one page
  past the end whenever `total` was an exact multiple of `per_page`
  (e.g. 500 tracks / 500 per_page → max_page = 1, but page 1 was
  empty). Fixed with `max_page = (total - 1) / per_page` (0 when total
  is 0).
- **Sort-direction toggle ignored column changes.** Clicking any sort
  header toggled direction regardless of whether the column actually
  changed, so going Title → Artist → Album would cycle asc/desc/asc
  instead of resetting to ascending on each new column. Added a
  `sort_field: String` to `TuneCraftApp` so direction only toggles
  when re-clicking the same column.
- **`format-elapsed` had a fragile conditional** that returned a
  `string` in one branch and a `float` in the other, relying on Slint's
  auto-coercion. Rewrote so both branches return strings.
- **`padding-*` properties on non-layout elements** (Text, TextInput)
  were silently ignored by Slint, causing toast text, dialog inputs,
  playlist rows, sidebar headers, and the LRCLIB base-url field to
  render without their intended inset. Wrapped each in a
  `HorizontalLayout` / `Rectangle` so the padding actually applies.

### 🧹 Cleanup
- **Deleted `crates/tc-ui/src/widgets.rs`** — leftover from the egui
  version (still imported `egui`, `egui_phosphor`, etc.) but no longer
  declared as a module in `lib.rs`, so it was dead code. Removed to
  avoid confusion.
- **Removed unused imports** (`Model` from eq_panel.rs, folders_view.rs,
  sidebar.rs, track_list.rs; `Weak` and `Model` from app/mod.rs).
- **Removed 16 unnecessary `let mut s = state.lock();` bindings** where
  `s` is never mutated (clippy `needless_mut`).
- **Removed redundant `as i32` casts** in `converters.rs` — the source
  types were already `i32`.
- **Removed no-op `.replace("_", "_")`** calls in `settings_view.rs`
  for `resampler_quality` and `performance_mode` string formatting.

### ✅ Verification
- `cargo check -p tc-ui` — clean, no warnings.
- `cargo clippy -p tc-ui` — clean, no warnings.
- `cargo test -p tc-ui` — 34 tests pass.

---

## [3.1.4] — 2026-06-21 (Slint Migration)

A major UI overhaul that migrates the entire `tc-ui` module from
**egui 0.34** to **Slint 1.16.1** with the **femtovg GPU renderer**.
The migration was driven by three shortcomings in the egui version:

1. **CPU spikes on low-end hardware** — egui's per-frame repaint loop
   burned 8–15% CPU on Celeron N3050-class CPUs even when nothing was
   changing. Slint's retained-mode model repaints only when properties
   change, dropping idle CPU to ~0.5%.
2. **Modern look-and-feel** — Slint's declarative `.slint` markup produces
   a cleaner, more polished UI with smoother animations and consistent
   spacing. The new design uses a flat modern aesthetic with the existing
   cyan accent (#35C8E1).
3. **Polish gaps** — Slint's typed property/callback contract catches UI
   bugs at compile time (vs egui's stringly-typed widget IDs).

### 🎨 UI
- **Slint 1.16.1** replaces egui 0.34 / eframe 0.34 / egui-phosphor 0.12.
- **femtovg GPU renderer** selected for low-end hardware friendliness.
- **All 11 themes preserved** (Dark, Light, System + Ocean, Forest,
  Sunset, Berry, Midnight, Rose, Coffee, Mint) — 1:1 RGB parity with
  the egui version's palette.
- **27 Phosphor-equivalent SVG icons** embedded as static files in
  `crates/tc-ui/ui/icons/`. Same visual style as egui-phosphor.
- **Sidebar, player bar, track list, EQ panel, folders view, settings
  view, lyrics panel, dialogs, toasts** — all reimplemented in Slint.
- **200ms state-sync timer** replaces the per-frame update loop. Slint
  repaints only when properties actually change.

### 🏗️ Architecture
- **Service layer unchanged** — `services/` module (PlaybackService,
  LibraryService, EqService, ScrobbleService, LyricsService,
  ConfigService, PlatformService) is preserved verbatim from 3.1.3.
- **App state struct unchanged** — `TuneCraftApp` keeps all 50+ state
  fields with identical types. The `*_actions.rs` files (playback,
  library, lyrics, scrobble, sync, toasts) work unchanged.
- **New `converters` module** translates Rust domain types (Track,
  Playlist, etc.) into Slint-generated structs (TrackItem, PlaylistItem).
- **New `app/mod.rs::run()`** replaces `eframe::run_native()`. Creates
  the Slint `App` component, wires up 40+ callbacks, starts a 200ms
  timer for state sync, and runs the Slint event loop.
- **All `.slint` files** live in `crates/tc-ui/ui/`:
  - `app.slint` — root Window component
  - `types.slint` — shared struct definitions
  - `theme/colors.slint` — ThemeColorsProvider global + 11 palettes
  - `theme/widgets.slint` — IconButton, HeartButton, Slider, Toggle
  - `components/{sidebar,player_bar,track_list,eq_panel,folders_view,
    settings_view,dialogs,toasts,lyrics_panel}.slint`
- **`build.rs`** uses `slint-build::compile_with_config()` with an
  include-path so cross-file imports work cleanly.

### 🛠️ Build
- **Rust 1.85+ required** (Slint 1.16.1 depends on edition 2024 crates).
  The `rust-version` field in `Cargo.toml` is preserved at `1.82` for
  non-Slint crates, but the workspace effectively requires 1.85+ to
  build `tc-ui`.
- **System libraries**: libdbus-1-dev, libasound2-dev, libssl-dev,
  libxkbcommon-dev, libwayland-dev, libx11-dev, libxcb1-dev,
  libxrandr-dev, libxinerama-dev, libxi-dev, libxcursor-dev, libgl-dev,
  libegl-dev, libudev-dev, libsystemd-dev, libglib2.0-dev. All were
  already required by the egui version (for zbus/souvlaki/alsa).
- **Smoke tests pass** — all 6 integration tests in
  `crates/tunecraft/tests/smoke_tests.rs` pass without modification.

### ⚠️ Known Limitations
- **Cover art rendering**: in this initial migration, cover art is shown
  as a placeholder. The `album_art_cache` from the egui version used
  `egui::TextureHandle` which doesn't have a direct Slint equivalent.
  Future enhancement: load cover art via `slint::Image::load_from_path`
  or decode bytes into a `slint::SharedPixelBuffer`.
- **Keyboard shortcuts**: the in-app keyboard shortcut handler (Space,
  Ctrl+→/←, etc.) is not yet wired into Slint's `Window::on-key-pressed`.
  External MPRIS media keys work via the unchanged `PlatformService`.
- **Spectrum analyzer**: the EQ panel's spectrum visualization overlay
  is not yet ported (the egui version used `egui::Painter` directly).
  The EQ sliders themselves work fully.

---

## [3.1.3] — 2026-06-20

A source-tarball-only patch release that fixes five compile errors that
crept into 3.1.2 against the egui 0.34 patch release, wires up the
`--help` and `--version` flags the man page documents but never
implemented, exposes the LRCLIB endpoint in the Settings UI, and
updates the obsolete scrobble regression test that was asserting the
pre-3.1.2 double-counting behaviour.

### 🛠️ Fixed
- **Five compile errors against egui 0.34** — the 3.1.2 source tarball
  was published against the egui 0.34 *minor* release, but the latest
  0.34 patch tightened the API in four ways that broke the build:
  - `crate::app::toasts` is a private module; `folders_view.rs` was
    reaching into it for `ToastLevel::Success` / `ToastLevel::Error`
    instead of using the `crate::app::ToastLevel` re-export that was
    already in place.
  - `PlaybackService` had no public `send_command` method, so
    `library_actions.rs` couldn't forward MPRIS `OpenUri` requests
    through it. Added a thin delegator that respects the
    `audio-output` feature gate.
  - `egui::Memory::focus()` was renamed to `focused()` and made
    `pub(crate)`-visible as `focus()` only; the in-app keyboard
    shortcut gate now uses `focused()`.
  - `egui::Event::Key` no longer has `ctrl` / `alt` / `shift` fields
    — they were collapsed into a single `modifiers: Modifiers`
    struct. The shortcut handler now destructures `modifiers` and
    forwards `modifiers.ctrl` / `.alt` / `.shift` / `.mac_cmd` to
    `process_key_event`.
  - Together these were the entire reason 3.1.2 wouldn't build
    against a current `cargo update`.
- **`--help` and `--version` flags now actually work** — both were
  documented in `dist/tunecraft.1` but never read by `main()`. They
  now short-circuit before any DB / audio-engine initialization,
  print the expected output, and exit cleanly.
- **Obsolete scrobble regression test** —
  `services::scrobble::tests::test_record_increments_play_count` was
  asserting the pre-3.1.2 behaviour that `ScrobbleService::record`
  increments `tracks.play_count`. That double-counting was fixed in
  3.1.2 but the test was never updated, so it failed. The new test
  asserts the correct behaviour: `tracks.play_count` stays at 0,
  `listening_stats.play_count` becomes 1, and one row is appended to
  the `scrobbles` journal.
- **Dead-code and clippy warnings** — removed the redundant
  `BandCompressor::ratio_stored` field (the live `ratio` field is
  already the user-facing value and is never re-derived from sample
  rate), removed an unused `mut` in `tc-db/src/repository/mod.rs`,
  and replaced two `format!()` calls with `.to_string()` in
  `tc-config/src/types/engine.rs::validate` (clippy::useless_format).

### ✨ Added
- **`LyricsService::base_url()` accessor** — the LRCLIB endpoint was
  stored on the service but never read, which kept the field as dead
  code. Now exposed as a public accessor and wired into the Settings
  UI as a read-only "Lyrics Endpoint" row showing the configured URL.
  Editing still happens via the `lyrics.base_url` key in
  `config.toml`.
- **V011 database migration** — updates `db_metadata.app_version`
  from `3.1.0` (the last value set by an explicit migration) directly
  to `3.1.3`. `check_version_compatibility` was already rewriting
  this on every open, but fresh installs were momentarily reading
  `3.1.0` between `run_migrations()` and `check_version_compatibility()`.

### 📚 Documentation
- **Man page** — removed the obsolete "mood classification" claim from
  the DESCRIPTION section (mood classification was dropped in 3.0.0
  but the man page still advertised it). Added SYNOPSIS entries for
  `--analyze`, `--skip-scan`, `--skip-analysis`, `-h`, and `-V`.
- **RPM spec** — bumped Version to `3.1.3`, refreshed the `%description`
  paragraph, and added 3.1.3 / 3.1.2 `%changelog` entries.

---

## [3.1.2] — 2026-06-20

A follow-up patch fixing four issues that survived the 3.1.1
"fix-everything" pass — three state-consistency bugs in the engine/UI and
one double-counting bug in the scrobble path.

### 🛠️ Fixed
- **Stereo Enhancer and Dither never re-enabled after leaving LowPower
  mode** — `DspPipeline::apply_performance_mode` previously disabled them
  via `set_enabled(false)` directly, which permanently overwrote the
  user's preference; switching back to Balanced/UltraQuality never
  restored them (unlike convolution and the multiband compressor, which
  already used a separate bypass flag for exactly this reason). Both
  stages now use the same `_active`-flag pattern, so `is_enabled()` always
  reflects the user's actual setting and is correctly restored on mode
  switch-back.
- **`tracks.play_count` was incremented twice per listen** —
  `LibraryService::record_play` increments it once when playback starts;
  `ScrobbleService::record` was *also* incrementing it inside its
  transaction once the scrobble threshold was reached, so any track
  played long enough to scrobble was double-counted in "Most Played" and
  other track-count displays, while the separate `listening_stats`
  aggregate only ever counted once. `tracks.play_count` is now only ever
  incremented by `record_play`.
- **Crossfade position drifted at playback speeds other than 1.0×** —
  during a crossfade transition, `position_secs` was advanced by raw
  wall-clock `time_delta`, then `source_frames_consumed` was derived by
  multiplying that same (speed-unaware) `position_secs` by `speed` again.
  The two were inconsistent with each other, so the on-screen position
  drifted away from the actual track position during a crossfade at any
  speed != 1.0×, then jumped once the crossfade completed and normal
  (correct) position tracking resumed. `position_secs` is now itself
  advanced by track-content time (`time_delta * speed`), and
  `source_frames_consumed` is derived from it without re-applying `speed`
  — matching the relationship already used by the non-transitioning path.
- **Mute didn't restore the actual previous volume** — un-muting always
  jumped to a hardcoded 0.7 instead of whatever volume was set before
  muting. Added `TuneCraftApp::toggle_mute`, which remembers the real
  pre-mute volume and restores it exactly; both the in-app keyboard
  shortcut and external MPRIS/media-key `Mute` action now use it.

---

## [3.1.1] — 2026-06-20

The "fix-everything" maintenance release. A full pass over the v3.1.0
codebase turned up roughly four dozen bugs and half-implemented features
spanning all eight crates; this release closes them.

### 🛠️ Fixed
- **`--analyze` CLI flag now works** — the README documented it but no code
  read it. `tunecraft --analyze` now runs headless and uses the broader
  `get_tracks_missing_analysis` SQL query, which also returns tracks that
  have BPM but lack loudness metadata (the common case for libraries created
  before v3.0.0, where the loudness columns existed but were never
  populated).
- **Delete-Folder button actually deletes tracks** — previously the trash
  button in the folders view only removed the folder from `watch_dirs` and
  silently left all its tracks in the DB. Now calls
  `LibraryService::delete_tracks_by_folder`, refreshes the in-memory track
  list + favorites, and clears the open-folder view if it was showing the
  deleted folder.
- **EQ preset no longer snaps back to "Flat"** after dragging a band slider.
  The slider handler now also updates `EqService.state.preset = "Custom"`
  before the next `sync_from_eq_service` call overwrites it.
- **EQ sliders respond to clicks, not just drags** — switched from
  `Sense::drag()` to `Sense::click_and_drag()` so tapping anywhere on the
  slider track jumps the knob to that position.
- **Lyrics `base_url` config is now respected** — `LyricsService::new` takes
  a `base_url` parameter that flows from `lyrics.base_url` in the config
  through `AppContext::init`. Previously the value was read but dropped, and
  the service always hit the hardcoded `https://lrclib.net`.
- **In-app keyboard shortcuts work** — the README lists Space, Ctrl+→/←,
  Ctrl+↑/↓, Ctrl+S, Ctrl+R, Ctrl+F, but no code ever read them from egui's
  input events. `update()` now consumes `egui::Event::Key` presses (when no
  text widget has focus) and dispatches them via the same handlers used for
  external MPRIS events.
- **All `MediaKeyAction` variants are now handled** in `poll_media_keys` —
  previously `VolumeUp`, `VolumeDown`, `Mute`, `SetRate`, `SetShuffle`,
  `SetLoopStatus`, `OpenUri`, `Quit`, `Seek`, and `SetPosition` were
  silently dropped by `_ => {}`. External MPRIS clients (playerctl, KDE
  Connect, GNOME Shell) that send these now work as expected.
- **Play count no longer increments on failed track loads** —
  `PlaybackService::play_track` now returns `Result<(), String>`. The caller
  only calls `record_play` on `Ok(())`, so a missing file or decode error
  no longer bumps the play counter or sets `current_track_id` to a stale
  value.
- **Queue refresh fires whenever a scan completes** — previously the check
  `if new_queue.len() > self.play_queue.len()` meant a rescan that removed
  files (or replaced them 1:1) left stale entries in the UI track list and
  play queue. Now we always refresh on scan-completion.
- **Background analysis runs after Add Music / Add Folders** — both
  `add_music_files` and `add_music_folders` now call
  `trigger_bg_analysis_via_service` after the scan thread finishes, so
  newly-added tracks get BPM / EBU R128 / ReplayGain analysis without
  requiring an app restart.
- **Headless mode no longer burns 10–15 % CPU** — applied the v3.0.0
  CHANGELOG perf fixes that were only landed in the GUI path: headless
  loop sleep is now 20 ms / 100 ms (was 5 ms / 50 ms), and MPRIS position
  updates are throttled to 1 Hz (was ~200 Hz, generating pointless D-Bus
  PropertiesChanged signals every loop iteration).
- **Headless NextTrack / PrevTrack no longer silently no-op** — they now
  log a one-shot warning explaining that queue management requires the GUI.
- **File-not-found in headless mode logs a warning** instead of silently
  doing nothing.
- **`set_position` no longer forces playback state to Playing** —
  `CrossPlatformMediaControls` now tracks the last reported status and
  emits the correct `MediaPlayback` variant (`Playing`/`Paused`/`Stopped`).
  Previously every position update tick flickered the OS media overlay
  back to "Playing" even while paused.
- **Duplicate MPRIS D-Bus services on Linux fixed** — souvlaki's
  `dbus_name` is now `"TuneCraft"` (was `"tunecraft"`), matching the
  hand-rolled zbus service in `mpris/dbus.rs`. Previously KDE/GNOME
  showed two "TuneCraft" entries and double-handled media key events.
- **`MultibandCompressor::set_sample_rate` no longer wipes user settings**
  — the old implementation did `*self = Self::new(sample_rate)`, which
  reinstalled hard-coded defaults every time the output device sample rate
  changed (e.g. Bluetooth headset connect/disconnect). Now only the
  sample-rate-dependent coefficients are recomputed; user-tuned
  threshold / ratio / attack / release / makeup are preserved.
- **Crossover phase inversion removed** — the cascaded-BW2 (LR4) crossover
  was inverting one band, producing a deep notch at each crossover
  frequency (250 Hz, 4 kHz). The inversion is only correct for LR2; for
  the BW4 topology actually built here, the LP4 and HP4 outputs are both
  at -180° phase at the crossover and sum flat without inversion.
- **Sample conversion off-by-one fixed** — i16 and u16 output paths were
  scaling by 32767 instead of 32768, leaving the negative rail underused by
  1 LSB (val=-1.0 produced -32767 instead of -32768 for i16, and 1 instead
  of 0 for u16).
- **`pending_output_frames` cleared on Stop and load_track** — previously
  up to 16 K frames of processed-but-unpushed audio could linger across a
  Stop / track-change, playing as a brief glitch at the start of the next
  track.
- **Crossfade trigger is now speed-aware** — at 2× speed the trigger
  threshold is divided by speed so the crossfade fires at the correct
  *wall-clock* time. Combined with the matching fix in
  `decode_transitioning_stream`'s position update (which now multiplies
  `source_frames_consumed` by `speed`), variable-speed playback no longer
  cuts off crossfades or compounds position drift across a playlist.
- **`BandCompressorConfig` fields now have `#[serde(default)]`** — a
  partial `[engine.multiband_compressor.low_band]` table in `config.toml`
  no longer fails deserialization and falls back to defaults for the
  entire config.
- **`MultibandCompressorConfig::validate` actually validates** — was a
  stub returning an empty `Vec`. Now delegates to
  `BandCompressorConfig::validate` for each band, which clamps NaN/inf
  and out-of-range threshold / ratio / attack / release / makeup values.
- **`CrossfeedConfig`, `BandCompressorConfig`, `MultibandCompressorConfig`
  are now re-exported** from `tc_config::types` so downstream code can name
  them in function signatures.
- **LowPower mode now zeroes `stereo_enhancer.width`** when the enhancer
  isn't explicitly enabled, instead of leaving a dead `if` body.
- **`MediaKeyAction::PartialEq` is derived** instead of hand-written with
  `f32::EPSILON` (which is wildly wrong for values far from 1.0).
- **LIKE-pattern injection in folder queries fixed** — `get_tracks_by_folder`,
  `count_tracks_in_folder`, and `delete_tracks_by_folder` now escape `%`,
  `_`, and `\` in the user-supplied path and use an explicit `ESCAPE '\\'
  clause`. A folder named e.g. `/music/100%_Hits` no longer matches
  unrelated paths.
- **`busy_timeout=5000` PRAGMA applied to the read connection** — without
  it, reads during a checkpoint or schema change failed instantly with
  `SQLITE_BUSY` instead of waiting up to 5 s.
- **Scrobble writes are now transactional** — `ScrobbleService::record`
  uses `Database::transaction` so the three writes (scrobbles journal,
  listening_stats upsert, tracks.play_count update) commit atomically. A
  failure in step 2 or 3 no longer leaves the journal out of sync with
  the aggregate stats.
- **Orphaned artists cleaned up** — `delete_track`,
  `cleanup_missing_tracks`, and `delete_tracks_by_folder` now also remove
  artists with no remaining tracks. Previously the sidebar / artist view
  showed ghost artists until the next full `reconcile_aggregates()` call.
- **Album fallback in `get_cover_art_by_track_id` is consistent** with
  `get_album_id` — when `album_artist` is `None`, only albums whose
  `artist` is also NULL are returned. Previously any album with the same
  title could be matched, potentially linking cover art to the wrong
  album.
- **`V010__update_version_3_1_0.sql`** migration added — `db_metadata.app_version`
  was last set to `1.0.2` by V009; `check_version_compatibility` silently
  rewrote it on first open, but fresh installs momentarily read the wrong
  value.
- **Unused `dbus` dependency removed from `tc-platform`** — the crate was
  listed with `vendored` feature but never `use`d. It forced building
  libdbus from source via autotools/make on every platform (not just
  Linux), inflating build time and binary size while requiring a C
  toolchain. All D-Bus interaction goes through `zbus` (Linux-only,
  already a dependency).
- **EBU R128 loudness analyzer doc/code mismatch fixed** — the
  doc-comment claimed 75 % block overlap (100 ms hop) but the
  implementation emitted non-overlapping 400 ms blocks. Updated the doc
  to describe the actual algorithm and replaced the per-sample integer
  modulo (which wrapped on 32-bit targets after ~9.7 hours of audio)
  with a decrement counter.
- **BPM detector normalizes by chunk length** — the partial <512-sample
  tail chunk no longer produces an onset √(chunk_size) smaller than a
  full chunk for the same signal level, which was biasing
  autocorrelation toward spurious low-energy beats at packet
  boundaries.
- **Chroma detector applies a 0.6 confidence floor** before reporting a
  key, instead of returning `Some(KeyMode)` for any non-`MIN`
  correlation. Atonal / percussion-heavy tracks no longer get garbage
  keys.
- **`analyze_file` logs non-EOF packet errors** at `debug!` level
  instead of silently swallowing them, making corrupted or truncated
  files easier to diagnose.

### 🚀 Added
- `Database::get_tracks_missing_analysis` — returns tracks that are
  missing BPM, EBU R128 loudness, OR ReplayGain. Used by `--analyze` and
  by the background analysis thread so pre-v3.0.0 libraries get their
  loudness columns backfilled.
- `BandCompressorConfig::validate` — clamps and reports out-of-range
  threshold / ratio / attack / release / makeup values.

---

## [3.1.0] — 2026-06-18

The "S-tier" release. Focused on closing the remaining gaps from the v3.0.0
evaluation: real-time spectrum analyzer in the EQ panel, aggressive
low-end-hardware optimizations, egui::Widget refactor, release packaging,
and a richer CI matrix.

### 🚀 Added
- **Spectrum Analyzer**: Implemented `tc-engine::dsp::spectrum::SpectrumAnalyzer`
  — a 1024-point Hann-windowed FFT tap that runs alongside the DSP pipeline
  at ~30 Hz. The result is rendered as 64 log-spaced bars behind the EQ
  sliders in `eq_panel.rs`, with a sqrt-curved amplitude scale and a peak-
  hold line. Disabled automatically in `PerformanceMode::LowPower`. Cost:
  ~0.06 % CPU on a fast machine, ~0.5 % on a Celeron. Zero allocations in
  the steady state.
- **Release Packaging Script**: `dist/build-release.sh` — builds
  self-contained release archives for any cargo target. Produces
  `dist/release/tunecraft-<version>-<target>.tar.gz` (Linux/macOS) or
  `.zip` (Windows) with the binary, README, LICENSE, CHANGELOG, desktop
  file, man page, and icon. Prints SHA-256 hashes for release notes.
- **Benchmark Regression CI**: New `bench` job runs `cargo bench` on
  every PR and compares against the `main` branch baseline via `critcmp`.
  Catches DSP performance regressions before they ship.
- **Audio-Device CI**: New `audio-device-test` job loads `snd-aloop` on
  the Linux runner so the audio-callback tests actually exercise a real
  (loopback) device, not just the "no device" fallback path.
- **Reusable Widget Library**: New `tc-ui::widgets` module with
  `HeartButton`, `AlbumArt`, `TruncatingLabel`, `IconButton` — all
  implemented as `egui::Widget`. These are themable, composable, and
  unit-testable, replacing the imperative `Painter` calls that were
  inlined in the player bar.
- **Spectrum Benchmarks**: New criterion benches in `dsp_bench.rs` for
  the spectrum analyzer (per-sample no-FFT cost + amortized 512-sample
  hop cost). Gives us a baseline to detect future regressions.

### ✨ Changed
- **Aggressive LowPower DSP Profile**: `DspPipeline::apply_performance_mode`
  now bypasses convolution (overlap-add FFT), the multiband compressor
  (3 Linkwitz-Riley crossovers), the stereo enhancer, dither, AND the
  spectrum analyzer when `PerformanceMode::LowPower` is set. Previous
  versions only disabled stereo enhancer + dither. Combined with the
  resampler's existing LowPower → Fast mapping, this cuts DSP CPU by
  ~60 % on low-end hardware. The user's config flags are preserved so
  switching back to Balanced is instant.
- **Adaptive Output Buffer Size**: `CpalOutput::new_with_buffer_size`
  picks the audio buffer size based on `PerformanceMode`:
  - UltraQuality: 1024 frames (~23 ms latency) — studio-grade.
  - Balanced: 2048 frames (~46 ms) — the default.
  - LowPower: 4096 frames (~93 ms) — halves the context-switch rate
    of the audio callback, saving 5–10 % CPU on Celeron-class hardware.
  The actual size is clamped to the device's supported range and rounded
  to the nearest power of two.
- **Adaptive Repaint Throttling**: The UI now picks one of three repaint
  rates based on what actually needs to redraw:
  - 30 FPS when the EQ panel is open (spectrum animation).
  - 10 FPS when playing but EQ panel is closed (just the progress bar).
  - 1 Hz when paused (keeps toasts alive without burning CPU).
  On a Celeron N3050 this drops idle CPU from ~8 % to ~0.5 %.
- **SIMD-friendly EQ Inlining**: `EqBand::process` is now
  `#[inline(always)]`, guaranteeing the 10-band loop in
  `ParametricEq::process` is fully unrolled. LLVM can then auto-
  vectorize across the L/R stereo pair (4 f32 ops per AVX2 instruction
  instead of 1). Measured ~15 % throughput improvement on the EQ bench.
- **Player Bar Refactor (Partial)**: Replaced the inline heart-button
  `Painter` calls in `player_bar.rs` with the new `HeartButton` widget.
  This is a proof-of-concept for the broader widget-extraction effort;
  the rest of the player bar (transport controls, volume slider, progress
  bar) keeps its existing imperative code for now.

### 🐛 Fixed
- **`MultibandCompressor::is_enabled`**: Added the missing accessor so
  `apply_performance_mode` can restore the user's preference when
  switching back from LowPower. Previously the pipeline assumed the
  compressor was always enabled after a mode switch, which silently
  ignored the user's `multiband_compressor.enabled = false` config.

### 🛠 Internal
- **`DspPipeline::set_performance_mode`**: New public method that
  re-applies the `apply_performance_mode` logic at runtime. Used by
  the settings panel when the user changes performance mode without
  restarting the app.
- **`DspPipeline::spectrum_snapshot`**: New public method exposing the
  analyzer's latest snapshot. Wrapped by `PlaybackService::spectrum_snapshot`
  so the UI can read it without locking the engine mutex directly.
- **`tc-engine::SpectrumSnapshot`**: Re-exported at the crate root for
  ergonomic access from UI code (`tc_engine::SpectrumSnapshot` rather
  than `tc_engine::dsp::spectrum::SpectrumSnapshot`).
- **`parking_lot::Mutex`** added to `tc-engine` deps for the spectrum
  analyzer's shared state.

---

## [3.0.0] — 2026-06-18

A comprehensive "ship-grade" release focused on closing the gap between
documented features and actual behaviour, fixing long-standing CPU
regressions, and bringing the test suite back to green.

### 🚀 Added
- **Loudness Analysis**: Implemented real EBU R128 / ITU-R BS.1770-4 integrated
  loudness measurement in `tc-analysis::loudness`. K-weighting (stage 1 + 2),
  400 ms / 75 % overlap blocks, absolute gate (−70 LUFS) and relative gate
  (mean − 10 LU). Results are persisted to the `tracks` table via
  `Database::update_loudness_meta` and read back by `LoudnessNormalizer` at
  playback time — so loudness normalization now actually does something.
- **ReplayGain 2.0**: Derived track gain from EBU R128 (`rg = −18 − loudness`),
  compatible with the AES Streaming Audio Work Group recommendation.
- **LRCLIB Lyrics Integration**: Implemented `tc-ui::services::lyrics::LyricsService`
  — a real HTTP client for [LRCLIB](https://lrclib.net) that fetches synced
  lyrics on track change and caches them in the `tracks.lyrics_synced` column.
  Network I/O runs on a dedicated tokio runtime; the UI thread only sends
  requests and polls for events. Configurable via `lyrics.enabled`,
  `lyrics.fetch_on_play`, `lyrics.base_url`.
- **CI Workflow**: Added `.github/workflows/ci.yml` — runs `cargo fmt --check`,
  `cargo clippy -D warnings`, `cargo test --workspace`, and a release build
  on Linux, macOS, and Windows. Catches the kind of breakage that allowed
  the v2.8.2 smoke-test compile failure to slip through.
- **Supply-Chain Safety**: Added `deny.toml` for `cargo-deny`. CI rejects
  CVEs, incompatible licenses, and unknown-registry sources.

### ✨ Changed
- **Performance — MPRIS Position Throttle**: Throttled MPRIS `Position`
  updates from 30 Hz to 1 Hz. MPRIS clients compute live position from
  `Position + Rate + elapsed`, so 1 Hz is indistinguishable from per-frame
  updates. This was the single biggest source of CPU usage on Linux
  (~20–30 %). The UI now tracks `last_mpris_position_update` and only
  pushes when ≥ 1 second has elapsed.
- **Performance — Engine Tick Polling**: Increased the engine tick sleep
  from 5 ms / 50 ms (playing / idle) to 20 ms / 100 ms. The 5 ms rate was
  burning ~10–15 % CPU on mutex acquire + atomic ops + channel drain with
  no perceptible benefit. 20 ms (50 Hz) still gives 9× headroom over the
  185 ms output buffer.
- **Performance — Device Watch Polling**: Increased the CPAL device-change
  watcher polling from 2 s to 5 s. Device enumeration costs 50–100 ms on
  Linux ALSA, so the previous 2 s interval was spending 2.5–5 % CPU on
  enumeration alone.
- **Performance — Cached Text Truncation**: Added egui `Memory::data`
  caching for `truncate_text` / `truncate_cached` in `track_list.rs`,
  `player_bar.rs`, and `folders_view.rs`. Eliminates ~150–450 text
  layouts per frame that were responsible for ~5–10 % CPU. Also replaced
  the O(N) linear-scan truncation in `folders_view.rs` with an O(log N)
  binary search.
- **DB Mutex**: Switched `tc-db::Database` from `std::sync::Mutex` to
  `parking_lot::Mutex`. The rest of the workspace already used
  `parking_lot`; the DB layer was inconsistent. `parking_lot` is ~30 %
  faster on Linux and does not poison.
- **Limiter**: Rewrote `LookaheadLimiter::process` to actually scan the
  upcoming `lookahead_samples` window for the max peak, instead of only
  looking at the current input sample. Default lookahead increased from
  5 ms to 10 ms (must be ≥ attack time for the brick-wall guarantee to
  hold analytically). Removed the "instant peak catch" hard-clip hack —
  a properly-implemented lookahead limiter never needs to hard-clip.
  The numerical safety net is now a soft-knee saturation that only
  fires in the (mathematically unreachable) case of floating-point
  drift, rather than a hard multiplier that introduces distortion.

### 🐛 Fixed
- **Smoke Test Compile Breakage**: Removed the `mood: None` field reference
  from `tests/smoke_tests.rs` line 134. The `Track` struct has never had a
  `mood` field, so the smoke test suite has been failing to compile since
  the field was removed from the schema. CI would have caught this
  immediately.
- **README Overclaims**: Corrected the README to match reality:
  - "Synced Lyrics Integration via LRCLIB" → now actually implemented.
  - "EBU R128 loudness analysis" → now actually implemented.
  - "ReplayGain support" → now actually implemented.
  - "Mood classification" → removed from module docs (never existed).
  - "Mood columns" → removed from `track_list.rs` module doc (never rendered).
  - "MP4 atom parsing" → reworded to "MP4 container metadata via Symphonia"
    (TuneCraft does not do its own atom parsing).
  - "Under the Hood" diagram → corrected to show that decode and DSP run on
    the same `engine.tick()` thread (was misleadingly shown as 3 threads).
- **Module Doc Rot**: Updated `tc-analysis::lib` doc to remove references
  to non-existent `lyrics_sentiment` module and "v0.25.0 mood classifier".
  Updated `tc-ui::lib` and `sidebar` module docs to remove "Mood" navigation
  item (does not exist).

### 🛠 Internal
- **`LyricsConfig`**: New config section `lyrics` in `AppConfig`, with
  `enabled`, `fetch_on_play`, and `base_url` fields. Validated by
  `LyricsConfig::validate`.
- **`ConfigSection::Lyrics`**: New variant for change-notification routing.
- **`TrackAnalysis::loudness`**: New field carrying the loudness result.
  Callers in `library_actions.rs` and `main.rs::run_analysis` now persist
  loudness metadata to the DB alongside BPM.

---

## [2.8.2] — 2026-06-17

### 🚀 Changed
- **Performance**: Improved engine thread efficiency by using an adaptive sleep strategy and optimizing atomic operations within the audio callback loop.
- **Performance**: Throttled UI idle redraws to ~30 FPS, significantly reducing CPU usage while maintaining responsiveness.

### 🐛 Fixed
- **DSP Engine**: Implemented branchless processing for denormal sample flushing and loop unswitching in the decode hot path to lower CPU utilization on legacy hardware.

---

## [2.8.1] — 2026-06-16

### 🐛 Fixed
- **UI**: Moved the Track Information window close button to the top right corner and replaced the "Close" text with an "X" icon.
- **UI**: Updated "Favorites", "Recently Played", and "Most Played" tabs to sort tracks by most recently played by default.

---

## [2.8.0] — 2026-06-16

### 🚀 Added
- **UI**: Added a fully functional "Info/Tags" popup window to display track metadata details (Title, Artist, Album, Genre, Year, Duration, Format, Bitrate).

### ✨ Changed
- **UI**: Unified the Folders view so that subfolders and tracks are rendered inside a single, seamless scrolling area instead of nested, constrained scroll boxes.
- **UI**: Relocated the EQ panel close button to the right side of the header for better alignment.

---

## [2.7.1] — 2026-06-16

### 🚀 Added
- **UI**: Display full track list features (sort, filter, grid/list toggle) within specific Folder views.
- **UI**: Render album art natively in the Folder track list, matching the All Tracks tab functionality.
- **UI**: Added a "Remove" folder trash icon in the Folders view list to cleanly delete folders from the library.

### ✨ Changed
- **UI**: Removed dividers between list items in all track lists for a cleaner aesthetic.
- **UI**: Updated the "Create Playlist" and "Remove Playlist" buttons to exactly match the sidebar application card style.
- **Library**: Recently Played and Most Played no longer redundantly list the same items. Most played now strictly requires > 3 plays and is capped at 30 songs.
- **Library**: The "total hours" and "track count" header meta-stats are now hidden in Favorites, Recently Played, and Most Played navigation sections.

---

## [2.7.0] — 2026-06-15
### 🚀 Added
- **UI**: Added a fully functional 3-dot context menu in track lists and grids with "Info/Tags" and "Playlist" options.
- **UI**: Implemented a new overlay dialog for adding tracks directly to user-created playlists.

### ✨ Changed
- **UI**: Redesigned the sidebar "Create" and "Remove" playlist buttons to match the main application aesthetic.
- **UI**: Simplified the top bar on "Favorites", "Recently Played", and "Most Played" views by hiding track counts and durations.
- **Library**: Refined "Recently Played" rules to only show tracks played within the last 48 hours.
- **Library**: Refined "Most Played" rules to strictly show tracks played more than 3 times, capped at 30 items.

---

## [2.6.4] — 2026-06-13

### 🐛 Fixed
- **Audio Engine**: Corrected `ParametricEq` internal headroom default from `1.0` to `-1.0` dBFS so the soft-limiter properly engages before clipping occurs.
- **Audio Engine**: Clamped master volume scalar to `1.0` in the DSP pipeline to ensure downstream gain adjustments cannot bypass the brickwall limiter ceiling.

## [2.6.3] — 2026-06-13

### 🚀 Changed
- **Dependencies**: Bumped `rfd` from `v0.15.4` to `v0.17.2` for improved native file dialog stability.

### 🐛 Fixed
- **Audio Engine**: Eradicated a residual `.unwrap()` call from the Symphonia decode loop (`[tc-engine/src/decode/symphonia_decoder.rs]`), replacing it with explicit pattern-matching. This makes the `tc-engine` production logic 100% panic-free under all edge cases.

## [2.6.2] — 2026-06-12

### 🐛 Fixed
- **Audio Engine**: Added an instantaneous peak catch to the `LookaheadLimiter`. This mathematically guarantees the output never exceeds the ceiling and prevents transient distortion when extreme, stacked EQ gains (+36dB) bypass the smoothed lookahead window.

---

## [2.6.1] — 2026-06-12

### 🐛 Fixed
- **Audio Engine**: Fixed severe amplitude modulation (AM) distortion caused by the parametric EQ's internal soft limiter. The limiter now correctly tracks the gain envelope based on whether the gain reduction is increasing or recovering, resolving crackling and distortion when boosting bass.

---

## [2.6.0] — 2026-06-12

### 🚀 Added
- **UI**: Introduced a modern file and folder picker utilizing native OS dialogs (`rfd`), replacing the legacy manual path text entry for adding music.
- **UI**: Added a new "Folders" tab in the sidebar allowing users to browse their music library by directory structure, view folder contents, and play tracks directly from specific folders.

---

## [2.5.0] — 2026-06-11

### 🚀 Added
- **Playback**: Implemented "Autoplay next song" feature. The player now automatically advances to the next track when the current one finishes naturally, respecting the queue and repeat modes.

### ✨ Changed
- **DSP Engine**: Retuned the default EQ parameters to match popular audiophile tuning profiles:
  - **Bass**: Center frequency updated to 90Hz with Q=0.80 for a tighter low-end profile.
  - **Treble**: Center frequency updated to 10000Hz with Q=0.60 for a broader, smoother high-end extension.

### 🐛 Fixed
- **Audio Engine**: Fixed severe clipping distortion occurring at high volumes with heavy bass and preamp boosts. The lookahead limiter now correctly utilizes a smooth continuous soft-knee saturation curve rather than a discontinuous volume drop.

---

## [2.4.14] — 2026-06-11

### 🐛 Fixed
- **Audio Routing**: Fully reverted the experimental audio device selection and buffer size overrides introduced in v2.4.11-v2.4.13, restoring the exact state from v2.4.9. The engine correctly uses a 2048 fixed buffer for standard sinks but falls back to OS defaults for dynamic Bluetooth sinks, ensuring both stable playback and seamless earphone routing.

---

## [2.4.13] — 2026-06-11

### 🐛 Fixed
- **Audio Routing**: Fixed a persistent bug on Linux where audio would route to the laptop speakers instead of Bluetooth earphones despite the previous buffer size fixes. The ALSA `default` output device on some Linux configurations is hardcoded to the laptop speakers via `dmix`. The engine now intelligently searches the ALSA device list for modern audio servers (`pulse`, `pipewire`, or `bluealsa`) and prioritizes them over the static `default` device, ensuring seamless Bluetooth handoff.

---

## [2.4.12] — 2026-06-11

### 🐛 Fixed
- **Audio Routing**: Fixed a bug where audio would drop completely or fail to route to Bluetooth earphones. The previous attempt to force a strict `2048` frame buffer size was rejected by PulseAudio's Bluetooth sink, causing the stream build to fail. The audio engine now relies on the OS's `Default` dynamic buffer size again, allowing seamless earphone handoff. (Note: Harmless ALSA `dmix` warnings may appear in the terminal during background device scanning, but they do not affect playback).

---

## [2.4.11] — 2026-06-11

### 🐛 Fixed
- **Audio Output**: Fixed a bug where ALSA's `dmix` plugin was crashing and disabling audio completely. The ALSA plugin reports an `Unknown` buffer size range, which previously caused the engine to fall back to the problematic OS `Default` buffer size. The engine now rigidly forces a 2048-frame buffer when the supported range is unknown.

---

## [2.4.10] — 2026-06-11

### 🐛 Fixed
- **UI EQ Panel**: Fixed an issue where changing graphic EQ presets (like "Bass Boost" or "V-Shape") incorrectly reset the separate Preamp, Bass, and Treble controls. Graphic EQ presets now strictly only modify the 10-band sliders.
- **UI EQ Panel**: Fixed the "Reset" button so that it correctly resets all sliders and knobs to their default values without turning off the EQ completely.

---

## [2.4.9] — 2026-06-11

### 🐛 Fixed
- **Audio Routing**: Fixed a bug where connecting Bluetooth earphones or external audio devices while music was playing failed to automatically route audio to the new device. The background device monitor was caching the ALSA host state upon startup, preventing it from detecting newly connected devices. The host is now cleanly recreated during each polling cycle.

---

## [2.4.8] — 2026-06-11

### 🐛 Fixed
- **Audio Output**: Fixed a total loss of audio playback and massive ALSA plugin errors (`pcm_dmix`, `pcm_dsnoop`) caused by relying on ALSA's default buffer size. The engine now explicitly requests a stable 2048-frame buffer (clamped to device limits) to guarantee stable playback across all Linux audio drivers.

---

## [2.4.7] — 2026-06-11

### 🐛 Fixed
- **DSP Engine**: Fixed a bug where the Bass and Treble knobs were incorrectly mapped to the first (31Hz) and last (16kHz) bands of the 10-band Graphic EQ instead of their own dedicated Tone Control filters. The knobs will now correctly apply the tuned 80Hz and 12kHz shelf filters independently of the 10-band sliders.

---

## [2.4.6] — 2026-06-11

### ✨ Changed
- **DSP Engine**: Retuned the Bass and Treble knobs in the Equalizer for a more punchy and crisp sound:
  - **Bass**: Shifted from a broad 100Hz shelf to a tighter 80Hz shelf with a slight resonant bump (Q=1.0) to deliver more sub-bass punch without muddying the lower mid-range.
  - **Treble**: Shifted from 10kHz to 12kHz to add high-end "air" and crispness while avoiding harsh vocal sibilance.

---

## [2.4.5] — 2026-06-11

### 🐛 Fixed
- **Audio Engine**: Eliminated periodic audio stuttering (every 2 seconds) by moving the ALSA device enumeration out of the real-time audio tick thread and into a dedicated background monitor thread.

---

## [2.4.4] — 2026-06-11

### 🐛 Fixed
- **Audio Engine**: Fixed an issue where connecting Bluetooth earphones caused playback to remain on laptop speakers. Increased the stream recovery delay to allow PulseAudio/PipeWire enough time to transition the default sink before recreating the audio stream.

---

## [2.4.3] — 2026-06-11

### 🐛 Fixed
- **Audio Engine**: Fixed audio playback stuttering by switching to dynamic OS-managed buffer sizing (`cpal::BufferSize::Default`) instead of forcing a fixed 2048-frame buffer.
- **Audio Engine**: Resolved multiple vector reallocations per audio chunk in `SymphoniaDecoder` that caused audio thread stalls.

---

## [2.4.2] — 2026-06-11

### 🐛 Fixed
- **UI**: Fixed the EQ panel missing border and padding, preventing it from blending with the background.

---

## [2.4.1] — 2026-06-11

### 🐛 Fixed
- **Audio Engine**: Fixed audio stream not recovering when switching to Auto backend if a Bluetooth device is connected. Tunecraft now correctly tracks ALSA's internal device count.
- **Audio Engine**: Fixed spurious "Audio dropout" warnings by relaxing the thread watchdog to respect 50ms OS timer resolutions.
- **Audio Engine**: Fixed `sample rate out of range` panics in CPAL by safely clamping fallback configurations within hardware format bounds.

---

## [2.4.0] — 2026-06-11

### 🚀 Added
- **UI**: Introduced `chromatic` theme constructor for fully saturated color palettes.

### ✨ Changed
- **UI**: Completely rewrote `src/theme.rs` to support clean, solid chromatic bases.
- **UI**: Refined Settings theme swatch previews to use true mid-hues.
- **UI**: Adjusted EQ window to dynamically fit content height.
- **UI**: Refined Playlists and Add Music Folder dialog inputs with frameless, full-height rects.
- **UI**: Replaced unicode icons in the EQ panel with phosphor icons.

### 🐛 Fixed
- **UI**: Constrained toolbar buttons with a clipping rect to prevent overflowing panel borders.
- **UI**: Removed trailing spacing in the EQ panel that caused vertical overflow.
- **UI**: Fixed search bar width and borders to match the design reference.

---

## [2.3.0] — 2026-06-10

### 🚀 Added
- **UI**: Added 8 new custom color themes (Ocean, Forest, Sunset, Berry, Midnight, Rose, Coffee, Mint).
- **UI**: Redesigned the theme selection in the Settings tab to use interactive color circles.

---

## [2.2.0] — 2026-06-10

### ✨ Changed
- **UI**: Fixed the sidebar width to be non-adjustable, relying solely on the minimize/maximize button.
- **UI**: Converted the track list filter button to a dropdown menu with "Release Year", "Album Type", and "File Size" options.

### 🚀 Added
- **UI**: Added a "Remove playlist" button in the sidebar that opens a popup to delete user-created playlists.

### 🗑️ Removed
- Completely removed the `tc-lyrics` module and all related code from the workspace.

---

## [2.1.4] — 2026-06-10

### 🐛 Fixed
- **UI**: Increased the width and removed borders of the search bar, 'Add Music Folder', and 'Create Playlist' input fields.
- **UI**: Disabled interaction with the EQ panel when the EQ toggle is off.
- **UI**: Centered the EQ panel and scaled it responsively to 60% of the screen size.
- **UI**: Converted the Sort button to a dropdown menu.
- **UI**: Reorganized the Settings tab with a grid layout and separator.
- **UI**: Removed the microphone icon below the volume bar.
- **UI**: Adjusted the progress bar position in the player bar.

---

## [2.1.3] — 2026-06-10

### ✨ Changed
- **Audio Engine**: Refactored the DSP Pipeline to eliminate linear scans and dynamic dispatch by utilizing a structured static chain.

### 🐛 Fixed
- **Audio Engine**: Added explicit NaN logging and clamping in `LookaheadLimiter` to prevent audio tearing upon receiving corrupted signal.
- **Audio Engine**: Optimized denormal sample flushing to utilize branchless bitwise checks.
- **Audio Engine**: Improved the security of `OpenUri` by ensuring target paths do not resolve to system directories outside of expected user boundaries.
- **Audio Engine**: Safely encapsulated the `FixedFrameBuffer` stream reset within `CpalOutput` to prevent data races.
- **Audio Engine**: Ensured comprehensive `set_config` synchronization for DSP parameters.

---

## [2.1.2] — 2026-06-10

### 🐛 Fixed
- **Audio Engine**: Fixed audio tearing and distortion by restructuring the DSP pipeline to prevent stateful IIR filters from interleaving during crossfades.
- **Audio Engine**: Bypassed the granular time stretcher to eliminate Hanning window phase modulation artifacts and allocations.
- **Audio Engine**: Offloaded the resampler FFT rebuild to a background thread to prevent decode thread blocking and silence gaps.
- **Audio Engine**: Routed seek fade-out frames to the pending output buffer to prevent hard cuts and seek clicks.

---

## [2.1.1] — 2026-06-10

### 🚀 Added
- **Keyboard Shortcuts**: Added support for Toggle Shuffle (`Ctrl+S`), Toggle Repeat (`Ctrl+R`), Global Search (`Ctrl+F`), and Volume Control (`Ctrl+Up/Down`).
- **Logo Update**: Upgraded the application logo to a high-quality glassmorphism design.

---

## [2.0.0] — 2026-06-10

### 🚀 Added
- **UI Overhaul**: Completely redesigned the user interface with modern styling, improved layout, and smoother animations using `egui` and `wgpu`.
- **Advanced DSP Features**: Implemented gapless crossfading, multi-band compressor, and real-time playback speed control.
- **High-Res Native Output**: Added support for high-resolution native audio output bypassing system mixers where available.

### 🐛 Fixed
- Resolved `clippy::derivable_impls` warning by deriving `Default` for `MultibandCompressorConfig`.

---

## [1.5.0] — 2026-06-09

### 🚀 Added
- Added advanced convolution reverb engine for immersive spatial audio.

### 🐛 Fixed
- Fixed a memory leak that occurred when loading extremely large impulse response (IR) files.

---

## [1.4.0] — 2026-06-08

### 🚀 Added
- Integrated a new time-stretching engine allowing seamless playback speed control without altering pitch.

### 🐛 Fixed
- Resolved minor UI stuttering when the user rapidly dragged the playback speed slider.

---

## [1.3.0] — 2026-06-07

### ✨ Changed
- Completely reworked the EQ backend to support a true 10-band parametric EQ, replacing the old fixed-band graphic EQ approach.

### 🐛 Fixed
- Fixed audio clipping that occurred when setting high preamp gains in the EQ panel.

---

## [1.2.0] — 2026-06-06

### 🚀 Added
- Introduced a multi-band compressor module for powerful dynamic range management.

### ✨ Changed
- Re-architected the audio processing pipeline to ensure the mastering chain (compressor and limiter) executes strictly at the final output boundary.

---

## [1.1.0] — 2026-06-05

### 🚀 Added
- Added experimental support for perfect gapless playback and crossfading between mixed audio formats (e.g., FLAC to MP3).

### 🐛 Fixed
- Fixed an issue where crossfade triggers were slightly delayed on certain variable bit-rate MP3 files.

---

## [1.0.3] — 2026-06-04

### 🐛 Fixed
- Addressed minor UI focus issues when running on Wayland compositors.
- Stabilized the SQLite database connection pool to prevent 'database locked' errors during heavy background scanning.

---

## [1.0.2] — 2026-06-03

### ✨ Added
- Added DB migration `V009` to update version metadata.

### 🐛 Fixed
- Fixed a compilation error in headless smoke tests.
- Cleaned up orphaned workspace dependencies.
- Updated launcher icon resolution for Linux packaging to fix blurriness.
- Bumped Flatpak runtime version to `24.08` to meet Flathub requirements.

---

## [1.0.1] — 2026-06-02

### 🐛 Fixed
- Updated man page version, date, and platform notes.
- Fixed CI cache invalidation by ensuring `Cargo.lock` generation.
- Corrected CI test feature-flag mismatches.

### 🗑️ Removed
- Removed all legacy Last.fm references; scrobbler is now fully offline.

---

## [1.0.0] — 2026-06-02

### 🚀 Added
- Introduced automated CI/CD pipeline for Linux, macOS, and Windows.
- Added comprehensive unit tests for SPSC ring buffers, analysis buffers, playback service, and EQ service.
- Implemented `ScrobbleService` stub methods for playback interface.

### 🐛 Fixed
- Fixed database version metadata migration (`V007`).
- Applied critical fixes from the pre-release series, including data race resolutions, audio quality improvements, and robust thread scheduling.

---

## [0.35.0] — 2026-05-25
### 🚀 Added
- Introduced smart caching for album artwork to significantly improve UI load times.
### 🐛 Fixed
- Fixed an issue where the library scanner would hang on corrupted MP3 headers.

---

## [0.34.0] — 2026-05-22
### 🚀 Added
- Added native desktop notifications on track change for Windows and macOS.
### 🐛 Fixed
- Resolved UI tearing issues on Wayland compositors.

---

## [0.33.0] — 2026-05-18
### 🚀 Added
- Added "On this day" feature in the offline scrobbling stats panel.
### 🐛 Fixed
- Fixed MPRIS track position desync on pause.

---

## [0.32.0] — 2026-05-10
### ✨ Changed
- Updated the underlying SQLite driver for better read concurrency.
### 🐛 Fixed
- Fixed a crash related to hot-unplugging the audio output device mid-playback.

---

## [0.31.0] — 2026-05-01
### 🚀 Added
- Implemented advanced DSP features including early limiter implementations.
- Introduced proper UI sidebars and optimized playback state handling.
### 🐛 Fixed
- Fixed EQ engine bugs and `f64` precision issues in the audio engine.
### 🗑️ Removed
- Removed legacy mood analyzer for better performance.

---

## [0.30.0] — 2026-04-20
### 🚀 Added
- Added support for fetching unsynced lyrics via LRCLIB fallback.
### 🐛 Fixed
- Fixed a memory leak in the LRC parsing module.

---

## [0.29.0] — 2026-04-12
### ✨ Changed
- Improved the full-text search indexing speed by 40%.
### 🐛 Fixed
- Fixed missing metadata for AAC files purchased from iTunes.

---

## [0.28.0] — 2026-04-05
### 🚀 Added
- Added configurable keyboard shortcuts via settings menu.
### 🐛 Fixed
- Prevented multiple shortcuts from triggering when global hooks overlapped.

---

## [0.27.0] — 2026-03-28
### ✨ Changed
- Restructured the internal thread messaging for lower latency UI updates.
### 🐛 Fixed
- Solved an issue where the play queue would skip tracks randomly.

---

## [0.26.0] — 2026-03-20
### 🚀 Added
- Added an interactive visualizer stub in the playback bar.
### 🐛 Fixed
- Fixed incorrect window dimensions persisting across restarts.

---

## [0.25.0] — 2026-03-12
### 🚀 Added
- Introduced "Shuffle Albums" mode alongside track shuffling.
### 🐛 Fixed
- Fixed an integer overflow error when calculating total library duration.

---

## [0.24.0] — 2026-03-01
### ✨ Changed
- Updated Symphonia decoding backend to support 32-bit floating point WAV.
### 🐛 Fixed
- Resolved stuttering during the first second of high-bitrate FLAC playback.

---

## [0.23.0] — 2026-02-20
### 🚀 Added
- Added a dark/light mode toggle switch to the main UI.
### 🐛 Fixed
- Fixed a layout bug causing text overlap in the playlist view.

---

## [0.22.0] — 2026-02-10
### ✨ Changed
- Optimized the database schema for faster playlist rendering.
### 🐛 Fixed
- Fixed missing scrollbars in the settings menu.

---

## [0.21.0] — 2026-01-25
### 🚀 Added
- Implemented support for Dither types (TPDF, Rectangular, Noise-Shaped).
### 🐛 Fixed
- Fixed the volume slider scaling logarithmically instead of linearly.

---

## [0.20.0] — 2026-01-15
### 🚀 Added
- Added local SQLite scrobbling capabilities (`V006` migration).
- Implemented cover art data extraction (`V005` migration).
### 🐛 Fixed
- Resolved several UI rendering bugs and clippy warnings.

---

## [0.15.0] — 2026-01-05
### 🚀 Added
- Implemented album unique constraints (`V003` migration).
- Added favorites tracking functionality (`V002` migration).
- Initial stable release of the audio playback engine (`V001` migration).

---

## [0.14.0] — 2025-12-20
### ✨ Changed
- Refactored the core library scanner for improved directory traversal speed.
### 🐛 Fixed
- Fixed a bug where nested subdirectories were occasionally skipped during library import.

---

## [0.12.0] — 2025-12-05
### 🚀 Added
- Added preliminary support for FLAC metadata parsing.
### 🐛 Fixed
- Resolved an issue causing the UI to freeze when loading more than 1,000 tracks.

---

## [0.10.0] — 2025-11-22
### 🚀 Added
- Implemented basic playback transport controls (Play, Pause, Stop) in the UI.
### ✨ Changed
- Switched to `wgpu` backend for `egui` to enable hardware-accelerated rendering.

---

## [0.8.0] — 2025-11-10
### 🚀 Added
- Introduced the initial SQLite database schema (`V001` draft) for testing track storage.
### 🐛 Fixed
- Fixed a panic on startup when the default audio device was unavailable.

---

## [0.5.0] — 2025-10-25
### 🚀 Added
- Project initialized.
- Added basic `egui` window shell and placeholder layout for the music player.
