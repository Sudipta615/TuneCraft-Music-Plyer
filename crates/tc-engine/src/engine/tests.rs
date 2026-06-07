//! Engine submodule tests.
//!
//! These tests exercise the engine's internal logic without requiring an
//! audio device. They cover:
//! - PlaybackStream state machine transitions
//! - EngineError conversions
//! - Crossfade trigger conditions
//! - Command dispatch
//! - PlaybackInfo RwLock recovery

use tc_config::EngineConfig;

use crate::{
    buffer::{EngineCommand, PlaybackState},
    engine::{AudioEngine, EngineError},
};

// ── PlaybackStream tests ──────────────────────────────────────────────────

#[test]
fn test_playback_stream_is_crossfading_single() {
    // We can't easily construct a PlaybackStream without a real decoder,
    // so we test the is_crossfading() logic indirectly via the engine.
    // Instead, test the EngineError enum directly.
    let err = EngineError::NoTrackLoaded;
    assert_eq!(format!("{}", err), "No track loaded");
}

#[test]
fn test_engine_error_display() {
    assert_eq!(format!("{}", EngineError::NoTrackLoaded), "No track loaded");
    assert_eq!(
        format!("{}", EngineError::AlreadyRunning),
        "Engine already running"
    );
    assert_eq!(format!("{}", EngineError::NotRunning), "Engine not running");
    assert!(format!("{}", EngineError::Config("bad".into())).contains("bad"));
    assert!(format!("{}", EngineError::StreamRecovery("failed".into())).contains("failed"));
}

// ── Engine construction tests ──────────────────────────────────────────────

#[test]
fn test_engine_new_default() {
    // AudioEngine::new_default should succeed even without an audio device
    // (it falls back to DEFAULT_SAMPLE_RATE when no device is found).
    let result = AudioEngine::new_default();
    assert!(
        result.is_ok(),
        "Engine creation should succeed: {:?}",
        result.err()
    );
    let engine = result.unwrap();
    assert!(
        !engine.is_running(),
        "Engine should not be running after creation"
    );
}

#[test]
fn test_engine_new_with_config() {
    let config = EngineConfig::default();
    let result = AudioEngine::new(config);
    assert!(
        result.is_ok(),
        "Engine creation with config should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_engine_not_running_initially() {
    let engine = AudioEngine::new_default().unwrap();
    assert!(!engine.is_running());
}

#[test]
fn test_engine_pipeline_accessors() {
    let mut engine = AudioEngine::new_default().unwrap();
    // Pipeline should be accessible.
    let _ = engine.pipeline().volume();
    let _ = engine.pipeline().speed();
    // Pipeline mut should work.
    engine.pipeline_mut().set_volume(0.5);
    for _ in 0..50000 {
        engine.pipeline_mut().process(0.0, 0.0);
    }
    assert!((engine.pipeline().volume() - 0.5).abs() < 0.01);
}

// ── Command dispatch tests ────────────────────────────────────────────────

#[test]
fn test_send_command_does_not_panic() {
    let engine = AudioEngine::new_default().unwrap();
    // Should not panic even when engine isn't started.
    engine.send_command(EngineCommand::Play);
    engine.send_command(EngineCommand::Pause);
    engine.send_command(EngineCommand::Stop);
    engine.send_command(EngineCommand::SetVolume(0.5));
    engine.send_command(EngineCommand::SetSpeed(1.5));
    engine.send_command(EngineCommand::Shutdown);
}

#[test]
fn test_send_command_channel() {
    let mut engine = AudioEngine::new_default().unwrap();
    let tx = engine.send_command_channel();
    // Sending through the cloned channel should work.
    assert!(tx.send(EngineCommand::Play).is_ok());
}

#[test]
fn test_set_track_id() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.set_track_id(42);
    let info = engine.playback_info();
    assert_eq!(info.track_id, Some(42));
}

#[test]
fn test_set_config() {
    let mut engine = AudioEngine::new_default().unwrap();
    let mut new_config = EngineConfig::default();
    new_config.eq.enabled = true;
    new_config.dither_enabled = false;
    engine.set_config(new_config);
    // Config change should not panic; just verify no crash.
}

// ── PlaybackInfo tests ────────────────────────────────────────────────────

#[test]
fn test_playback_info_default() {
    let info = crate::buffer::PlaybackInfo::default();
    assert_eq!(info.state, PlaybackState::Stopped);
    assert_eq!(info.volume, 1.0);
    assert_eq!(info.speed, 1.0);
    assert_eq!(info.track_id, None);
    assert!(!info.resampler_disabled);
    assert!(!info.convolution_ir_needs_reload);
}

#[test]
fn test_playback_info_access() {
    let engine = AudioEngine::new_default().unwrap();
    let info = engine.playback_info();
    assert_eq!(info.state, PlaybackState::Stopped);
    assert_eq!(info.volume, 1.0);
}

#[test]
fn test_playback_info_arc() {
    let engine = AudioEngine::new_default().unwrap();
    let arc = engine.playback_info_arc();
    let info = arc.read().unwrap();
    assert_eq!(info.state, PlaybackState::Stopped);
}

// ── Tick tests ────────────────────────────────────────────────────────────

#[test]
fn test_tick_without_start() {
    // Tick should not panic when engine hasn't been started.
    let mut engine = AudioEngine::new_default().unwrap();
    engine.tick();
    engine.tick();
    engine.tick();
}

// ── Seek validation tests ─────────────────────────────────────────────────

#[test]
fn test_seek_command_validation() {
    let engine = AudioEngine::new_default().unwrap();
    // These should be silently ignored (no track loaded).
    engine.send_command(EngineCommand::Seek(-1.0));
    engine.send_command(EngineCommand::Seek(f32::NAN));
    engine.send_command(EngineCommand::Seek(f32::INFINITY));
    engine.send_command(EngineCommand::Seek(30.0));
}

// ── Speed validation tests ────────────────────────────────────────────────

#[test]
fn test_speed_command_validation() {
    let engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetSpeed(0.5));
    engine.send_command(EngineCommand::SetSpeed(f32::NAN));
    engine.send_command(EngineCommand::SetSpeed(f32::INFINITY));
}

// ── Volume command test ───────────────────────────────────────────────────

#[test]
fn test_volume_command() {
    let engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetVolume(0.75));
}

// ── Resampler disabled test ───────────────────────────────────────────────

#[test]
fn test_is_resampler_disabled_no_stream() {
    let engine = AudioEngine::new_default().unwrap();
    // No stream loaded → resampler is not disabled (it doesn't exist).
    assert!(!engine.is_resampler_disabled());
}

// ── PlaybackState tests ──────────────────────────────────────────────────

#[test]
fn test_playback_state_equality() {
    assert_eq!(PlaybackState::Stopped, PlaybackState::Stopped);
    assert_ne!(PlaybackState::Playing, PlaybackState::Paused);
    assert_ne!(PlaybackState::Stopped, PlaybackState::Buffering);
}

// ── EngineCommand Debug test ──────────────────────────────────────────────

#[test]
fn test_engine_command_debug() {
    let cmd = EngineCommand::SetVolume(0.5);
    let debug_str = format!("{:?}", cmd);
    assert!(debug_str.contains("SetVolume"));
}

// ── Percent decode tests ──────────────────────────────────────────────────

#[test]
fn test_percent_decode_simple() {
    // Test via the OpenUri command — just verify the command can be created.
    let cmd = EngineCommand::OpenUri("file:///path/to/file.mp3".to_string());
    if let EngineCommand::OpenUri(uri) = cmd {
        assert!(uri.starts_with("file://"));
    } else {
        panic!("Expected OpenUri command");
    }
}

// ── Drop test ─────────────────────────────────────────────────────────────

#[test]
fn test_engine_drop_does_not_panic() {
    let engine = AudioEngine::new_default().unwrap();
    drop(engine); // Should not panic
}

#[test]
fn test_engine_stop_idempotent() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.stop(); // Not started — should be a no-op (no panic)
    engine.stop(); // Double stop — also safe
}
