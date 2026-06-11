# 📝 Changelog

All notable changes to **TuneCraft** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
