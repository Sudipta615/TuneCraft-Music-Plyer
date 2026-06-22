<div align="center">

<img src="crates/tc-ui/icon.png" alt="TuneCraft Logo" width="160" />

# 🎵 TuneCraft Music Player

**A true audiophile-grade, cross-platform offline music player engineered in Rust.**

<p align="center">
  <a href="https://semver.org"><img src="https://img.shields.io/badge/version-3.1.3-blue.svg?style=for-the-badge" alt="Version" /></a>
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
* **Parametric EQ & Convolution:** 10-band parametric EQ and IR-based spatial reverb.
* **Real-Time Spectrum Analyzer:** v3.1.0 — 1024-pt Hann FFT tap rendered as 64 log-spaced bars behind the EQ curve, so you see exactly what your EQ is doing.

</td>
<td width="50%">

### 🛡️ Unbreakable Playback
* **Real-Time OS Priority:** Audio callbacks run at `ThreadPriority::Max` ensuring zero stutter.
* **Auto-Recovery:** Hot-swaps output streams instantly if a device (e.g. Bluetooth) drops out.
* **Lock-Free Hot Paths:** Zero heap allocations via SPSC ring buffers in the audio thread.
* **Lookahead Limiting:** v3.0.0 true-lookahead brick-wall limiter with no hard-clip catch.

</td>
</tr>
<tr>
<td width="50%">

### 📚 Smart Library & Scrobbling
* **Lightning SQLite:** WAL mode + FTS5 means instant search through hundreds of thousands of tracks.
* **Offline Scrobbling:** Fully local listen tracking, playback streaks, and "On this day" stats.
* **Container Parsing:** ID3v2 (MP3), Vorbis Comments (FLAC/OGG), and MP4 container metadata via Symphonia.

</td>
<td width="50%">

### 🎨 State-of-the-Art UI
* **GPU-Accelerated `egui`:** Buttery smooth animations rendered directly by `wgpu`.
* **Glassmorphism & Theming:** Deep dark modes, vibrant accents, and sleek UI components.
* **Synced Lyrics Integration:** v3.0.0 real-time fetching and syncing via [LRCLIB](https://lrclib.net), cached in the local DB so subsequent plays never hit the network.

</td>
</tr>
<tr>
<td width="50%">

### 🔊 Loudness Normalization
* **EBU R128 / ITU-R BS.1770-4:** v3.0.0 real integrated loudness measurement (K-weighting, absolute + relative gating, 400 ms / 75 % overlap blocks).
* **ReplayGain 2.0:** Derived track gain from EBU R128 (−18 LUFS reference).
* **True Peak Guard:** Sample-peak tracking with 1 dB safety margin; prevents inter-sample clipping.

</td>
<td width="50%">

### ⚡ Performance (v3.1.0)
* **1 Hz MPRIS Position Throttle:** Was 30 Hz, responsible for 20–30 % CPU on Linux.
* **20 ms / 100 ms Engine Tick Polling:** Was 5 ms / 50 ms — 9× headroom over the output buffer.
* **Cached Text Truncation:** ~150–450 text layouts per frame eliminated via egui `Memory::data` cache.
* **`parking_lot::Mutex` Everywhere:** Including the DB layer (was `std::sync::Mutex`).
* **Adaptive Output Buffer (v3.1.0):** 1024 / 2048 / 4096 frames based on `PerformanceMode` — halves context-switch rate on low-end hardware.
* **Aggressive LowPower DSP (v3.1.0):** Bypasses convolution + multiband + spectrum + stereo enhancer + dither. Cuts DSP CPU by ~60 % on Celeron-class hardware.
* **Adaptive Repaint Throttle (v3.1.0):** 30 FPS when EQ panel is open (spectrum animation), 10 FPS when playing (progress bar), 1 Hz when paused. Idle CPU drops from ~8 % to ~0.5 % on a Celeron N3050.
* **SIMD-friendly EQ Inlining (v3.1.0):** `EqBand::process` marked `#[inline(always)]` for full loop unrolling + AVX2 auto-vectorization across the L/R pair.

</td>
</tr>
</table>

---

## 🏗️ Under the Hood

<details>
<summary><b>View Audio Pipeline Architecture</b></summary>

TuneCraft separates processing into distinct phases to enable complex overlapping features like smart crossfades.

**Threading reality (v3.0.0 corrected):** decoding and DSP both happen on the same `engine.tick()` background thread; only the audio output callback runs on a separate real-time thread. The previous README implied three separate threads, which was inaccurate.

```text
┌─────────────────────────────────┐         SPSC          ┌────────────┐
│   engine.tick() thread          │  ────output──────→    │   Output   │
│   ├─ Decode (Symphonia)         │                       │  (cpal)    │
│   ├─ DSP Pipeline               │                       │  callback  │
│   │   ├─ Preamp → Loudness      │                       └────────────┘
│   │   ├─ EQ → Convolution       │
│   │   └─ Balance → Stereo       │
│   └─ Crossfade Mixer → Limiter  │
└─────────────────────────────────┘

[Crossfade Phase] — dual decoders feed into the same DSP pipeline
[Outgoing] → Preamp → Loudness → EQ → Convolution → Balance → Stereo ─┐
                                                                       ├→ Mixer → Limiter → Volume → Dither
[Incoming] → Preamp → Loudness → EQ → Convolution → Balance → Stereo ─┘
```
</details>

<details>
<summary><b>View Threading Model</b></summary>

TuneCraft strictly isolates the UI, file decoding + DSP, and audio output into dedicated threads.

```text
Main Thread (GUI)          Background Thread         Audio Callback
─────────────────          ─────────────────         ──────────────
egui::App::update()        engine.tick()              cpal callback
  ├─ Poll playback info     ├─ Process commands        ├─ Read output buffer
  ├─ Send commands          ├─ Decode audio chunk      └─ Write to device
  └─ Render UI              ├─ Run DSP pipeline
                            └─ Push to output buffer
                            (50 Hz while playing, 10 Hz idle — v3.0.0)
```
</details>

<details>
<summary><b>View Loudness Analysis Pipeline (v3.0.0)</b></summary>

Loudness analysis runs on the background `tunecraft-audio-analysis` thread alongside BPM and chroma detection, sharing the same Symphonia decode loop:

```text
File ──→ Symphonia decoder ──┬─→ BpmDetector       (onset-strength)
                              ├─→ ChromaDetector    (12 Goertzel + KS profiles)
                              └─→ LoudnessAnalyzer  (K-weighting + 400 ms blocks)
                                    │
                                    ├─ Absolute gate: drop blocks < −70 LUFS
                                    ├─ Relative gate: drop blocks < (mean − 10 LU)
                                    └─ Integrated loudness = −0.691 + 10·log₁₀(mean_ms)
                                          │
                                          ├─→ ebu_r128_loudness (LUFS)
                                          ├─→ ebu_r128_peak (dBTP, sample peak + 1 dB margin)
                                          └─→ replaygain_track_db = −18 − loudness (RG 2.0)
```

These values are persisted via `Database::update_loudness_meta` and read back by `LoudnessNormalizer` at playback time. The `LoudnessMode` config (`off` / `track_replay_gain` / `album_replay_gain` / `ebu_r128`) determines which value drives the gain computation.
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

# Force loudness analysis on all unanalyzed tracks
cargo run --release -- --analyze
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

* `engine.performance_mode` — Choose between `ultra_quality`, `balanced`, or `low_power`.
* `engine.eq.enabled` — Master switch for the DSP pipeline.
* `engine.crossfade.duration_ms` — Millisecond-accurate crossfade timings.
* `engine.loudness.mode` — One of `off`, `track_replay_gain`, `album_replay_gain`, `ebu_r128`.
* `engine.loudness.target_lufs` — EBU R128 reference loudness (default: −23 LUFS).
* `engine.loudness.true_peak_guard` — Boolean, clamp to `true_peak_dbtp` ceiling.
* `lyrics.enabled` — Master switch for LRCLIB network access (default: true).
* `lyrics.fetch_on_play` — Auto-fetch lyrics when a track starts (default: true).
* `lyrics.base_url` — LRCLIB instance URL; can point to a self-hosted instance.

---

<div align="center">
  <p>Crafted with ❤️ for audiophiles. Licensed under the <strong>MIT License</strong>.</p>
</div>
