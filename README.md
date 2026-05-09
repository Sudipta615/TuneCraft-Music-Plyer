# TuneCraft

**Professional cross-platform offline music player**

Built with Rust, Dioxus, and GStreamer — featuring mood-based discovery, smart playlists, a 10-band parametric EQ, and deep desktop integration across Linux, macOS, and Windows.

[![CI](https://github.com/tunecraft/tunecraft/actions/workflows/ci.yml/badge.svg)](https://github.com/tunecraft/tunecraft/actions/workflows/ci.yml)
[![Release](https://github.com/tunecraft/tunecraft/actions/workflows/release.yml/badge.svg)](https://github.com/tunecraft/tunecraft/actions/workflows/release.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

---

## Attention

** Hey guys Shasow is here, first I want to clarify I am a non-coder guy and I don't believe in Vibe Coder bullshit. I just love tech so I tried to build a music player for my own old system, but I am dropping it as I don't have time I am focusing on studies. This project was bild completely using AI so it has many bugs, though I fixed most of them. So if anyone is interested on completing it fell free to do so and also let me know, I will be happy to try the final version.
---

## Features

- **Hybrid three-thread audio pipeline** — GStreamer decode, Rust DSP processing, cpal output
- **10-band parametric EQ** with ISO frequencies, 6 presets, bass/treble shelves, M/S widening, and AutoEQ import
- **Mood analysis** — Automatic BPM detection and classification (Dance, Romantic, Sad, Sufi, Chill)
- **Smart playlists** — Rule-based with AND/OR groups, 8 built-in templates
- **Crossfade & gapless playback** with configurable fade duration
- **ReplayGain & EBU R128 loudness normalization**
- **Last.fm scrobbling** with OAuth and queue-based batch submission
- **LRCLIB lyrics** with synced LRC highlighting
- **Desktop integration** — MPRIS2 (Linux), MPNowPlayingInfoCenter (macOS), SMTC (Windows)
- **Waveform seek bar & real-time spectrum analyzer**
- **Session persistence** — Volume, queue, EQ preset, theme, and playback speed restored on startup
- **Cross-platform** — Linux (AppImage, Flatpak), macOS (.dmg), Windows (.msi, portable .zip)

---

## Installation

Download the latest release from [GitHub Releases](https://github.com/tunecraft/tunecraft/releases):

| Platform | Format | Instructions |
|----------|--------|-------------|
| Linux | AppImage | `chmod +x Tunecraft-*.AppImage && ./Tunecraft-*.AppImage` |
| Linux | Flatpak | `flatpak-builder build-dir build-aux/flatpak/org.tunecraft.Tunecraft.json --user --install` |
| macOS | .dmg | Open .dmg, drag Tunecraft to Applications |
| Windows | .msi | Run the MSI installer |
| Windows | .zip | Extract and run `Tunecraft.exe` |

---

## Building from Source

### Prerequisites

| Dependency | Minimum Version | Purpose |
|-----------|----------------|---------|
| Rust | 1.75+ (edition 2021) | Compiler |
| GStreamer | 1.20+ | Audio decoding pipeline |
| SQLite3 | 3.x | Library database (bundled via rusqlite) |

### Install GStreamer

**Ubuntu/Debian:**
```bash
sudo apt install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav libsqlite3-dev
```

**Fedora:**
```bash
sudo dnf install gstreamer1-devel gstreamer1-plugins-base-devel \
    gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-libav sqlite-devel
```

**macOS:**
```bash
brew install gstreamer
```

**Windows:** Download the GStreamer MSVC 64-bit runtime from [gstreamer.freedesktop.org](https://gstreamer.freedesktop.org/download/) and set the `GSTREAMER_1_0_ROOT_MSVC_X86_64` environment variable.

### Build & Run

```bash
git clone https://github.com/tunecraft/tunecraft.git
cd tunecraft
cargo build --release
cargo run --release
```

For low-end hardware without GPU support:
```bash
TUNECRAFT_LOG_LEVEL=debug cargo run --release
```

### Running Tests

```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

---

## Configuration

TuneCraft uses a TOML config file, auto-generated on first launch:

| Platform | Config Path |
|----------|------------|
| Linux | `~/.config/TuneCraft/tunecraft.toml` |
| macOS | `~/Library/Application Support/com.tunecraft.Tunecraft/tunecraft.toml` |
| Windows | `%APPDATA%\com.tunecraft.Tunecraft\config\tunecraft.toml` |

See [config.toml.sample](config.toml.sample) for the full configuration reference with all available options.

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Space` | Play / Pause |
| `N` / `P` | Next / Previous Track |
| `S` | Toggle Shuffle |
| `R` | Cycle Repeat Mode |
| `F5` | Rescan Library |
| `Ctrl+,` | Open Preferences |
| `Ctrl+Q` | Quit |
| `Ctrl+N` | New Playlist |
| `Ctrl+L` | Show Lyrics |
| Media keys | Next, Previous, Volume |

---

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Ensure all checks pass: `cargo fmt`, `cargo clippy`, `cargo test`
4. Submit a Pull Request against the `main` branch

---

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
