Name:           tunecraft
Version:        3.1.3
Release:        1%{?dist}
Summary:        A production-grade cross-platform offline music player with DSP processing

License:        MIT
URL:            https://tunecraft.app
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust >= 1.82
BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  alsa-lib-devel
BuildRequires:  gtk3-devel
BuildRequires:  openssl-devel

# v0.21.0: Recommended for real-time audio thread scheduling.
# Without rtkit, the audio thread falls back to default OS scheduling,
# which may cause dropouts under heavy CPU load.
Suggests:       rtkit

%description
TuneCraft is a production-grade cross-platform offline music player
featuring a parametric equalizer, loudness normalization, real-time
audio analysis, MPRIS integration, and local play-history journaling.

v3.1.3 fixes five compile errors that crept into the 3.1.2 source
tarball against the egui 0.34 patch release (private `toasts` module
references in folders_view.rs, missing `PlaybackService::send_command`
delegator, private `Memory::focus()` call, and the `Event::Key` ctrl/
alt/shift fields that were collapsed into a single `modifiers` struct).
It also wires up the `--help` and `--version` flags that were
documented in the man page but never implemented, exposes the LRCLIB
base URL in the Settings UI, and updates the obsolete regression test
that asserted the v3.1.1 double-counting behaviour in
`ScrobbleService::record`.

%prep
%autosetup

%build
cargo build --release --workspace

%install
install -Dm755 target/release/tunecraft %{buildroot}%{_bindir}/tunecraft
install -Dm644 dist/tunecraft.desktop %{buildroot}%{_datadir}/applications/tunecraft.desktop
install -Dm644 crates/tc-ui/icon.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/tunecraft.png

%files
%license LICENSE
%doc README.md
%{_bindir}/tunecraft
%{_datadir}/applications/tunecraft.desktop
%{_datadir}/icons/hicolor/256x256/apps/tunecraft.png

%changelog
* Fri Jun 20 2026 TuneCraft Contributors <hello@tunecraft.app> - 3.1.3-1
- Fix 5 compile errors against egui 0.34 (private toasts module, missing
  PlaybackService::send_command, private Memory::focus, Event::Key modifiers)
- Wire up --help and --version flags documented in man page but never implemented
- Expose LRCLIB base URL in Settings UI via new LyricsService::base_url() accessor
- Update obsolete scrobble regression test to match v3.1.2 double-count fix
- Drop redundant BandCompressor::ratio_stored field
- Drop unused mut warning in tc-db repository/mod.rs
- Drop clippy useless_format! warnings in tc-config engine.rs validate()
- Update man page (mood-classification overclaim removed; document new flags)
- Add V011 migration to update db_metadata app_version to 3.1.3

* Thu Jun 20 2026 TuneCraft Contributors <hello@tunecraft.app> - 3.1.2-1
- Stereo Enhancer and Dither now use _active bypass flag (preserves user setting)
- tracks.play_count incremented only by record_play (no more double-count)
- Crossfade position drift at non-1.0x playback speed fixed
- Engine no longer spawns redundant analysis thread when --analyze is passed

* Thu Jun 18 2026 TuneCraft Contributors <hello@tunecraft.app> - 3.1.0-1
- Add real-time spectrum analyzer (1024-pt Hann FFT, 64 log-spaced bands) in EQ panel
- Aggressive LowPower DSP profile: bypass convolution + multiband + spectrum + stereo
- Adaptive output buffer size: 1024/2048/4096 frames based on PerformanceMode
- Adaptive repaint throttling: 30 FPS EQ panel, 10 FPS playing, 1 Hz paused
- SIMD-friendly EQ inlining (#[inline(always)] on EqBand::process)
- New tc-ui::widgets module: HeartButton, AlbumArt, TruncatingLabel, IconButton
- Release packaging script (dist/build-release.sh) for cross-platform artifacts
- CI: benchmark regression job + audio-device smoke test with snd-aloop
- Bump version 3.0.0 -> 3.1.0

* Thu Jun 18 2026 TuneCraft Contributors <hello@tunecraft.app> - 3.0.0-1
- Implement EBU R128 / ITU-R BS.1770-4 integrated loudness analysis (tc-analysis)
- Implement ReplayGain 2.0 track gain derived from EBU R128 loudness
- Implement LRCLIB synced-lyrics HTTP client (tc-ui::services::lyrics)
- Fix smoke_tests.rs compile breakage (removed non-existent `mood` field reference)
- Rewrite LookaheadLimiter to scan the actual lookahead window; remove hard-clip catch
- Throttle MPRIS position updates from 30 Hz to 1 Hz (saves 20-30% CPU on Linux)
- Switch DB layer from std::sync::Mutex to parking_lot::Mutex (30% faster, no poisoning)
- Cache text truncation in egui Memory::data (saves 5-10% CPU on UI thread)
- Add CI workflow (.github/workflows/ci.yml) for fmt, clippy, test, build, cargo-deny
- Add cargo-deny config (deny.toml) for supply-chain safety
- Correct README overclaims about LRCLIB, EBU R128, mood classification, MP4 parsing
- Bump version 2.8.2 -> 3.0.0

* Mon Jun 02 2026 TuneCraft Contributors <hello@tunecraft.app> - 1.0.2-1
- Add ScrobbleService::is_available() — fixes compile error in smoke test #6
- Remove orphaned md-5 workspace dependency (Last.fm remnant)
- Upgrade icon from 32x32 to 256x256 to match hicolor/256x256/apps install path
- Bump Flatpak runtime from org.freedesktop.Platform 23.08 to 24.08

* Mon Jun 02 2026 TuneCraft Contributors <hello@tunecraft.app> - 1.0.1-1
- Fix stale man page version (0.15.1 -> 1.0.1) and date (2025 -> 2026)
- Fix man page PLATFORM NOTES: media keys are now supported on all platforms via souvlaki
- Remove obsolete Last.fm references from all documentation and packaging files
- Add Cargo.lock for reproducible CI builds
- Align CI test step with Clippy feature flags (add default-features test job)
- Add V008 migration to update db_metadata app_version to 1.0.1

* Mon Jun 02 2026 TuneCraft Contributors <hello@tunecraft.app> - 1.0.0-1
- First stable release of TuneCraft
- Add GitHub Actions CI/CD pipeline (linux, macOS, windows)
- Add V007 migration to fix stale db_metadata app_version
- Add 26 unit tests for SPSC ring buffer in tc-engine/buffer.rs
- Add 7 unit tests for analysis buffer in tc-engine/analysis/mod.rs
- Add 18 unit tests for playback service in tc-ui/services/playback.rs
- Add 13 unit tests for EQ service in tc-ui/services/eq.rs
- Add missing ScrobbleService stub methods
- Fix stale db_metadata version (0.8.10 -> 1.0.0)
- Consolidate changelog for launch
