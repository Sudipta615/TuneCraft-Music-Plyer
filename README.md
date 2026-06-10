<div align="center">

<img src="crates/tc-ui/icon.png" alt="TuneCraft Logo" width="160" />

# 🎵 TuneCraft Music Player

**A true audiophile-grade, cross-platform offline music player engineered in Rust.**

<p align="center">
  <a href="https://semver.org"><img src="https://img.shields.io/badge/version-2.1.1-blue.svg?style=for-the-badge" alt="Version" /></a>
  <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge" alt="License: MIT" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.82+-orange.svg?style=for-the-badge&logo=rust" alt="Rust" /></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg?style=for-the-badge" alt="Platform" />
</p>

> *"Where uncompromised audio fidelity meets state-of-the-art UI design."*

</div>

<br />

## 🌟 Why TuneCraft?

Most modern music players compromise on either performance, audio fidelity, or design. **TuneCraft** was built from the ground up to solve this. Utilizing a heavily optimized, zero-allocation DSP engine and a GPU-accelerated interface, it delivers studio-grade sound processing without taxing your CPU. 

---

## ✨ Features at a Glance

<table>
<tr>
<td width="50%">

### 🎧 Audiophile Engine
* **Native 64-bit Processing:** Pristine audio fidelity out to the hardware boundary.
* **True Parallel Decoding:** Flawless gapless crossfading using dual-decoder state machines.
* **Dynamic Split DSP:** Advanced pipeline processing independent audio streams simultaneously.
* **Parametric EQ & Convolution:** 10-band interactive EQ and IR-based spatial reverb.

</td>
<td width="50%">

### 🛡️ Unbreakable Playback
* **Real-Time OS Priority:** Audio callbacks run at `ThreadPriority::Max` ensuring zero stutter.
* **Auto-Recovery:** Hot-swaps output streams instantly if a device (e.g. Bluetooth) drops out.
* **Lock-Free Hot Paths:** Zero heap allocations via SPSC ring buffers in the audio thread.
* **Lookahead Limiting:** Safe, dynamic true-peak limiting protects your ears and gear.

</td>
</tr>
<tr>
<td width="50%">

### 📚 Smart Library & Scrobbling
* **Lightning SQLite:** WAL mode + FTS5 means instant search through hundreds of thousands of tracks.
* **Offline Scrobbling:** Fully local listen tracking, playback streaks, and "On this day" stats.
* **Comprehensive Metadata:** Deep parsing for ID3v2, Vorbis Comments, and MP4 atoms.

</td>
<td width="50%">

### 🎨 State-of-the-Art UI
* **GPU-Accelerated `egui`:** Buttery smooth animations rendered directly by `wgpu`.
* **Glassmorphism & Theming:** Deep dark modes, vibrant accents, and sleek UI components.
* **Synced Lyrics Integration:** Real-time fetching and syncing via LRCLIB.

</td>
</tr>
</table>

---

## 🏗️ Under the Hood

<details>
<summary><b>View Audio Pipeline Architecture</b></summary>

TuneCraft separates processing into distinct phases to enable complex overlapping features like smart crossfades.

```text
┌──────────┐    SPSC     ┌─────────────┐    SPSC     ┌────────────┐
│  Decode   │──Buffer──→│  DSP Thread  │──Buffer──→│   Output    │
│  Thread   │            │  Pipeline    │            │  (cpal)    │
└──────────┘            └─────────────┘            └────────────┘

[Crossfade Phase]
[Outgoing] → Preamp → Loudness → EQ → Convolution → Balance → Stereo ─┐
                                                                       ├→ Mixer → Limiter → Volume → Dither
[Incoming] → Preamp → Loudness → EQ → Convolution → Balance → Stereo ─┘
```
</details>

<details>
<summary><b>View Threading Model</b></summary>

TuneCraft strictly isolates the UI, file decoding, and audio output into dedicated threads.

```text
Main Thread (GUI)          Background Thread         Audio Callback
─────────────────          ─────────────────         ──────────────
egui::App::update()        engine.tick()              cpal callback
  ├─ Poll playback info     ├─ Process commands        ├─ Read output buffer
  ├─ Send commands          ├─ Decode audio chunk      └─ Write to device
  └─ Render UI              └─ Push to output buffer
```
</details>

---

## 🚀 Installation & Setup

### Prerequisites
* **Rust:** `1.82+` (Edition 2021)
* **Linux Specific:** `libasound2-dev` (ALSA headers)

### Compilation

```bash
# Clone the repository
git clone https://github.com/Sudipta615/TuneCraft-Music-Plyer.git
cd TuneCraft-Music-Plyer

# Compile with maximum optimizations (Recommended for DSP)
cargo build --release
```

### Execution

```bash
# Launch the graphical application
cargo run --release

# Play a specific file instantly
cargo run --release -- /path/to/masterpiece.flac

# Run headless mode (Background daemon)
cargo run --release -- --headless
```

---

## ⌨️ Advanced Keyboard Controls

| Action | Shortcut |
| :--- | :--- |
| **Play / Pause** | <kbd>Space</kbd> |
| **Next / Prev Track** | <kbd>Ctrl</kbd> + <kbd>→</kbd> / <kbd>←</kbd> |
| **Volume Control** | <kbd>Ctrl</kbd> + <kbd>↑</kbd> / <kbd>↓</kbd> |
| **Toggle Shuffle** | <kbd>Ctrl</kbd> + <kbd>S</kbd> |
| **Toggle Repeat** | <kbd>Ctrl</kbd> + <kbd>R</kbd> |
| **Global Search** | <kbd>Ctrl</kbd> + <kbd>F</kbd> |

---

## ⚙️ Configuration

TuneCraft uses a highly readable `.toml` file for absolute control over its engine. 
Find it at `~/.config/tunecraft/config.toml` (Linux/macOS) or `%APPDATA%\tunecraft\config.toml` (Windows).

* `engine.performance_mode` — Choose between `UltraQuality`, `Balanced`, or `LowPower`.
* `engine.eq.enabled` — Master switch for the DSP pipeline.
* `engine.crossfade.duration_ms` — Millisecond-accurate crossfade timings.

---

<div align="center">
  <p>Crafted with ❤️ for audiophiles. Licensed under the <strong>MIT License</strong>.</p>
</div>
