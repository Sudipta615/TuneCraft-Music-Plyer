pub mod analysis;
pub mod buffer;
pub mod decode;
pub mod dsp;
#[cfg(feature = "audio-output")]
pub mod engine;
#[cfg(feature = "audio-output")]
pub mod output;

// prelude, so `tc_engine::AudioEngine` works directly (not just via prelude).
#[cfg(feature = "audio-output")]
pub use engine::AudioEngine;
// v3.1.0: re-export spectrum types at crate root so UI code can write
// `tc_engine::SpectrumSnapshot` without diving into the dsp module tree.
pub use dsp::spectrum::{SpectrumAnalyzer, SpectrumSnapshot, NUM_VISUAL_BANDS};

pub mod prelude {
    #[cfg(feature = "audio-output")]
    pub use crate::engine::AudioEngine;
    #[cfg(feature = "audio-output")]
    pub use crate::engine::PlaybackStream;
    pub use crate::{
        buffer::{
            AudioChunk, AudioFrame, BufferError, EngineCommand, FixedFrameBuffer, PlaybackInfo,
            PlaybackState, DEFAULT_SAMPLE_RATE,
        },
        dsp::pipeline::DspPipeline,
    };
}
