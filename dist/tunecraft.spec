Name:           tunecraft
Version:        1.0.2
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

v1.0.2 is a bug-fix release that adds the missing ScrobbleService::is_available()
method (fixing a compile error in smoke test #6), removes the orphaned md-5
workspace dependency, upgrades the icon to 256x256, and bumps the Flatpak
runtime to org.freedesktop.Platform 24.08.

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
