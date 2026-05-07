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

            // Fix Bug #10: Copy the old DspEngine's settings into the
            // preloaded DspEngine before the session swap. The preloaded
            // DspEngine starts with flat EQ, volume=1.0, and no ReplayGain
            // — copying settings ensures no audible gap in DSP processing.
            {
                let old_dsp_arc = self.dsp_arc();
                let new_dsp_arc = Arc::clone(&preloaded.dsp);
                let old_dsp = old_dsp_arc.lock().unwrap_or_else(|e| e.into_inner());
                let mut new_dsp = new_dsp_arc.lock().unwrap_or_else(|e| e.into_inner());
                new_dsp.copy_settings_from(&old_dsp);
            }

            // Fix C2: Session no longer has bus_watch_id or position_timer_id
            // fields (those were GLib-dependent, removed in v3.0 migration).
            // Fix C3: Don't call pipeline.watch_bus() or glib::timeout_add_local()
            // — the tick() method handles bus polling and position updates.
            // Fix E0509: PreloadedSession implements Drop, so we can't move fields
            // out directly. Use ManuallyDrop + ptr::read to take ownership of each
            // field without running Drop. This is safe because we consume ALL fields
            // — there's nothing left for Drop to clean up.
            let preloaded = std::mem::ManuallyDrop::new(preloaded);
            let p_ptr = &*preloaded as *const _;
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

            // Fix: Propagate convolution and loudness settings from the engine
            // to the preloaded session's DSP thread. The preloaded session
            // creates its own Arcs with convolution=None and
            // loudness_enabled=false during pre-roll (to avoid data races
            // while the previous track is still playing). Now that we're
            // swapping in the preloaded session, copy the engine's current
            // settings so the new track retains convolution and loudness
            // normalization.
            {
                // Propagate loudness state (enabled + config) — consolidated
                // lock acquisition avoids the previous 2-lock pattern which
                // could deadlock if another thread acquired them in reverse order.
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

                // Bug #2 fix: Transfer the ConvolutionEngine to the preloaded session's
                // DSP thread by moving the value out of engine.convolution and into
                // preloaded.convolution. The old DSP thread has already been stopped via
                // stop_and_join() above, so there is no contention on either Arc.
                {
                    let mut engine_conv_guard =
                        self.convolution.lock().unwrap_or_else(|e| e.into_inner());
                    let mut preload_conv_guard = preloaded
                        .convolution
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    // Move the ConvolutionEngine (if any) from engine into the preloaded slot.
                    *preload_conv_guard = engine_conv_guard.take();
                    tracing::debug!("Gapless: convolution state transferred to preloaded session");
                }
            }

            // Fix Bug #9: Swap engine.dsp to point at the preloaded session's
            // DspEngine Arc. After the session swap, the preloaded DSP thread
            // owns the active audio path. Without this, all subsequent calls
            // to set_eq_state(), set_volume_gain(), set_balance(),
            // set_replaygain_factor() would mutate the old (disconnected)
            // DspEngine with no effect on audio output. The double-indirection
            // `Mutex<Arc<Mutex<DspEngine>>>` allows atomic Arc swap via
            // `swap_dsp_arc()` so the engine and DSP thread share the same
            // DspEngine instance.
            self.swap_dsp_arc(Arc::clone(&preloaded.dsp));

            // Fix Bug #11: Call mark_new_track() AFTER stop_and_join() and on
            // the PRELOADED DspEngine (now engine.dsp). The old code called
            // mark_new_track() on the old engine.dsp before stop_and_join(),
            // so the preloaded DspEngine never received the signal, breaking
            // the GaplessSmoother's apply_to_head() blending at the boundary.
            self.dsp_arc()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .mark_new_track();

            // Apply ReplayGain for the new track if enabled
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

            // Fix Bug #15: Store the current track path for ReplayGain
            // enable-apply on the currently-playing track.
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
