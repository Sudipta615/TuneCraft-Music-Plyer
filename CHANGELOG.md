# Changelog

All notable changes to TuneCraft are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.2] — 2026-06-03

Bug-fix release addressing issues found during final pre-ship review of v1.0.1.

### Fixed

- **Compile error in smoke test #6** (`tests/smoke_tests.rs`):
  `test_app_context_init_headless` called `ctx.scrobble.is_available()` but
  `ScrobbleService` only exposed `is_enabled()`. This caused a compile error
  that prevented `cargo test` from running on any platform. Added
  `ScrobbleService::is_available()` (returns `true` unconditionally — the
  offline journal has no network dependency to be unavailable).

- **Orphaned `md-5` workspace dependency** (`Cargo.toml`):
  `md-5 = "0.10.6"` was declared in `[workspace.dependencies]` but referenced
  by no crate — a leftover from the Last.fm scrobbler removed in v0.31.0.
  Removed to keep the dependency graph clean and avoid confusing security audits.

- **Icon undersized for declared install path** (`crates/tc-ui/icon.png`):
  The icon was a 32×32 PNG but all three packaging targets (RPM spec,
  `build-deb.sh`, Flatpak manifest) installed it to the
  `hicolor/256x256/apps/` slot, causing a blurry launcher icon. Replaced
  with a 256×256 version.

- **Flatpak runtime version outdated** (`dist/net.tunecraft.TuneCraft.yml`):
  Manifest referenced `org.freedesktop.Platform 23.08` (September 2023),
  which is past its maintenance window by mid-2026 and blocked Flathub
  submission. Bumped to `24.08`.

### Added

- **DB migration V009** (`crates/tc-db/migrations/V009__update_version_1_0_2.sql`):
  Updates `db_metadata.app_version` from `1.0.1` to `1.0.2`.

## [1.0.1] — 2026-06-02

Bug-fix release addressing documentation inaccuracies, obsolete references,
CI configuration issues, and version alignment discovered during pre-ship
review of v1.0.0.

### Fixed

- **Man page version and date** (`dist/tunecraft.1`):
  The man page header still read `TuneCraft 0.15.1` with a `2025` date.
  Updated to `TuneCraft 1.0.1` and `2026`.

- **Man page PLATFORM NOTES** (`dist/tunecraft.1`):
  The man page incorrectly stated that media keys and MPRIS were
  "not yet implemented" on macOS and Windows.  Since v0.20.0, the
  `souvlaki` crate provides cross-platform media key support
  (MPRemoteCommandCenter on macOS, SMTC on Windows).  The section
  now correctly documents platform-specific backends.

- **CI cache invalidation** (`.github/workflows/ci.yml`):
  The cache key referenced `hashFiles('**/Cargo.lock')` but no
  `Cargo.lock` was committed to the repository.  This caused the
  cache hash to always be empty, defeating caching entirely.  A new
  CI step now generates `Cargo.lock` before caching.  For a binary
  application, `Cargo.lock` should always be committed to guarantee
  reproducible builds and prevent silent dependency updates from
  breaking CI.

- **CI test feature-flag mismatch** (`.github/workflows/ci.yml`):
  Clippy ran with `--all-features` but the test step used
  `--no-default-features`, meaning the `audio-output` and `gui`
  feature code paths were linted but never tested.  A second test
  job now runs `cargo test --workspace` with default features to
  ensure those code paths are exercised.

### Removed

- **Last.fm references from documentation and packaging**:
  The scrobbler has been fully offline since v0.31.0 (local SQLite
  play journal).  All references to Last.fm — including API key
  environment variables, scrobble queue file documentation, and
  feature descriptions — have been removed from the man page,
  README, RPM spec, and `.deb` build script.  Source code comments
  referencing Last.fm have also been updated.

## [1.0.0] — 2026-06-02

First stable release of TuneCraft, a production-grade cross-platform offline music
player built in Rust. This version incorporates all bug fixes, performance
improvements, and architectural hardening from the 0.15.x through 0.31.x
pre-release series.

### Added

- **CI/CD pipeline** (`.github/workflows/ci.yml`):
  Automated continuous integration via GitHub Actions running on ubuntu-latest,
  macos-latest, and windows-latest. Each platform runs `cargo fmt --check`,
  `cargo clippy -- -D warnings`, `cargo test --workspace --no-default-features`,
  and `cargo build --release --workspace` on every push and pull request.

- **Database version metadata migration** (`crates/tc-db/migrations/V007__fix_version_metadata.sql`):
  Updates the stale `app_version` value in `db_metadata` from `0.8.10` to `1.0.0`.
  The version was left unchanged through 15 releases after V005.

- **SPSC ring buffer tests** (`crates/tc-engine/src/buffer.rs`):
  26 unit tests covering AudioFrame construction, scaling, interpolation, mono-to-stereo
  promotion, SPSC Producer/Consumer push/pop semantics, buffer wrap-around, fill-and-drain,
  capacity validation, FixedFrameBuffer compatibility shim, PlaybackInfo defaults,
  EngineCommand debug/clone, denormal prevention helpers, and AudioChunk creation.

- **Analysis buffer tests** (`crates/tc-engine/src/analysis/mod.rs`):
  7 unit tests covering AnalysisBuffer construction, capacity and decimation validation,
  feed-and-read with decimation, circular buffer wrap-around, and reset functionality.

- **Playback service tests** (`crates/tc-ui/src/services/playback.rs`):
  18 unit tests covering PlaybackState defaults, volume and speed clamping, shuffle order
  generation and validation, repeat mode behavior, queue navigation (sequential,
  repeat-all wrap, repeat-off end), previous navigation, queue management, stop/reset,
  seek, scrobble threshold for short tracks, and version counter increments.

- **EQ service tests** (`crates/tc-ui/src/services/eq.rs`):
  13 unit tests covering EqState defaults, EQ frequency constants, service initialization,
  enable/disable toggling, band gain setting (in-range and out-of-range), preamp, stereo
  width clamping, balance, dither, Mid/Side mode, panel visibility toggle, and full
  parameter band setting.

- **ScrobbleService stub methods** (`crates/tc-ui/src/services/scrobble.rs`):
  Added `update_now_playing()` and `clear_now_playing()` stub methods required by the
  playback service interface.

### Changed

- **Version bumped** from 0.31.0 to 1.0.0 across workspace `Cargo.toml`, RPM spec,
  `.deb` build script, README, and smoke tests.

### Summary of fixes from pre-release series (0.15.0 through 0.31.0)

The following fixes were applied during the pre-release stabilization period and are
included in this release:

**Critical fixes:** Data race in Stop command (pause-before-reset), crossfade resampling
bypass, use-after-move in PoisonError handlers, OOB write in convolution overlap-add,
stream recovery rebuilding only incoming resampler, concurrency data race in load_track,
double into_inner() consumption, and inverted ReplayGain sign.

**Audio quality:** Volume reset on track load, stereo width reset on seek, crossfade
silence gaps from incomplete resampler reads, convolution IR tail never output,
and DspPipeline::set_sample_rate() omitting convolution engine.

**Robustness:** Frame dropout under CPU load (pending chunk caching), crossfade decode
dropped frames on buffer stall, redundant file open in prepare_next_track, and
zero-allocation audio callback via pre-allocated scratch buffers.

**Architecture:** True parallel decoding with dual-decoder state machine, DSP pipeline
split processing for crossfade, cross-platform media keys via souvlaki, real-time
audio thread scheduling, and automatic stream recovery on device disconnection.

**UI/State:** MPRIS OpenUri desync, scrobble threshold at non-1x speeds, shuffle context
loss on manual selection, navigate_prev() version increment, toggle_playback state
sync race, play_next empty-queue vs repeat-off-end distinction, EQ panel persistence
and state overwrite, and toast alpha inconsistency.

**Scrobble:** Queue-mode event emission, threshold for 30-60s tracks, missing duration
in API call, and shuffle wrap-around replay prevention.

**Infrastructure:** Module visibility fixes, dead code removal, doc comment cleanup
(165 broken comments across 39 files), clippy.toml MSRV alignment, and atomic write
TOCTOU gap elimination.
