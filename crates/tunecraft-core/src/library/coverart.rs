use anyhow::{Context, Result};
use lofty::file::TaggedFileExt;
use std::path::Path;

/// Maximum allowed cover art size (10 MB). Larger images are skipped to avoid
/// excessive memory usage and UI lag from decoding huge embedded artwork.
/// Fix Bug #7: Previously had no size limit, allowing 100MB+ embedded images
/// to be loaded into memory, potentially causing OOM or severe UI jank.
const MAX_COVER_ART_BYTES: usize = 10 * 1024 * 1024;

/// Extract cover art from an audio file using lofty.
/// Returns the raw image bytes and the correct MIME type detected
/// from the embedded picture data.
pub fn extract_cover_art(path: &Path) -> Result<Option<CoverArt>> {
    let tagged_file = lofty::probe::Probe::open(path)
        .context("failed to probe file")?
        .guess_file_type()?
        .read()
        .context("failed to read file")?;

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    if let Some(tag) = tag {
        // Try front cover first, then fall back to any available picture.
        // Many files use PictureType::Other or don't set the type correctly.
        let mut fallback: Option<&lofty::picture::Picture> = None;

        for picture in tag.pictures() {
            if picture.pic_type() == lofty::picture::PictureType::CoverFront {
                // Best case: explicit front cover
                if picture.data().len() > MAX_COVER_ART_BYTES {
                    tracing::warn!(
                        "Cover art for {:?} is {} bytes (exceeds {} MB limit), skipping",
                        path,
                        picture.data().len(),
                        MAX_COVER_ART_BYTES / (1024 * 1024)
                    );
                    continue;
                }
                let mime_type = detect_mime_type(picture);
                return Ok(Some(CoverArt {
                    data: picture.data().to_vec(),
                    mime_type,
                    width: None,
                    height: None,
                }));
            }
            // Keep the first picture of any type as a fallback
            if fallback.is_none() {
                fallback = Some(picture);
            }
        }

        // No CoverFront found — use whatever picture is available
        if let Some(picture) = fallback {
            if picture.data().len() > MAX_COVER_ART_BYTES {
                tracing::warn!(
                    "Fallback cover art for {:?} is {} bytes (exceeds {} MB limit), skipping",
                    path,
                    picture.data().len(),
                    MAX_COVER_ART_BYTES / (1024 * 1024)
                );
                return Ok(None);
            }
            let mime_type = detect_mime_type(picture);
            return Ok(Some(CoverArt {
                data: picture.data().to_vec(),
                mime_type,
                width: None,
                height: None,
            }));
        }
    }

    Ok(None)
}

/// Detect the MIME type of a picture, using the declared MIME type
/// from the tag if available, otherwise sniffing the magic bytes.
///
/// Made pub(crate) so the combined `read_metadata_and_cover_art` function
/// in metadata.rs can reuse this without a second file open (Bug #26 fix).
pub(crate) fn detect_mime_type(picture: &lofty::picture::Picture) -> String {
    // Try the tag's declared MIME type first
    if let Some(mime) = picture.mime_type() {
        match mime {
            lofty::picture::MimeType::Png => return "image/png".to_string(),
            lofty::picture::MimeType::Jpeg => return "image/jpeg".to_string(),
            lofty::picture::MimeType::Gif => return "image/gif".to_string(),
            lofty::picture::MimeType::Bmp => return "image/bmp".to_string(),
            lofty::picture::MimeType::Unknown(ref s) if s.contains("webp") => {
                return "image/webp".to_string()
            }
            _ => {}
        }
    }

    // Fallback: sniff magic bytes from the image data
    let data = picture.data();
    if data.len() >= 8 {
        // PNG: 89 50 4E 47
        if data[..4] == [0x89, 0x50, 0x4E, 0x47] {
            return "image/png".to_string();
        }
        // JPEG: FF D8 FF
        if data[..3] == [0xFF, 0xD8, 0xFF] {
            return "image/jpeg".to_string();
        }
        // GIF: "GIF87a" or "GIF89a"
        if data[..3] == [b'G', b'I', b'F'] {
            return "image/gif".to_string();
        }
        // WebP: "RIFF....WEBP"
        if data.len() >= 12 && &data[8..12] == b"WEBP" {
            return "image/webp".to_string();
        }
        // BMP: "BM"
        if data[..2] == [b'B', b'M'] {
            return "image/bmp".to_string();
        }
    }

    // Ultimate fallback
    // Unknown image format — use generic MIME type instead of assuming JPEG
    "application/octet-stream".to_string()
}

/// Extracted cover art data.
#[derive(Debug, Clone)]
pub struct CoverArt {
    pub data: Vec<u8>,
    pub mime_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}
