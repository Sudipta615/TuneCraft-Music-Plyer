pub mod crypto;
pub mod hash;
pub mod i18n;
pub mod validation;

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
    if !bytes.len().is_multiple_of(4) {
        tracing::warn!(
            "F32LE buffer length {} is not a multiple of 4, skipping sample",
            bytes.len()
        );
        return None;
    }
    if !(bytes.as_ptr() as usize).is_multiple_of(std::mem::align_of::<f32>()) {
        tracing::warn!("F32LE buffer is not 4-byte aligned, skipping sample");
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const f32, bytes.len() / 4) })
}
