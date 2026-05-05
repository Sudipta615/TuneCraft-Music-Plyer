pub mod hash;
pub mod validation;
pub mod crypto;
pub mod i18n;

/// Reinterpret a `&[u8]` GStreamer buffer as `&[f32]`.
///
/// Returns `None` if the buffer is misaligned or its length is not a
/// multiple of 4. This is safer than panicking in the audio callback —
/// a malformed GStreamer buffer should skip the sample rather than crash
/// the entire application.
///
/// This is the single shared implementation used by both `convolution.rs`
/// and `pipeline.rs` to avoid code duplication.
pub fn cast_u8_to_f32(bytes: &[u8]) -> Option<&[f32]> {
    if bytes.len() % 4 != 0 {
        tracing::warn!(
            "F32LE buffer length {} is not a multiple of 4, skipping sample",
            bytes.len()
        );
        return None;
    }
    if bytes.as_ptr() as usize % std::mem::align_of::<f32>() != 0 {
        tracing::warn!("F32LE buffer is not 4-byte aligned, skipping sample");
        return None;
    }
    // SAFETY: We verified alignment and length above. GStreamer guarantees
    // F32LE buffers are properly aligned and sized, but we guard against
    // malformed buffers rather than panicking in the audio callback.
    Some(unsafe {
        std::slice::from_raw_parts(bytes.as_ptr() as *const f32, bytes.len() / 4)
    })
}
