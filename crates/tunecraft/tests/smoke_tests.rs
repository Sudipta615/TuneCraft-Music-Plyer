//! Smoke tests for TuneCraft v3.0.0
//!
//! These integration tests exercise the critical startup, data, and shutdown
//! paths that unit tests inside individual crates cannot cover end-to-end.
//! They do not start a GUI or audio output; they test the headless service
//! layer directly.
//!
//! Run with: `cargo test --test smoke_tests`

use std::path::PathBuf;
use std::sync::Arc;

// ── helpers ──────────────────────────────────────────────────────────────────

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tunecraft_test_{}", name));
    // v3.1.3: Remove any stale state from a previous test run before
    // recreating the directory. The previous version used
    // `create_dir_all` without first removing the directory, so a
    // leftover `library.db` from a prior run would persist and pollute
    // the new run (e.g. `test_local_scrobble_record_and_read` saw
    // `total_seconds_listened = 420` instead of `210` because the
    // scrobble was inserted on top of the previous run's row).
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

// ── 1. Database opens and migrations run without error ────────────────────────

#[test]
fn test_db_opens_and_migrates() {
    let dir = temp_dir("db_migrate");
    let db_path = dir.join("library.db");
    let db = tc_db::Database::open(&db_path).expect("Database::open");
    drop(db);
    assert!(db_path.exists(), "database file should exist after open");
}

// ── 2. Config loads or defaults without panic ─────────────────────────────────

#[test]
fn test_config_loads_or_defaults() {
    // Point config at a temp dir so it does not touch the real user config.
    let dir = temp_dir("config");
    std::env::set_var("XDG_CONFIG_HOME", &dir);
    let _cfg = tc_config::ConfigPersistence::load_or_default();
    // Just verifying no panic on load.
}

// ── 3. Library scan on empty directory completes without error ────────────────

#[test]
fn test_library_scan_empty_dir() {
    let dir = temp_dir("scan_empty");
    let db_path = dir.join("library.db");
    let db = Arc::new(tc_db::Database::open(&db_path).expect("db open"));

    let library_cfg = tc_config::LibraryConfig {
        watch_dirs: vec![dir.clone()],
        ..Default::default()
    };
    let library = tc_library::LibraryManager::new(Arc::clone(&db), library_cfg);
    let result = library.scan(|_| {});
    assert!(
        result.is_ok(),
        "scan of empty dir should succeed: {:?}",
        result
    );
}

// ── 4. Library scan picks up a real audio file ────────────────────────────────

#[test]
fn test_library_scan_finds_audio_file() {
    let dir = temp_dir("scan_audio");
    // Write a minimal valid WAV header so the scanner detects it as audio.
    let wav_path = dir.join("test.wav");
    let wav_bytes: &[u8] = &[
        // RIFF header
        b'R', b'I', b'F', b'F', 36, 0, 0, 0, b'W', b'A', b'V', b'E', // fmt chunk
        b'f', b'm', b't', b' ', 16, 0, 0, 0, 1, 0, // PCM
        1, 0, // mono
        0x44, 0xAC, 0x00, 0x00, // 44100 Hz
        0x88, 0x58, 0x01, 0x00, // byte rate
        2, 0, // block align
        16, 0, // bits per sample
        // data chunk (empty)
        b'd', b'a', b't', b'a', 0, 0, 0, 0,
    ];
    std::fs::write(&wav_path, wav_bytes).expect("write wav");

    let db_path = dir.join("library.db");
    let db = Arc::new(tc_db::Database::open(&db_path).expect("db open"));
    let library_cfg = tc_config::LibraryConfig {
        watch_dirs: vec![dir.clone()],
        ..Default::default()
    };
    let library = tc_library::LibraryManager::new(Arc::clone(&db), library_cfg);
    let progress = library.scan(|_| {}).expect("scan");
    assert!(
        progress.files_found >= 1,
        "scanner should find at least the test WAV (found {})",
        progress.files_found,
    );
}

// ── 5. Local scrobble service records a listen and reads it back ─────────────

#[test]
#[allow(deprecated)] // NaiveDateTime::from_timestamp_opt is deprecated in chrono 0.4.44+
                     // but the suggested replacement (DateTime::from_timestamp)
                     // returns a different type. Test code, leave as-is.
fn test_local_scrobble_record_and_read() {
    let dir = temp_dir("scrobble_local");
    let db_path = dir.join("library.db");
    let db = Arc::new(tc_db::Database::open(&db_path).expect("db open"));

    // Insert a track so we can scrobble it.
    let track = tc_db::Track {
        id: 0,
        path: "/music/test.flac".to_string(),
        title: "Test Song".to_string(),
        artist: Some("Test Artist".to_string()),
        album: Some("Test Album".to_string()),
        album_artist: None,
        genre: None,
        year: None,
        track_number: None,
        disc_number: None,
        duration_secs: 210.0,
        sample_rate: 44100,
        channels: 2,
        bitrate_kbps: Some(320),
        format: "FLAC".to_string(),
        file_size: 1000000,
        file_modified: 0,
        crc32: None,
        replaygain_track_db: None,
        replaygain_album_db: None,
        replaygain_track_peak: None,
        replaygain_album_peak: None,
        ebu_r128_loudness: None,
        ebu_r128_peak: None,
        bpm: None,
        lyrics_synced: None,
        lyrics_unsynced: None,
        last_played: None,
        play_count: 0,
        date_added: chrono::NaiveDateTime::from_timestamp_opt(1_700_000_000, 0).unwrap(),
        date_scanned: chrono::NaiveDateTime::from_timestamp_opt(1_700_000_000, 0).unwrap(),
    };
    let track_id = db.insert_track(&track).expect("insert track");

    // Create the local scrobble service and record a listen.
    let svc = tc_ui::services::scrobble::ScrobbleService::new(db, true);
    svc.record(tc_ui::services::scrobble::LocalScrobbleEntry {
        track_id,
        artist: "Test Artist".to_string(),
        track: "Test Song".to_string(),
        duration_played_secs: 210.0,
    });

    // Verify the scrobble was recorded by checking the total listening time.
    let total = svc.total_listening_secs();
    assert!(
        (total - 210.0).abs() < 0.01,
        "total listening time should be 210s after one scrobble, got {}",
        total
    );

    // Verify the event was emitted.
    let event = svc.try_recv_event();
    assert!(
        event.is_some(),
        "scrobble service should emit a Recorded event"
    );
}

// ── 6. AppContext initialises without panicking (headless) ────────────────────

#[test]
fn test_app_context_init_headless() {
    // Redirect data/config dirs to temp to avoid touching user dirs.
    let dir = temp_dir("app_context");
    std::env::set_var("XDG_DATA_HOME", &dir);
    std::env::set_var("XDG_CONFIG_HOME", &dir);

    // AppContext::init() requires an audio device; skip if one is unavailable
    // (CI environments typically have no audio hardware).
    match tc_ui::app::AppContext::init() {
        Ok(ctx) => {
            // Basic sanity: scrobble service is created.
            assert!(!ctx.scrobble.is_enabled() || ctx.scrobble.is_available());
            drop(ctx); // exercises Drop / graceful shutdown
        },
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            // Accept known headless-environment failures.
            // "hwnd" covers the Windows souvlaki panic: "Windows media controls
            // require an HWND in MediaControlsOptions."
            assert!(
                msg.contains("audio")
                    || msg.contains("device")
                    || msg.contains("alsa")
                    || msg.contains("no available")
                    || msg.contains("thread")
                    || msg.contains("hwnd")
                    || msg.contains("media controls"),
                "unexpected AppContext init error in headless env: {}",
                e
            );
        },
    }
}
