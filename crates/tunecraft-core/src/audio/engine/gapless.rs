//! Gapless preloading methods.

use std::sync::Arc;

use super::{AudioEngine, Session};
use anyhow::Result;

impl AudioEngine {
    /// Queue the next track for gapless preloading.
    ///
    /// Call this as soon as the UI knows what track will play next (e.g. when
    /// the current track passes the 50% mark, or when the user explicitly
    /// queues a track). The next track's GStreamer pipeline is built and
    /// pre-rolled to PAUSED in the background. When EOS fires on the current
    /// track, call `play_preloaded()` to swap it in with zero silence.
    ///
    /// The preloaded session creates its own `DspEngine` to avoid data races.
    /// Settings are copied from the current engine at swap time (not at
    /// preroll time) so the latest EQ/ReplayGain/stereo width settings are
    /// applied.
    pub fn preload_next(&self, path: &std::path::Path) -> Result<()> {
        let uri = super::path_to_uri(path)?;
        self.gapless_preloader.preload(uri);
        Ok(())
    }

    /// Returns `true` if the preloaded next track is ready to play.
    pub fn next_track_ready(&self) -> bool {
        self.gapless_preloader.is_ready()
    }

    /// Cancel any in-progress preload (e.g. when the user changes the queue).
    pub fn cancel_preload(&self) {
        self.gapless_preloader.cancel();
    }

    /// Attempt to start the preloaded session. Returns `true` on success.
    ///
    /// Call this from the EOS callback instead of `load()` for true gapless
    /// playback. Falls back gracefully if no preloaded session is ready.
    ///
    /// # Fix C2 & C3: GLib decoupling
    ///
    /// The previous version of this method used `pipeline.watch_bus()` and
    /// `glib::timeout_add_local()` — both GLib-dependent APIs that break
    /// on macOS/Windows with iced (no GLib main context). The `Session`
    /// struct also had `bus_watch_id` and `position_timer_id` fields that
    /// were removed in the v3.0 migration but still referenced here.
    ///
    /// Both are removed: the `tick()` method already handles bus polling
    /// and position queries in a poll-driven fashion that doesn't require
    /// GLib. The preloaded session is swapped in cleanly without any
    /// GLib-specific setup.
    pub fn play_preloaded(&self, path: Option<&std::path::Path>) -> Result<bool> {
        if let Some(preloaded) = self.gapless_preloader.take_ready() {
            let underrun_count = preloaded.audio_output.underrun_count_arc();

            {
                let old_dsp_arc = self.dsp_arc();
                let new_dsp_arc = Arc::clone(&preloaded.dsp);
                let old_dsp = old_dsp_arc.lock().unwrap_or_else(|e| e.into_inner());
                let mut new_dsp = new_dsp_arc.lock().unwrap_or_else(|e| e.into_inner());
                new_dsp.copy_settings_from(&old_dsp);
            }

            let preloaded = std::mem::ManuallyDrop::new(preloaded);
            let p_ptr = &*preloaded as *const PreloadedSession;
            let session = Session {
                pipeline: unsafe { std::ptr::read(&(*p_ptr).pipeline) },
                _audio_output: unsafe { std::ptr::read(&(*p_ptr).audio_output) },
                dsp_stop: unsafe { std::ptr::read(&(*p_ptr).dsp_stop) },
                dsp_thread: unsafe { std::ptr::read(&(*p_ptr).dsp_thread) },
                playing: unsafe { std::ptr::read(&(*p_ptr).playing_flag) },
                is_playing: false,
                underrun_count,
            };

            {
                let mut s = self.session.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(mut old) = s.take() {
                    old.stop_and_join();
                }
                *s = Some(session);
            }

            {
                let engine_state = self
                    .loudness_state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let engine_enabled = engine_state.enabled;
                let engine_config = engine_state.config.clone();
                drop(engine_state); // Release engine lock before acquiring preloaded lock
                {
                    let mut preload_state = preloaded
                        .loudness_state
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    preload_state.enabled = engine_enabled;
                    preload_state.config = engine_config;
                }

                {
                    let mut engine_conv_guard =
                        self.convolution.lock().unwrap_or_else(|e| e.into_inner());
                    let mut preload_conv_guard = preloaded
                        .convolution
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    *preload_conv_guard = engine_conv_guard.take();
                    tracing::debug!("Gapless: convolution state transferred to preloaded session");
                }
            }

            self.swap_dsp_arc(Arc::clone(&preloaded.dsp));

            self.dsp_arc()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .mark_new_track();

            let rg = self
                .rg_state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .enabled;
            if rg {
                if let Some(p) = path {
                    if let Err(e) = self.apply_replaygain_for(p) {
                        tracing::warn!("ReplayGain (gapless): {}", e);
                    }
                }
            }

            *self
                .current_track_path
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = path.map(|p| p.to_path_buf());

            self.play()?;
            tracing::info!(
                "Gapless: swapped to preloaded track -- zero silence, zero settings gap"
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
