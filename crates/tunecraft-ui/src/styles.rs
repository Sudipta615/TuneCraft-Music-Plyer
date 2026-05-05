//! TuneCraft v5.0 — Complete CSS stylesheet for the Dioxus UI.
//!
//! This stylesheet replicates the visual design from the iced UI,
//! supporting both light and dark themes with the purple accent color scheme.

pub const TUNECRAFT_CSS: &str = r#"
/* ═══════════════════════════════════════════════════════════════
   TuneCraft v5.0 — CSS Stylesheet
   ═══════════════════════════════════════════════════════════════ */

/* ── CSS Variables (Light Theme) ────────────────────────────── */
:root {
    --bg-primary: #ffffff;
    --bg-secondary: #f9fafb;
    --bg-tertiary: #f3f4f6;
    --text-primary: #1f2937;
    --text-secondary: #6b7280;
    --text-tertiary: #9ca3af;
    --accent: #6366f1;
    --accent-hover: #4f46e5;
    --accent-light: #e0e7ff;
    --border: #e5e7eb;
    --border-light: #f3f4f6;
    --shadow: rgba(0, 0, 0, 0.08);
    --shadow-lg: rgba(0, 0, 0, 0.12);
    --sidebar-width: 240px;
    --sidebar-collapsed-width: 52px;
    --topbar-height: 56px;
    --playback-bar-height: 80px;
    --panel-bg: #ffffff;
    --hover-bg: #f3f4f6;
    --active-bg: #e0e7ff;
    --scrollbar-track: #f3f4f6;
    --scrollbar-thumb: #d1d5db;
}

/* ── Dark Theme ─────────────────────────────────────────────── */
.dark {
    --bg-primary: #0f172a;
    --bg-secondary: #1e293b;
    --bg-tertiary: #334155;
    --text-primary: #f1f5f9;
    --text-secondary: #94a3b8;
    --text-tertiary: #64748b;
    --accent: #818cf8;
    --accent-hover: #6366f1;
    --accent-light: #312e81;
    --border: #334155;
    --border-light: #1e293b;
    --shadow: rgba(0, 0, 0, 0.3);
    --shadow-lg: rgba(0, 0, 0, 0.5);
    --panel-bg: #1e293b;
    --hover-bg: #334155;
    --active-bg: #312e81;
    --scrollbar-track: #1e293b;
    --scrollbar-thumb: #475569;
}

/* ── Reset & Base ───────────────────────────────────────────── */
*, *::before, *::after {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

html, body {
    height: 100%;
    overflow: hidden;
    font-family: 'Inter', 'Segoe UI', system-ui, -apple-system, sans-serif;
    font-size: 14px;
    color: var(--text-primary);
    background: var(--bg-primary);
    -webkit-font-smoothing: antialiased;
}

/* ── App Container ──────────────────────────────────────────── */
.app-container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
    background: var(--bg-primary);
    overflow: hidden;
    position: relative;
}

/* ── Main Layout ────────────────────────────────────────────── */
.main-layout {
    display: flex;
    flex: 1;
    overflow: hidden;
}

/* ── Sidebar ────────────────────────────────────────────────── */
.sidebar {
    width: var(--sidebar-width);
    min-width: var(--sidebar-width);
    background: var(--bg-secondary);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    overflow-x: hidden;
    transition: width 0.2s ease, min-width 0.2s ease;
}

.sidebar.collapsed {
    width: var(--sidebar-collapsed-width);
    min-width: var(--sidebar-collapsed-width);
}

.sidebar-logo {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px;
    border-bottom: 1px solid var(--border);
}

.logo-text {
    font-size: 18px;
    font-weight: 700;
    color: var(--accent);
    letter-spacing: -0.5px;
}

.sidebar-toggle-btn {
    background: none;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 16px;
    padding: 4px 8px;
    border-radius: 4px;
}

.sidebar-toggle-btn:hover {
    background: var(--hover-bg);
    color: var(--text-primary);
}

.sidebar-section {
    padding: 8px 0;
}

.sidebar-section-header {
    padding: 8px 16px 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-tertiary);
    letter-spacing: 0.5px;
    text-transform: uppercase;
}

.sidebar-item {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 8px 16px;
    background: none;
    border: none;
    color: var(--text-secondary);
    font-size: 14px;
    cursor: pointer;
    text-align: left;
    gap: 10px;
    transition: background 0.15s, color 0.15s;
}

.sidebar-item:hover {
    background: var(--hover-bg);
    color: var(--text-primary);
}

.sidebar-item.active {
    background: var(--accent-light);
    color: var(--accent);
}

.sidebar-item-icon {
    width: 20px;
    text-align: center;
    font-size: 14px;
}

.sidebar-item-text {
    flex: 1;
}

.sidebar-badge {
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-light);
    padding: 2px 8px;
    border-radius: 10px;
}

.mood-badge {
    font-size: 11px;
    font-weight: 600;
    color: white;
    padding: 2px 8px;
    border-radius: 10px;
    min-width: 24px;
    text-align: center;
}

.sidebar-bottom {
    margin-top: auto;
    border-top: 1px solid var(--border);
    padding-top: 4px;
}

/* ── Content Area ───────────────────────────────────────────── */
.content-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--bg-primary);
}

/* ── Top Bar ────────────────────────────────────────────────── */
.topbar {
    display: flex;
    align-items: center;
    padding: 0 20px;
    height: var(--topbar-height);
    background: var(--bg-primary);
    border-bottom: 1px solid var(--border);
    gap: 8px;
}

.topbar.light {
    background: var(--bg-primary);
}

.topbar.dark {
    background: var(--bg-primary);
}

.search-bar {
    display: flex;
    align-items: center;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0 14px;
    max-width: 500px;
    width: 100%;
    gap: 8px;
}

.search-icon {
    color: var(--text-tertiary);
    font-size: 14px;
}

.search-input {
    background: none;
    border: none;
    outline: none;
    color: var(--text-primary);
    font-size: 13px;
    padding: 8px 0;
    width: 100%;
    font-family: inherit;
}

.search-input::placeholder {
    color: var(--text-tertiary);
}

.topbar-spacer {
    flex: 1;
}

.topbar-icon-btn {
    position: relative;
    background: none;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 18px;
    padding: 6px 10px;
    border-radius: 6px;
    transition: background 0.15s;
}

.topbar-icon-btn:hover {
    background: var(--hover-bg);
}

.notification-badge {
    position: absolute;
    top: 2px;
    right: 2px;
    background: var(--accent);
    color: white;
    font-size: 9px;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 8px;
    min-width: 14px;
    text-align: center;
}

.add-music-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--accent);
    color: white;
    border: none;
    padding: 10px 20px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.15s;
}

.add-music-btn:hover {
    background: var(--accent-hover);
}

/* ── Track List ─────────────────────────────────────────────── */
.track-list-container {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--bg-primary);
}

.track-list-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 24px 12px;
}

.track-list-header-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
}

.track-list-title {
    font-size: 22px;
    font-weight: 700;
    color: var(--text-primary);
}

.track-list-subtitle {
    font-size: 13px;
    color: var(--text-secondary);
}

.track-list-header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
}

.toolbar-btn {
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    color: var(--text-secondary);
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.15s, color 0.15s;
}

.toolbar-btn:hover {
    background: var(--hover-bg);
    color: var(--text-primary);
}

.eq-btn {
    background: var(--accent-light);
    color: var(--accent);
    border-color: var(--accent);
    font-weight: 600;
}

.eq-btn:hover {
    background: var(--accent);
    color: white;
}

/* ── Track Table ────────────────────────────────────────────── */
.track-table {
    flex: 1;
    overflow-y: auto;
    padding: 0 24px;
}

.track-table::-webkit-scrollbar {
    width: 6px;
}

.track-table::-webkit-scrollbar-track {
    background: var(--scrollbar-track);
}

.track-table::-webkit-scrollbar-thumb {
    background: var(--scrollbar-thumb);
    border-radius: 3px;
}

.track-table-header {
    display: flex;
    align-items: center;
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-tertiary);
    letter-spacing: 0.5px;
    text-transform: uppercase;
}

.col-num { width: 40px; text-align: center; }
.col-title { flex: 3; }
.col-album { flex: 2; }
.col-duration { width: 60px; text-align: center; }
.col-mood { width: 100px; text-align: center; }
.col-actions { width: 80px; text-align: right; }

.track-row {
    display: flex;
    align-items: center;
    padding: 8px 0;
    border-bottom: 1px solid var(--border-light);
    transition: background 0.15s;
}

.track-row:hover {
    background: var(--hover-bg);
}

.track-play-btn {
    background: var(--accent);
    color: white;
    border: none;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    cursor: pointer;
    font-size: 11px;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity 0.15s;
}

.track-row:hover .track-play-btn {
    opacity: 1;
}

.track-title {
    font-size: 14px;
    font-weight: 500;
    color: var(--text-primary);
}

.track-artist {
    font-size: 12px;
    color: var(--text-secondary);
}

.mood-tag {
    display: inline-block;
    font-size: 11px;
    font-weight: 600;
    color: white;
    padding: 2px 10px;
    border-radius: 10px;
}

.love-btn {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 16px;
    color: var(--text-tertiary);
    padding: 2px 4px;
}

.love-btn.loved {
    color: #ef4444;
}

.love-btn:hover {
    color: #ef4444;
}

.more-btn {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 16px;
    color: var(--text-tertiary);
    padding: 2px 4px;
    border-radius: 4px;
}

.more-btn:hover {
    background: var(--hover-bg);
    color: var(--text-primary);
}

/* ── Playback Bar ───────────────────────────────────────────── */
.playback-bar {
    display: flex;
    align-items: center;
    height: var(--playback-bar-height);
    padding: 0 20px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border);
    gap: 20px;
}

.pb-track-info {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 250px;
    min-width: 200px;
}

.pb-track-art {
    width: 48px;
    height: 48px;
    background: var(--accent-light);
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    color: var(--accent);
}

.pb-track-text {
    flex: 1;
    overflow: hidden;
}

.pb-track-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.pb-track-artist {
    font-size: 12px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.pb-love-btn {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 18px;
    color: var(--text-tertiary);
}

.pb-love-btn.loved {
    color: #ef4444;
}

.pb-controls {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
}

.pb-main-controls {
    display: flex;
    align-items: center;
    gap: 12px;
}

.pb-ctrl-btn {
    background: none;
    border: none;
    color: var(--text-secondary);
    font-size: 18px;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 4px;
}

.pb-ctrl-btn:hover {
    color: var(--text-primary);
}

.pb-ctrl-btn.active {
    color: var(--accent);
}

.pb-play-btn {
    background: var(--accent);
    color: white;
    border: none;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    cursor: pointer;
    font-size: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s;
}

.pb-play-btn:hover {
    background: var(--accent-hover);
}

.pb-progress {
    display: flex;
    align-items: center;
    width: 100%;
    gap: 8px;
}

.pb-time {
    font-size: 12px;
    color: var(--text-secondary);
    min-width: 36px;
    text-align: center;
}

.pb-progress-bar {
    flex: 1;
    height: 4px;
    background: var(--bg-tertiary);
    border-radius: 2px;
    position: relative;
    cursor: pointer;
}

.pb-progress-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 2px;
    transition: width 0.1s linear;
}

.pb-progress-slider {
    position: absolute;
    top: -8px;
    left: 0;
    width: 100%;
    height: 20px;
    opacity: 0;
    cursor: pointer;
}

.pb-right {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 200px;
    justify-content: flex-end;
}

.pb-volume-slider {
    width: 80px;
    accent-color: var(--accent);
}

/* ── Overlay Panels ─────────────────────────────────────────── */
.overlay-panel {
    position: absolute;
    top: 60px;
    right: 20px;
    background: var(--panel-bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 8px 32px var(--shadow-lg);
    z-index: 1000;
    padding: 20px;
    max-height: calc(100vh - 160px);
    overflow-y: auto;
}

/* ── EQ Panel ───────────────────────────────────────────────── */
.eq-panel {
    width: 600px;
}

.eq-header {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 20px;
}

.eq-title {
    font-size: 20px;
    font-weight: 700;
    color: var(--text-primary);
}

.eq-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-secondary);
    cursor: pointer;
}

.eq-toggle input[type="checkbox"] {
    accent-color: var(--accent);
    width: 36px;
    height: 20px;
}

.eq-preset-select {
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    color: var(--accent);
    padding: 4px 12px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
}

.eq-close-btn {
    margin-left: auto;
    background: none;
    border: none;
    color: var(--text-tertiary);
    font-size: 18px;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 4px;
}

.eq-close-btn:hover {
    background: var(--hover-bg);
}

.eq-bands {
    display: flex;
    gap: 4px;
    margin-bottom: 20px;
    align-items: stretch;
}

.eq-db-scale {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    font-size: 10px;
    color: var(--text-tertiary);
    padding: 0 8px 24px 0;
    text-align: right;
    min-width: 40px;
}

.eq-sliders-container {
    display: flex;
    flex: 1;
    gap: 8px;
}

.eq-band {
    display: flex;
    flex-direction: column;
    align-items: center;
    flex: 1;
}

.eq-band-value {
    font-size: 10px;
    color: var(--text-secondary);
    margin-bottom: 4px;
}

.eq-band-slider.vertical {
    writing-mode: vertical-lr;
    direction: rtl;
    height: 160px;
    width: 20px;
    accent-color: var(--accent);
}

.eq-band-label {
    font-size: 11px;
    color: var(--text-secondary);
    margin-top: 8px;
    white-space: nowrap;
}

.eq-secondary {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    margin-bottom: 20px;
    padding: 16px;
    background: var(--bg-tertiary);
    border-radius: 8px;
}

.eq-secondary-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    min-width: 100px;
}

.eq-secondary-item label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
}

.eq-secondary-item small {
    font-size: 10px;
    color: var(--text-tertiary);
}

.eq-secondary-item input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
}

.eq-secondary-item span {
    font-size: 11px;
    color: var(--text-secondary);
}

.eq-toggles {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-left: auto;
    justify-content: center;
}

.eq-footer {
    display: flex;
    align-items: center;
    gap: 16px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
}

.eq-preamp {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
}

.eq-preamp label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
}

.eq-preamp input[type="range"] {
    flex: 1;
    accent-color: var(--accent);
}

.eq-preamp span {
    font-size: 11px;
    color: var(--text-secondary);
    min-width: 50px;
}

.eq-reset-btn {
    background: none;
    border: 1px solid var(--border);
    color: var(--accent);
    padding: 8px 16px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.15s;
}

.eq-reset-btn:hover {
    background: var(--accent-light);
}

/* ── Filter Panel ───────────────────────────────────────────── */
.filter-panel {
    width: 400px;
}

.panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
}

.panel-header h3 {
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary);
}

.panel-header-actions {
    display: flex;
    gap: 8px;
}

.panel-close-btn {
    background: none;
    border: none;
    color: var(--text-tertiary);
    font-size: 16px;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 4px;
}

.panel-close-btn:hover {
    background: var(--hover-bg);
    color: var(--text-primary);
}

.panel-action-btn {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-secondary);
    padding: 4px 12px;
    border-radius: 6px;
    font-size: 12px;
    cursor: pointer;
    font-family: inherit;
}

.panel-action-btn:hover {
    background: var(--hover-bg);
}

.filter-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
}

.filter-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
}

.filter-field label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
}

.filter-input {
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    color: var(--text-primary);
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 14px;
    font-family: inherit;
    outline: none;
}

.filter-input:focus {
    border-color: var(--accent);
}

.filter-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
}

.filter-apply-btn {
    background: var(--accent);
    color: white;
    border: none;
    padding: 8px 20px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
}

.filter-apply-btn:hover {
    background: var(--accent-hover);
}

.filter-clear-btn {
    background: none;
    border: 1px solid var(--border);
    color: var(--text-secondary);
    padding: 8px 20px;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
    font-family: inherit;
}

.filter-clear-btn:hover {
    background: var(--hover-bg);
}

/* ── Queue Panel ────────────────────────────────────────────── */
.queue-panel {
    width: 400px;
}

.queue-list {
    max-height: 400px;
    overflow-y: auto;
}

.queue-item {
    display: flex;
    align-items: center;
    padding: 8px 4px;
    border-bottom: 1px solid var(--border-light);
    gap: 10px;
}

.queue-item.current {
    background: var(--accent-light);
    border-radius: 6px;
}

.queue-item-num {
    width: 30px;
    text-align: center;
    font-size: 12px;
    color: var(--text-tertiary);
}

.queue-item.current .queue-item-num {
    color: var(--accent);
}

.queue-item-info {
    flex: 1;
    overflow: hidden;
}

.queue-item-title {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.queue-item-artist {
    font-size: 11px;
    color: var(--text-secondary);
}

.queue-item-duration {
    font-size: 12px;
    color: var(--text-tertiary);
}

.queue-item-remove {
    background: none;
    border: none;
    color: var(--text-tertiary);
    cursor: pointer;
    font-size: 14px;
    padding: 2px 6px;
    border-radius: 4px;
}

.queue-item-remove:hover {
    background: var(--hover-bg);
    color: #ef4444;
}

/* ── Notifications Panel ────────────────────────────────────── */
.notifications-panel {
    width: 400px;
}

.notifications-list {
    max-height: 400px;
    overflow-y: auto;
}

.notifications-empty {
    text-align: center;
    padding: 24px;
    color: var(--text-tertiary);
    font-size: 14px;
}

.notification-item {
    padding: 10px 4px;
    border-bottom: 1px solid var(--border-light);
}

.notification-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
}

.notification-body {
    font-size: 12px;
    color: var(--text-secondary);
}

.notification-time {
    font-size: 11px;
    color: var(--text-tertiary);
    margin-top: 2px;
}

/* ── Context Menu ───────────────────────────────────────────── */
.context-menu-overlay {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    z-index: 2000;
    background: transparent;
}

.context-menu {
    position: fixed;
    /* Position is set dynamically via inline styles from cursor coordinates */
    background: var(--panel-bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 4px 16px var(--shadow-lg);
    padding: 4px;
    min-width: 200px;
    z-index: 2001;
}

.context-menu-item {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    color: var(--text-primary);
    padding: 8px 16px;
    font-size: 13px;
    cursor: pointer;
    border-radius: 4px;
    font-family: inherit;
}

.context-menu-item:hover {
    background: var(--hover-bg);
}

/* ── Scrollbar Styling ──────────────────────────────────────── */
::-webkit-scrollbar {
    width: 6px;
}

::-webkit-scrollbar-track {
    background: var(--scrollbar-track);
}

::-webkit-scrollbar-thumb {
    background: var(--scrollbar-thumb);
    border-radius: 3px;
}

::-webkit-scrollbar-thumb:hover {
    background: var(--text-tertiary);
}

/* ── Range Slider Styling ───────────────────────────────────── */
input[type="range"] {
    -webkit-appearance: none;
    appearance: none;
    height: 4px;
    background: var(--bg-tertiary);
    border-radius: 2px;
    outline: none;
}

input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    cursor: pointer;
    border: 2px solid white;
    box-shadow: 0 1px 3px var(--shadow);
}

input[type="range"]::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    cursor: pointer;
    border: 2px solid white;
    box-shadow: 0 1px 3px var(--shadow);
}

/* ── Track Grid (Issue #10) ────────────────────────────────── */
.track-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 16px;
    padding: 0 24px 24px;
}

/* ── Album Card (Issue #10) ────────────────────────────────── */
.album-card {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 16px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, box-shadow 0.15s;
}

.album-card:hover {
    background: var(--hover-bg);
    border-color: var(--accent);
    box-shadow: 0 2px 12px var(--shadow);
}

.album-card:focus {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
}

.album-card-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-bottom: 4px;
}

.album-card-artist {
    font-size: 12px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-bottom: 4px;
}

.album-card-meta {
    font-size: 11px;
    color: var(--text-tertiary);
}

/* ── Artist Card (Issue #10) ───────────────────────────────── */
.artist-card {
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 16px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, box-shadow 0.15s;
}

.artist-card:hover {
    background: var(--hover-bg);
    border-color: var(--accent);
    box-shadow: 0 2px 12px var(--shadow);
}

.artist-card:focus {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
}

.artist-card-name {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.artist-card-meta {
    font-size: 11px;
    color: var(--text-tertiary);
}

/* ── Settings Placeholder (Issue #10) ─────────────────────── */
.settings-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    padding: 60px 20px;
    text-align: center;
    color: var(--text-tertiary);
    gap: 8px;
}

.settings-placeholder p {
    font-size: 15px;
    max-width: 400px;
}

/* ── Settings Panel (Issue #27) ──────────────────────────── */
.settings-panel {
    flex: 1;
    overflow-y: auto;
    padding: 0 24px 24px;
    max-width: 700px;
}

.settings-section {
    margin-bottom: 28px;
    padding: 20px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 10px;
}

.settings-section-title {
    font-size: 16px;
    font-weight: 700;
    color: var(--text-primary);
    margin-bottom: 16px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
}

.settings-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0;
    gap: 16px;
}

.settings-row + .settings-row {
    border-top: 1px solid var(--border-light);
}

.settings-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    min-width: 140px;
}

.settings-control {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1;
    justify-content: flex-end;
}

.settings-slider {
    width: 160px;
    accent-color: var(--accent);
}

.settings-slider:disabled {
    opacity: 0.4;
}

.settings-value {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
    min-width: 40px;
    text-align: right;
}

.settings-dirs {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
}

.settings-path {
    font-size: 11px;
    font-family: monospace;
    color: var(--text-tertiary);
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.settings-checkbox-label {
    font-size: 12px;
    color: var(--text-secondary);
    min-width: 50px;
}

.settings-btn {
    background: var(--accent);
    color: white;
    border: none;
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    font-family: inherit;
    white-space: nowrap;
    transition: background 0.15s;
}

.settings-btn:hover {
    background: var(--accent-hover);
}

/* ── Context Menu Playlist Picker (Issue #10) ─────────────── */
.context-menu-playlist-picker {
    border-top: 1px solid var(--border);
    margin-top: 4px;
    padding-top: 4px;
}

.context-menu-picker-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    padding: 4px 16px 2px;
}

.context-menu-playlist-choice {
    padding-left: 24px;
    font-style: italic;
}

/* ── Loading Overlay (Issue #19) ──────────────────────────── */
.loading-overlay {
    position: fixed;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background: var(--bg-primary);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    z-index: 9999;
    gap: 16px;
}

.loading-spinner {
    width: 40px;
    height: 40px;
    border: 3px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
}

@keyframes spin {
    to { transform: rotate(360deg); }
}

.loading-text {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-secondary);
}

/* ── Firefox Scrollbar Styles (Issue #30) ─────────────────── */
* {
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-track);
}

.sidebar {
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-track);
}

.track-table {
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-track);
}

.queue-list {
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-track);
}

.notifications-list {
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-track);
}

.overlay-panel {
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) var(--scrollbar-track);
}
"#;
