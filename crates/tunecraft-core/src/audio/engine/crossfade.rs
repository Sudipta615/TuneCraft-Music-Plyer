//! Crossfade control methods.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::audio::crossfade::CrossfadeEngine;

use super::{AudioEngine, EndOfStreamCallback};

impl AudioEngine {
    pub fn load_with_crossfade(
        &self,
        path: &Path,
        fade_duration_ms: u32,
        eos_cb: EndOfStreamCallback,
    ) -> Result<()> {
        let uri = super::path_to_uri(path)?;
        {
            let mut ts = self
                .transport_state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            ts.use_crossfade = true;
            ts.fade_duration_ms = fade_duration_ms;
        }
        let vol = self
            .volume_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .volume;
        let engine = CrossfadeEngine::new(fade_duration_ms)?;
        engine.load_track(&uri)?;
        engine.set_volume(vol);
        engine.on_end_of_stream(eos_cb);
        *self.crossfade.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(engine));
        Ok(())
    }

    pub fn load_track_with_crossfade(&self, uri: &str) -> Result<()> {
        let cf = self.crossfade.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref e) = *cf {
            e.load_track_with_crossfade(uri)?;
        }
        Ok(())
    }

    pub fn fade_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(
            self.transport_state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .fade_duration_ms as u64,
        )
    }
    pub fn set_fade_duration(&self, ms: u32) {
        self.transport_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .fade_duration_ms = ms;
    }
}
