# Changelog

<!--
  CHANGELOG GUIDELINES — PLEASE READ BEFORE EDITING

  1. VERSION FORMAT: Always use x.y format (e.g., 4.2, 3.18). Never use x.y.z.
     This project follows a two-part versioning scheme.

  2. HOW TO ADD A NEW ENTRY:
     - Add a new `## [x.y] — YYYY-MM-DD` section at the TOP (below these instructions).
     - Use one of these sub-headings: `### Added`, `### Changed`, `### Fixed`, `### Removed`.
     - Write one concise bullet per change. Focus on WHAT changed, not HOW.
     - No implementation details, no code references, no file paths.
     - Example:
       ### Fixed
       - EQ panel now correctly applies band changes to the audio engine

  3. KEEP IT BRIEF:
     - One line per change. If you need more than one line, the entry is too detailed.
     - No multi-paragraph explanations — that belongs in commit messages or PR descriptions.
     - No sub-sections for severity levels (Critical/High/Medium/Low).
     - No "New State Fields", "New Messages", "New Style Structs" implementation details.

  4. FORMAT follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
-->

---

## [5.0] — 2026-05-03

### Changed
- UI framework migrated from iced v0.14 to Dioxus v0.7 (desktop)
- Architecture changed from Elm-style (Model/Message/update/view) to component-based (React-like with hooks/signals)
- Styling migrated from iced style structs to CSS with light/dark theme support
- All UI elements rewired to TuneCraft Core (audio engine, database, queue, EQ, filters)
- Version bumped from 4.2 to 5.0 across all project files

### Removed
- iced dependency removed (replaced by Dioxus)
- tiny-skia dependency removed (no longer needed for software renderer fallback)
- Font Awesome / Inter font binary embeds removed (CSS web fonts instead)

---

## [4.2] — 2026-05-02

### Fixed
- Version format corrected to x.y across all project files (was incorrectly x.y.z)
- README and CHANGELOG trimmed of redundant and duplicated content

### Changed
- All version strings synchronized to 4.2

---

## [4.1] — 2026-05-02

### Fixed
- OS keyring integration for credential encryption (Linux/macOS/Windows)
- Build system version mismatch — all project files synchronized
- Missing SAFETY documentation for `unsafe impl Send` in gapless.rs

### Added
- 9 integration tests covering config, state machines, crypto, and validation

### Changed
- Replaced unmaintained `samplerate` crate with `rubato` for audio resampling
- Upgraded `rand` from 0.8 to 0.9
- Extracted `load_internal()` into `engine/loader.rs` for module decomposition
- Added `mutex_lock!` macro and sub-struct definitions for AppState decomposition

---

## [3.18] — 2026-05-02

### Fixed
- Track activation now plays from the visible view, not the queue
- Grid view index mapping for rows beyond the first
- Context menu actions resolve the correct track in filtered views
- Persistence on window close now saves playback state
- Playback speed is restored into the audio engine on startup
- EQ preamp slider now pushes gain to the audio engine
- Removing current queue track auto-advances instead of jumping to first
- Genre/year filter uses proper SQL instead of text search
- Playlist creation and drill-down now functional

---

## [3.17] — 2026-05-02

### Changed
- EQ panel redesigned: 10-band ISO frequencies, preset dropdown, bass/treble/stereo/balance sliders, dither toggle, M/S EQ toggle, preamp slider, and reset button
- Full light/dark theme support for all EQ panel elements

---

## [3.16] — 2026-05-02

### Fixed
- EQ UI now wired to audio engine (was calling wrong method)
- Queue shuffle order rebuilt after add/remove/clear/reorder
- Favorites use stable file_hash instead of volatile indices
- Theme and playback speed persist across restarts
- Removing playing track auto-advances correctly
- Album/artist cards drill down to filtered views
- Year range filter applied alongside genre filter
- Unicode-safe mood title capitalization

---

## [3.15] — 2026-05-01

### Added
- EQ panel, filter panel, queue panel, notifications panel
- Context menu with Play Next, Add to Queue, Go to Album/Artist, Add to Playlist
- Grid/list view switching
- Sidebar library views (Albums, Artists, Playlists, Settings)
- Playback speed control and stop button
- Dedicated search results view

---

## [3.14] — 2026-05-01

### Changed
- PcmCache capacity reduced from 10 to 5 entries (peak memory ~165 MB instead of ~330 MB)

### Added
- PcmCache wired into production mood analysis pipeline

### Fixed
- Version mismatch across project files synchronized

---

## [3.12] — 2026-05-01

### Fixed
- Sidebar, bell, EQ, filter, sort, grid/list, favorites, and context menu buttons all functional (were placeholders)
- Active track row styling updated to match mockup

---

## [3.11] — 2026-05-01

### Changed
- FontAwesome 6 icons replacing Unicode/emoji icons throughout the UI
- Inter font for all typography
- Circular album art and pill-shaped mood badges
- Responsive search bar

---

## [3.7] — 2026-05-01

### Changed
- DSP module split into five focused submodules (biquad, dynamics, gapless_smoother, ms_eq, seek_fade)

---

## [3.6] — 2026-05-01

### Fixed
- DRY cleanup: shared `cast_u8_to_f32()` utility
- Centralized `gstreamer::init()` calls
- Smooth seek-fade ramp replacing hard mute/unmute
- Monolithic `media_bridge.rs` split into per-platform modules

### Added
- Last.fm scrobble wiring with ScrobbleManager

---

## [3.5] — 2026-05-01

### Fixed
- All 62 bugs from v3.0 audit resolved (16 Critical, 17 High, 20 Medium, 9 Low)

---

## [3.0] — 2026-05-01

### Changed
- UI migrated from GTK4/libadwaita to iced v0.13 for cross-platform support
- AudioEngine decoupled from GLib — poll-driven bus and position updates

### Added
- MPRIS2 D-Bus (Linux), MPNowPlayingInfoCenter (macOS), SMTC (Windows)
- Cross-platform desktop notifications
- GStreamer bundling scripts for macOS and Windows
- GitHub Actions CI and release workflows
- Flatpak manifest, macOS entitlements, Windows WiX installer
- tiny-skia software renderer fallback
- Configurable ring buffer sizes and visualization modes

---

## [2.1] — 2026-04-30

### Changed
- AudioEngine Mutex fields consolidated from 15 locks into 3 grouped sub-structs
- All UI-layer `.expect()` calls replaced with mutex poison recovery

### Added
- Hindi locale translation
- Playlist import/export (M3U, XSPF)
- Shared PCM cache for mood analysis

### Fixed
- MPRIS state changes now emit D-Bus signals

---

## [2.0] — 2026-04-30

### Fixed
- 87 bugs including deadlocks, DSP corruption, heap allocations in audio path, and SQL injection

---

## [1.8] — 2026-04-30

### Fixed
- Player state and scrobble timer field access errors
- Recently Played smart playlist resolver
- Shuffle order regeneration edge case

---

## [1.7] — 2026-04-30

### Fixed
- Cover art extraction for FLAC and OGG Vorbis
- Memory leak in playback state callback
- MPRIS volume and position reporting

---

## [1.6] — 2026-04-30

### Added
- 10-band parametric EQ with genre presets
- Waveform seek bar with cached peaks
- Mood classification (Dance, Romantic, Sad, Sufi, Chill)

---

## [1.5] — 2026-04-30

### Added
- Smart playlist engine with rule builder and 8 templates
- LRCLIB lyrics integration with LRC parser

---

## [1.4] — 2026-04-30

### Added
- EBU R128 loudness measurement and normalization
- Bit-perfect / exclusive mode output
- Mid/Side EQ processing

### Fixed
- GaplessPreloader shares engine's DspEngine
- Underrun count properly wired

---

## [1.3] — 2026-04-30

### Added
- 50+ audio format support via GStreamer uridecodebin

---

## [1.1] — 2026-04-29

### Added
- Comprehensive unit tests for DSP, EQ, mood, playlists, and config
- OS keyring integration for credential encryption
- i18n framework with gettext-rs

### Fixed
- File watcher symlink escape prevention
- Scanner deduplication by canonical path

---

## [1.0] — 2026-04-29

### Fixed
- Audio pipeline panic on malformed buffers
- Deadlock during GLib callbacks
- MPRIS open_uri error handling
- Version numbers synchronized across all files

### Added
- Configuration validation and confirmation dialogs
- AppStream screenshots for Flathub

---

## [0.10] — 2026-04-29

### Fixed
- Panic-free audio pipeline with full mutex poison recovery

---

## [0.9] — 2026-04-28

### Added
- Hybrid GStreamer + cpal audio pipeline
- 10-band parametric EQ
- Mood-based discovery
- Last.fm scrobbling and MPRIS2 D-Bus
- Smart playlists

---

## [0.1] — 2024-01-15

### Added
- Initial release with core playback, library management, and UI
