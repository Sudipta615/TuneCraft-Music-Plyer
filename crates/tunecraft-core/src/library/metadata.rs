use anyhow::{Context, Result};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::ItemKey;
use std::path::Path;

use crate::database::models::Track;
use crate::library::coverart::{detect_mime_type as detect_cover_mime_type, CoverArt};

/// Read metadata from an audio file using lofty.
pub fn read_metadata(path: &Path) -> Result<Track> {
    let tagged_file = lofty::probe::Probe::open(path)
        .context("failed to probe file")?
        .guess_file_type()?
        .read()
        .context("failed to read file")?;

    build_track_from_tagged_file(path, &tagged_file)
}

/// Read metadata and cover art from an audio file in a single file open.
pub fn read_metadata_and_cover_art(path: &Path) -> Result<(Track, Option<CoverArt>)> {
    let tagged_file = lofty::probe::Probe::open(path)
        .context("failed to probe file")?
        .guess_file_type()?
        .read()
        .context("failed to read file")?;

    let track = build_track_from_tagged_file(path, &tagged_file)?;

    let cover_art = extract_cover_art_from_tagged(&tagged_file);

    Ok((track, cover_art))
}

/// Build a Track struct from an already-parsed lofty TaggedFile.
fn build_track_from_tagged_file(
    path: &Path,
    tagged_file: &lofty::file::TaggedFile,
) -> Result<Track> {
    let properties = tagged_file.properties();
    let primary_tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let title = primary_tag.and_then(|t| t.get_string(&ItemKey::TrackTitle).map(|s| s.to_string()));
    let artist =
        primary_tag.and_then(|t| t.get_string(&ItemKey::TrackArtist).map(|s| s.to_string()));
    let album = primary_tag.and_then(|t| t.get_string(&ItemKey::AlbumTitle).map(|s| s.to_string()));
    let genre = primary_tag.and_then(|t| t.get_string(&ItemKey::Genre).map(|s| s.to_string()));
    let year = primary_tag
        .and_then(|t| t.get_string(&ItemKey::Year))
        .and_then(|s| s.parse::<u32>().ok())
        .map(|v| v as i32);
    let track_number = primary_tag
        .and_then(|t| t.get_string(&ItemKey::TrackNumber))
        .and_then(|s| s.parse::<u32>().ok())
        .map(|v| v as i32);

    let duration = properties.duration().as_secs();
    let sample_rate = properties.sample_rate().map(|v| v as i32);
    let bitrate = properties.audio_bitrate().map(|v| v as i32);

    let file_meta = std::fs::metadata(path).ok();
    Ok(Track {
        id: None,
        file_path: path.to_string_lossy().to_string(),
        file_hash: None,
        file_size: file_meta.as_ref().map(|m| m.len() as i64),
        file_mtime: file_meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
        title,
        artist,
        album,
        genre,
        year,
        track_number,
        duration: if duration > 0 { Some(duration) } else { None },
        sample_rate,
        bitrate,
        play_count: None,
        skip_count: None,
        rating: None,
        love: None,
        bpm: None,
        energy: None,
        bass_ratio: None,
        spectral_centroid: None,
        dynamic_range: None,
        mood: None,
        mood_override: None,
        date_added: chrono::Local::now().date_naive(),
        last_played: None,
    })
}

/// Extract cover art from an already-parsed lofty TaggedFile.
///
/// This avoids a second file open by reusing the already-loaded tag data.
/// The logic mirrors `coverart::extract_cover_art` but operates on the
/// already-parsed tagged_file instead of opening the file again.
fn extract_cover_art_from_tagged(tagged_file: &lofty::file::TaggedFile) -> Option<CoverArt> {
    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    if let Some(tag) = tag {
        let mut fallback: Option<&lofty::picture::Picture> = None;

        for picture in tag.pictures() {
            if picture.pic_type() == lofty::picture::PictureType::CoverFront {
                let mime_type = detect_cover_mime_type(picture);
                return Some(CoverArt {
                    data: picture.data().to_vec(),
                    mime_type,
                    width: None,
                    height: None,
                });
            }
            if fallback.is_none() {
                fallback = Some(picture);
            }
        }

        if let Some(picture) = fallback {
            let mime_type = detect_cover_mime_type(picture);
            return Some(CoverArt {
                data: picture.data().to_vec(),
                mime_type,
                width: None,
                height: None,
            });
        }
    }

    None
}
