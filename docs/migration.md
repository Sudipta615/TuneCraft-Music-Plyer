# TuneCraft Migration Guide

---

## v4.x → v5.0

v5.0 replaces the iced v0.14 frontend with [Dioxus v0.7](https://dioxuslabs.com/) (desktop).
The core audio engine, database, and configuration are unchanged — only the UI layer was rewritten.

### Configuration

No changes to `tunecraft.toml`. Your existing config file is fully compatible with v5.0.
On first launch, TuneCraft will continue reading it from the same location as before:

| Platform | Path |
|----------|------|
| Linux    | `~/.config/TuneCraft/tunecraft.toml` |
| macOS    | `~/Library/Application Support/com.tunecraft.Tunecraft/tunecraft.toml` |
| Windows  | `%APPDATA%\com.tunecraft.Tunecraft\config\tunecraft.toml` |

### Library database

Fully compatible. No migration or re-import required.

### UI differences

| Area | v4.x (iced) | v5.0 (Dioxus) |
|------|------------|---------------|
| Theming | iced style structs | CSS with `light` / `dark` classes |
| Font loading | Embedded binary fonts (Font Awesome, Inter) | CSS web fonts — no binary embeds |
| Rendering | tiny-skia software fallback | Dioxus desktop (WebView) |
| Component model | Elm-style `Model / Message / update / view` | React-like components with hooks and signals |

### Removed dependencies

The following crates are no longer in the dependency tree and do not need to be installed:

- `iced` (all feature flags)
- `tiny-skia`

### Linux packaging

The Flatpak manifest and AppImage build scripts are unchanged. The Meson build system
version is now `5.0` — if you maintain a downstream package, update the version string.

### Keyboard shortcuts

Unchanged from v4.x. All shortcuts listed in the README remain the same.

---

## v3.0 → v4.x

The v3.0 → v4.x migration is the largest single migration in TuneCraft's history. It covers the
transition from the GTK4 UI framework to [iced](https://iced.rs/) v0.14, making TuneCraft
cross-platform (Linux, macOS, Windows), and a number of config and database changes.

### UI framework transition: GTK4 → iced

| Area | v3.0 (GTK4) | v4.x (iced) |
|------|------------|-------------|
| Framework | GTK4 / libadwaita | iced v0.14 |
| Rendering | GPU via GTK / Cairo | tiny-skia software fallback |
| Component model | GTK widgets + GObject signals | Elm Architecture (`Model / Message / update / view`) |
| Font loading | Pango / system fonts | Embedded binary fonts (Font Awesome, Inter) |
| Main loop | GLib main context (`glib::MainLoop`) | iced event loop (`iced::Daemon` / `iced::Subscription`) |
| Windowing | GDK / GTK Window | iced window (winit backend) |
| Theming | GTK CSS / libadwaita dark mode | iced style structs (`iced::widget::Style`) |

The GTK4 → iced migration required rewriting the entire UI layer. The core audio engine,
database, and configuration are unchanged — only the UI layer was rewritten. Any custom
GTK4 CSS or libadwaita themes will not work with iced.

### New dependency: iced v0.14

The `tunecraft-ui` crate now depends on `iced` v0.14 with the following features:

```toml
[dependencies]
iced = { version = "0.14", features = ["tokio", "image", "svg", "advanced"] }
```

The `tiny-skia` crate is also pulled in as a transitive dependency for software rendering.
No GPU drivers are required for the iced UI — this is a significant advantage over GTK4
on headless or remote systems.

### Cross-platform support

v3.0 was Linux-only (required GTK4, GLib, and libadwaita). v4.x runs on:

| Platform | Requirements |
|----------|-------------|
| Linux | GStreamer 1.x, ALSA/PulseAudio, iced (X11 or Wayland) |
| macOS | GStreamer 1.x (Homebrew or .pkg), CoreAudio |
| Windows | GStreamer 1.x MSVC runtime, WASAPI |

The `glib::timeout_add_local` and `glib::SourceId` dependencies were removed from the
audio engine. Position updates and bus message handling are now poll-driven via the
engine's `tick()` method, called periodically by iced's `Subscription` system.

### Configuration format changes

No changes to the `tunecraft.toml` format itself. However, the **config file location** has
changed on macOS and Windows to follow platform conventions:

| Platform | v3.0 path | v4.x path |
|----------|-----------|-----------|
| Linux | `~/.config/TuneCraft/tunecraft.toml` | `~/.config/TuneCraft/tunecraft.toml` (unchanged) |
| macOS | `~/.config/TuneCraft/tunecraft.toml` | `~/Library/Application Support/com.tunecraft.Tunecraft/tunecraft.toml` |
| Windows | N/A (Linux-only) | `%APPDATA%\com.tunecraft.Tunecraft\config\tunecraft.toml` |

On macOS, TuneCraft will automatically migrate the config file from the v3.0 location to
the v4.x location on first launch. The old file is not deleted — it remains as a backup.

New audio config keys added in v4.x (also backported to v3.0 for forward compatibility):

| Key | Default | Description |
|-----|---------|-------------|
| `decode_ring_size` | `65536` | Decode ring buffer size (f32 samples). Range: 4096–262144. |
| `output_ring_size` | `32768` | Output ring buffer size (f32 samples). Range: 2048–131072. |
| `visualization_mode` | `"deferred"` | `"always"`, `"deferred"`, or `"disabled"`. |

### Database schema additions

No schema-breaking changes. The following tables/columns were added:

| Version | Change | Description |
|---------|--------|-------------|
| v4.1 | `eq_presets` table | Equalizer preset storage (was in-memory only in v3.0) |
| v4.1 | `user_prefs` table | Key-value preferences store (credential encryption, UI state) |
| v4.1 | `waveforms` table | Waveform peak data cache for the waveform visualizer |
| v4.1 | Credential encryption | Last.fm API keys/session keys in `user_prefs` are now AES-256-GCM encrypted |

The database is fully backward-compatible — a v3.0 database works without migration in v4.x.
The new tables are created on first launch if they don't exist.

### Keyboard shortcut changes

The move from GTK4 to iced required re-implementing all keyboard shortcuts. Most shortcuts
are unchanged, but some GTK4-specific combinations were remapped:

| Action | v3.0 (GTK4) | v4.x (iced) |
|--------|------------|-------------|
| Play/Pause | `Space` | `Space` (unchanged) |
| Next track | `Ctrl+Right` | `Ctrl+Right` / `MediaNext` |
| Previous track | `Ctrl+Left` | `Ctrl+Left` / `MediaPrev` |
| Volume up | `Ctrl+Up` | `Ctrl+Up` / `MediaVolUp` |
| Volume down | `Ctrl+Down` | `Ctrl+Down` / `MediaVolDown` |
| Toggle EQ | `Ctrl+E` | `Ctrl+E` (unchanged) |
| Search | `Ctrl+F` | `Ctrl+F` (unchanged) |
| Preferences | `Ctrl+P` (GTK4 dialog) | `Ctrl+,` (iced settings panel) |
| Quit | `Ctrl+Q` | `Ctrl+Q` (unchanged) |

Media key support (`MediaNext`, `MediaPrev`, `MediaPlayPause`, `MediaVolUp`, `MediaVolDown`)
is new in v4.x — these were not available in the GTK4 build due to libadwaita limitations.

### Resampler change (v4.1)

The `samplerate` crate was replaced with `rubato`. Audio quality is equivalent;
this change only affects build dependencies.

---

## v2.1 → v3.0

The v2.1 → v3.0 migration covered the initial Linux-only release with GTK4.
If you are upgrading from v2.1 directly to v5.0, the v3.0 → v4.x section above
covers the GTK4 → iced transition, and the v4.x → v5.0 section covers the
iced → Dioxus transition. Your database and library are fully compatible.
