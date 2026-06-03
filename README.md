# TuneCraft v1.0.2

A production-grade, cross-platform offline music player built in Rust.
Core audio playback, DSP processing, library management, media key
integration, and stream recovery all work on **Linux, macOS, and Windows**.

## Features

### Audio Engine
- **True parallel decoding**: Dual-decoder state machine (`PlaybackStream`) with `Single` and `Transitioning` variants for real crossfading — distinct sample streams from outgoing and incoming decoders are fed into the `TrackMixer` simultaneously
- **DSP pipeline split processing**: During crossfade, outgoing and incoming tracks are processed independently through the first half of the pipeline (Preamp → Loudness → EQ → Convolution → Balance → Stereo), then mixed, then the combined signal passes through the second half (Limiter → Volume → SeekFade → Dither)
- **64-bit float processing** throughout the pipeline, 32-bit at output boundary
- **Zero-allocation audio callback**: SPSC ring buffers, pre-allocated FFT workspaces, no heap alloc in hot path
- **Format support**: MP3, FLAC, OGG, WAV, AAC (via Symphonia)
- **Sample rate conversion**: Rubato-based resampler with 3 quality profiles (HighQuality, Balanced, Fast) — applied to both single-stream and crossfade paths
- **Loudness normalization**: EBU R128 and ReplayGain support with K-weighting filter
- **10-band parametric EQ**: Peaking, low-shelf, high-shelf filters with smooth coefficient interpolation, asymmetric headroom management, and optional Mid/Side mode
- **Lookahead limiter**: True peak protection with configurable ceiling
- **Crossfade/gapless**: Equal-power and S-curve crossfade with smart boundaries and sample-accurate trigger timing
- **Dither**: TPDF, rectangular, and noise-shaped dither types
- **Three performance modes**: Ultra Quality, Balanced, Low Power

### Real-Time Audio Scheduling
- **Real-time thread priority**: Audio callback thread escalated to `ThreadPriority::Max` via the `thread-priority` crate
- **Graceful fallback**: If priority escalation fails (e.g., no rtkit on Linux), audio continues with default scheduling and a warning is logged
- **Linux requirements**: rtkit permissions, `ulimit -r` adjustments, or `CAP_SYS_NICE` capability — documented in packaging specs

### Hardware & Stream Recovery
- **Device disconnect detection**: CPAL error callback sets an `AtomicBool` flag when the audio device is removed (USB unplug, Bluetooth disconnect)
- **Automatic recovery**: Engine re-detects the output device, rebuilds the DSP pipeline and resampler coefficients at the new sample rate, and hot-swaps the output stream — no restart required
- **Capped retry**: Recovery is limited to 5 attempts to prevent infinite loops when no audio device is available
- **Both resamplers rebuilt**: During recovery in crossfade state, both outgoing and incoming resamplers are rebuilt to match the new sample rate

### Library Management
- **SQLite database** with WAL mode, FTS5 full-text search, and automatic migrations
- **Directory scanning** with recursive walk and audio file detection
- **Metadata extraction** from ID3v2 (MP3), Vorbis Comments (FLAC/OGG), and MP4 atoms
- **Play statistics**: Play count tracking, last played timestamps
- **Batch analysis**: BPM detection (energy-based onset + autocorrelation with mean subtraction) and mood classification (energy-valence grid with complete coverage)

### User Interface
- **egui/eframe** desktop GUI with GPU-accelerated rendering (wgpu)
- **Dark/light themes** with purple accent (#4231F1)
- **Sidebar navigation**: Library, Playlists, Mood categories
- **Track list** with sortable columns, search filtering, and mood tags
- **Player bar**: Transport controls (play/pause/next/prev/shuffle/repeat), progress bar with seek, volume
- **EQ panel**: 10-band parametric EQ with interactive sliders, presets (Flat/Bass Boost/Treble Boost/V-Shape/Vocal), preamp, stereo width, dither toggle
- **Lyrics display**: Synced lyrics with current-line highlighting from LRCLIB

### Platform Integration
- **Cross-platform media keys** via `souvlaki`:
  - **Linux**: MPRIS D-Bus (via souvlaki's MPRIS backend)
  - **macOS**: MPRemoteCommandCenter (via souvlaki's macOS backend)
  - **Windows**: SystemMediaTransportControls (via souvlaki's Windows backend)
- **Advanced MPRIS D-Bus** (Linux-only): Full `org.mpris.MediaPlayer2.Player` interface with dedicated Play/Pause/Quit actions, Seek, SetPosition, OpenUri, volume, rate, shuffle, loop status, and PropertiesChanged signals with actual values
- **Keyboard shortcuts**: Configurable shortcuts with duplicate detection (Space=PlayPause, Ctrl+Right=Next, etc.)
- **Desktop notifications**: Asynchronous dispatch to avoid blocking the UI thread

### Local Play History
- **Offline scrobbling**: Every completed listen is recorded to a local SQLite journal
- **Listen statistics**: Play counts, total listening time, listening streaks
- **Top tracks and artists**: Most-played rankings based on completed listens
- **History browsing**: Full listening history with date-range filtering
- **On this day**: Revisit tracks you played on this date in previous years

### Lyrics
- **LRCLIB API**: Search for synced and unsynced lyrics
- **LRC parser**: Full LRC format support with millisecond-precision timestamps
- **Smart matching**: Score-based best result selection prioritizing synced lyrics
- **DB caching**: Lyrics cached in SQLite for offline access

## Architecture

```
TuneCraft (9-crate Cargo workspace)
├── tc-engine/    — Audio engine (decode → DSP → output)
├── tc-db/        — SQLite database with migrations
├── tc-library/   — Directory scanner + metadata extraction
├── tc-config/    — TOML-based configuration
├── tc-analysis/  — BPM detection + mood classification
├── tc-platform/  — Media keys, MPRIS, keyboard shortcuts
├── tc-lyrics/    — LRCLIB lyrics client + LRC parser
├── tc-ui/        — egui/eframe GUI, scrobbling, playback service
└── tunecraft/    — Main binary entry point
```

### Audio Pipeline

```
┌──────────┐    SPSC     ┌─────────────┐    SPSC     ┌────────────┐
│  Decode   │──Buffer──→│  DSP Thread  │──Buffer──→│   Output    │
│  Thread   │            │  Pipeline    │            │  (cpal)    │
└──────────┘            └─────────────┘            └────────────┘
                              │
          Single-track:  Preamp → Loudness → EQ → Convolution →
                        Balance → Stereo → Limiter → Volume →
                        SeekFade → Dither

          Crossfade:    [Outgoing] → Preamp → Loudness → EQ →
                        Convolution → Balance → Stereo ─┐
                                                         ├→ Mixer →
                        [Incoming] → Preamp → Loudness → ┘   Limiter →
                        EQ → Convolution → Balance →         Volume →
                        Stereo ─────────────────────┘       SeekFade → Dither
```

### Thread Architecture

```
Main Thread (GUI)          Background Thread         Audio Callback
─────────────────          ─────────────────         ──────────────
egui::App::update()        engine.tick()              cpal callback
  ├─ Poll playback info     ├─ Process commands        ├─ Read output buffer
  ├─ Send engine commands   ├─ Decode audio chunk      └─ Write to device
  ├─ Update EQ params      ├─ Run DSP pipeline
  └─ Render UI             └─ Push to output buffer
```

## Building

### Prerequisites
- Rust 1.82+ (edition 2021)
- C compiler (for cpal and SQLite bundled build)
- ALSA dev headers on Linux: `libasound2-dev`

### Build Commands

```bash
# Full build with GUI
cargo build --release

# Build without audio output (library management only)
cargo build --release --no-default-features

# Build without GUI (headless/CLI mode)
cargo build --release --no-default-features --features audio-output

# Run tests
cargo test --workspace

# Run benchmarks
cargo bench --workspace
```

## Running

```bash
# Launch GUI (default)
cargo run --release

# Run in headless/CLI mode
cargo run --release -- --headless

# Play a specific file
cargo run --release -- /path/to/song.flac
```

### Environment Variables

- `RUST_LOG`: Log level filter (default: `info`)

## Configuration

Configuration is stored at `~/.config/tunecraft/config.toml` (or platform equivalent).

Key settings:
- `engine.performance_mode`: UltraQuality, Balanced, LowPower
- `engine.eq.enabled`: Enable parametric EQ
- `engine.loudness.mode`: Off, TrackReplayGain, AlbumReplayGain, EbuR128
- `engine.crossfade.enabled`: Enable crossfade between tracks
- `engine.crossfade.duration_ms`: Crossfade duration in milliseconds
- `library.watch_dirs`: Directories to scan for music files
- `library.scan_on_startup`: Auto-scan on launch
- `scrobble.enabled`: Enable local listen recording

## License

MIT
