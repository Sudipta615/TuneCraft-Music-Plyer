//! Cover art extraction and MIME detection.

use std::path::Path;

use log::info;

use super::{LibraryError, LibraryManager};

/// Cover art data extracted from an audio file's embedded metadata.
pub struct CoverArtData {
    /// Raw image bytes
    pub data: Vec<u8>,
    /// MIME type detected from magic bytes
    pub mime_type: String,
    /// CRC32 hash of data (hex string) for deduplication
    pub data_hash: String,
    /// Image dimensions (0×0 if the image crate cannot decode them)
    pub width: i32,
    pub height: i32,
}

impl LibraryManager {
    /// Extract the first `Visual` (cover art) from a revision into `CoverArtData`.
    ///
    /// In symphonia 0.5.x, visual data is accessed via `revision.visuals()`
    /// rather than through tag keys.
    pub(crate) fn extract_visual_from_revision(
        revision: &symphonia::core::meta::MetadataRevision,
    ) -> Option<CoverArtData> {
        use symphonia::core::meta::StandardVisualKey;

        // Prefer FrontCover, then any visual with a standard key, then any visual at all
        let visuals = revision.visuals();
        let visual = visuals
            .iter()
            .find(|v| v.usage == Some(StandardVisualKey::FrontCover))
            .or_else(|| visuals.iter().find(|v| v.usage.is_some()))
            .or_else(|| visuals.first())?;

        let data = &visual.data;
        let mime_type = if visual.media_type.is_empty() {
            detect_image_mime(data)
        } else {
            visual.media_type.clone()
        };
        let data_hash = format!("{:x}", crc32fast::hash(data));
        let (width, height) = image::ImageReader::new(std::io::Cursor::new(data.as_ref()))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok())
            .map(|(w, h)| (w as i32, h as i32))
            .unwrap_or((0, 0));
        Some(CoverArtData {
            data: data.to_vec(),
            mime_type,
            data_hash,
            width,
            height,
        })
    }

    /// Extract and persist cover art for a single track (on-demand, outside scan).
    ///
    /// During normal scans the combined `probe_file` path is used instead.
    pub fn extract_cover_art(
        &self,
        path: &Path,
        track_id: i64,
    ) -> Result<Option<i64>, LibraryError> {
        let (_, _, _, _, cover) = Self::probe_file(path)
            .ok_or_else(|| LibraryError::Other(format!("Probe failed for {}", path.display())))?;

        let art = match cover {
            Some(a) => a,
            None => return Ok(None),
        };

        let album_id = self.db.get_track(track_id).ok().flatten().and_then(|t| {
            let album = t.album.as_deref()?;
            self.db
                .get_album_id(album, t.album_artist.as_deref())
                .ok()
                .flatten()
        });

        let id = self.db.insert_cover_art(
            album_id,
            Some(track_id),
            None,
            Some(&art.data),
            Some(&art.data_hash),
            art.width,
            art.height,
            &art.mime_type,
        )?;

        info!(
            "Extracted cover art for track {} ({} bytes, {}×{})",
            track_id,
            art.data.len(),
            art.width,
            art.height
        );
        Ok(Some(id))
    }
}

pub fn detect_image_mime(data: &[u8]) -> String {
    if data.len() >= 3 && data[0..3] == [0xFF, 0xD8, 0xFF] {
        return "image/jpeg".to_string();
    }
    if data.len() >= 4 && data[0..4] == [0x89, 0x50, 0x4E, 0x47] {
        return "image/png".to_string();
    }
    if data.len() >= 4 && data[0..4] == [0x47, 0x49, 0x46, 0x38] {
        return "image/gif".to_string();
    }
    if data.len() >= 12
        && data[0..4] == [0x52, 0x49, 0x46, 0x46]
        && data[8..12] == [0x57, 0x45, 0x42, 0x50]
    {
        return "image/webp".to_string();
    }
    if data.len() >= 2 && data[0..2] == [0x42, 0x4D] {
        return "image/bmp".to_string();
    }
    // L13: Returning "image/jpeg" for unrecognized data causes renderers to
    // attempt JPEG decoding on arbitrary bytes, which produces decode errors or
    // garbled images. Return a neutral octet-stream MIME type instead.
    "application/octet-stream".to_string()
}
