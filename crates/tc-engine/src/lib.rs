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

pub mod prelude {
    pub use crate::buffer::{
        AudioChunk, AudioFrame, BufferError, EngineCommand, FixedFrameBuffer, PlaybackInfo,
        PlaybackState, DEFAULT_SAMPLE_RATE,
    };
    pub use crate::dsp::pipeline::DspPipeline;
    #[cfg(feature = "audio-output")]
    pub use crate::engine::AudioEngine;
    #[cfg(feature = "audio-output")]
    pub use crate::engine::PlaybackStream;
}
