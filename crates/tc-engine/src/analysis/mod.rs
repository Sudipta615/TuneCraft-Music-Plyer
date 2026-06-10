//! Off-thread audio analysis for visualization and metadata
//!
//! This module re-exports analysis types from the `tc-analysis` crate,
//! which is the canonical implementation. 

// Re-export the canonical analysis types from tc-analysis
pub use tc_analysis::{BpmDetector, TrackAnalysis, WaveformGenerator as FileWaveformGenerator};
