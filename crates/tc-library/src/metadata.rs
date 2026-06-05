//! Metadata extraction pipeline for audio files.
//!
//! Provides symphonia-based probing, tag reading, and track building.

use std::path::Path;

use log::warn;
use tc_db::models::Track;

use super::{CoverArtData, FileTags, LibraryError};

impl super::LibraryManager {
    /// Extract track info for an **updated** file (no cover art re-extraction).
    pub(crate) fn extract_track_info(
        &self,
        path: &Path,
        file_size: i64,
        file_modified: i64,
    ) -> Result<Track, LibraryError> {
        let (dur, sr, ch, tags, _cover) = Self::probe_file(path).ok_or_else(|| {
            LibraryError::Other(format!("Could not probe audio info for {}", path.display()))
        })?;
        self.build_track(path, file_size, file_modified, dur, sr, ch, tags)
    }

    /// Combined probe for **new** files: metadata + cover art in one file open.
    pub(crate) fn extract_track_info_with_cover(
        &self,
        path: &Path,
        file_size: i64,
        file_modified: i64,
    ) -> Result<(Track, Option<CoverArtData>), LibraryError> {
        let (dur, sr, ch, tags, cover) = Self::probe_file(path).ok_or_else(|| {
            LibraryError::Other(format!("Could not probe audio info for {}", path.display()))
        })?;
        let track = self.build_track(path, file_size, file_modified, dur, sr, ch, tags)?;
        Ok((track, cover))
    }

    /// Single-pass symphonia probe: audio parameters + tags + embedded cover art.
    ///
    /// All callers use this method; symphonia setup is no longer duplicated.
    ///
    /// Returns `(duration_secs, sample_rate, channels, FileTags, Option<CoverArtData>)`.
    /// `duration_secs` is `-1.0` when `n_frames` is unavailable; callers must
    /// convert to `0.0` before storing (see `build_track`).
    pub(crate) fn probe_file(
        path: &Path,
    ) -> Option<(f64, u32, usize, FileTags, Option<CoverArtData>)> {
        use symphonia::core::{
            codecs::CODEC_TYPE_NULL, formats::FormatOptions, io::MediaSourceStream,
            meta::MetadataOptions, probe::Hint,
        };

        let file = std::fs::File::open(path).ok()?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let mut probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .ok()?;

        let track = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)?;

        let codec_params = &track.codec_params;
        let sample_rate = codec_params.sample_rate.unwrap_or(0);
        let channels = codec_params.channels.map(|c| c.count()).unwrap_or(0);

        if sample_rate == 0 || channels == 0 {
            warn!(
                "File {} missing codec parameters (sample_rate={}, channels={}) — skipping",
                path.display(),
                sample_rate,
                channels
            );
            return None;
        }

        let duration = codec_params
            .n_frames
            .map(|n| n as f64 / sample_rate.max(1) as f64)
            .unwrap_or(-1.0);

        let (tags, cover_art) = Self::extract_tags_and_cover_from_probed(&mut probed);
        Some((duration, sample_rate, channels, tags, cover_art))
    }

    /// Build a `Track` from probed parameters, clamping invalid values.
    pub(crate) fn build_track(
        &self,
        path: &Path,
        file_size: i64,
        file_modified: i64,
        duration_secs: f64,
        sample_rate: u32,
        channels: usize,
        tags: FileTags,
    ) -> Result<Track, LibraryError> {
        if duration_secs == 0.0 && sample_rate == 0 {
            return Err(LibraryError::Other(format!(
                "Invalid audio metadata for {} (zero duration and sample rate)",
                path.display()
            )));
        }

        let bitrate_kbps = if duration_secs > 0.0 {
            Some(((file_size as f64 * 8.0) / duration_secs / 1000.0).round() as i32)
        } else if file_size > 0 {
            warn!(
                "Duration unavailable for {}; bitrate estimated from file size assuming 3 min",
                path.display()
            );
            Some(((file_size as f64 * 8.0) / 180.0 / 1000.0).round() as i32)
        } else {
            None
        };

        // Negative duration means n_frames was unavailable; store as 0.0
        let stored_duration = if duration_secs < 0.0 {
            warn!(
                "Duration unknown for {} (n_frames unavailable); storing 0.0",
                path.display()
            );
            0.0_f64
        } else {
            duration_secs
        };

        let title = tags
            .title
            .unwrap_or_else(|| match path.file_stem().and_then(|s| s.to_str()) {
                Some(stem) => stem.to_string(),
                None => {
                    warn!(
                        "Non-UTF-8 filename for {}; falling back to title \"Unknown\"",
                        path.display()
                    );
                    "Unknown".to_string()
                },
            });

        Ok(Track {
            id: 0,
            path: path.to_string_lossy().into_owned(),
            title,
            artist: tags.artist,
            album: tags.album,
            album_artist: tags.album_artist,
            genre: tags.genre,
            year: tags.year,
            track_number: tags.track_number,
            disc_number: tags.disc_number,
            duration_secs: stored_duration,
            sample_rate: sample_rate as i32,
            channels: channels as i32,
            bitrate_kbps,
            format: path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown")
                .to_string(),
            file_size,
            file_modified,
            crc32: None,
            replaygain_track_db: None,
            replaygain_album_db: None,
            replaygain_track_peak: None,
            replaygain_album_peak: None,
            ebu_r128_loudness: None,
            ebu_r128_peak: None,
            bpm: None,
            mood: None,
            lyrics_synced: None,
            lyrics_unsynced: None,
            last_played: None,
            play_count: 0,
            date_added: chrono::Utc::now().naive_utc(),
            date_scanned: chrono::Utc::now().naive_utc(),
        })
    }

    /// Extract tags and cover art from a symphonia `ProbeResult` in one pass.
    pub(crate) fn extract_tags_and_cover_from_probed(
        probed: &mut symphonia::core::probe::ProbeResult,
    ) -> (FileTags, Option<CoverArtData>) {
        let mut tags = FileTags::default();
        let mut cover: Option<CoverArtData> = None;

        if let Some(mut metadata) = probed.metadata.get() {
            if let Some(rev) = metadata.current() {
                Self::read_tags_from_revision(rev, &mut tags);
                if cover.is_none() {
                    cover = Self::extract_visual_from_revision(rev);
                }
            }
            if let Some(rev) = metadata.skip_to_latest() {
                Self::read_tags_from_revision(rev, &mut tags);
                if cover.is_none() {
                    cover = Self::extract_visual_from_revision(rev);
                }
            }
        }
        {
            let mut fmt_meta = probed.format.metadata();
            if let Some(rev) = fmt_meta.current() {
                Self::read_tags_from_revision(rev, &mut tags);
                if cover.is_none() {
                    cover = Self::extract_visual_from_revision(rev);
                }
            }
            if let Some(rev) = fmt_meta.skip_to_latest() {
                Self::read_tags_from_revision(rev, &mut tags);
                if cover.is_none() {
                    cover = Self::extract_visual_from_revision(rev);
                }
            }
        }
        (tags, cover)
    }

    /// Read tags from a `MetadataRevision` into `FileTags` (first-source-wins).
    pub(crate) fn read_tags_from_revision(
        revision: &symphonia::core::meta::MetadataRevision,
        tags: &mut FileTags,
    ) {
        use symphonia::core::meta::StandardTagKey;
        for tag in revision.tags() {
            if let Some(std_key) = tag.std_key {
                match std_key {
                    StandardTagKey::TrackTitle => {
                        if tags.title.is_none() {
                            tags.title = tag_value_to_string(&tag.value);
                        }
                    },
                    StandardTagKey::Artist => {
                        if tags.artist.is_none() {
                            tags.artist = tag_value_to_string(&tag.value);
                        }
                    },
                    StandardTagKey::Album => {
                        if tags.album.is_none() {
                            tags.album = tag_value_to_string(&tag.value);
                        }
                    },
                    StandardTagKey::AlbumArtist => {
                        if tags.album_artist.is_none() {
                            tags.album_artist = tag_value_to_string(&tag.value);
                        }
                    },
                    StandardTagKey::Genre => {
                        if tags.genre.is_none() {
                            tags.genre = tag_value_to_string(&tag.value);
                        }
                    },
                    StandardTagKey::Date => {
                        if tags.year.is_none() {
                            tags.year = tag_value_to_year(&tag.value);
                        }
                    },
                    StandardTagKey::TrackNumber => {
                        if tags.track_number.is_none() {
                            tags.track_number = tag_value_to_i32(&tag.value);
                        }
                    },
                    StandardTagKey::DiscNumber => {
                        if tags.disc_number.is_none() {
                            tags.disc_number = tag_value_to_i32(&tag.value);
                        }
                    },
                    _ => {},
                }
            }
        }
    }

    /// Read metadata tags from a file without full decoding.
    pub fn read_file_tags(path: &Path) -> Option<FileTags> {
        Self::probe_file(path).map(|(_, _, _, tags, _)| tags)
    }
}

pub(crate) fn tag_value_to_string(value: &symphonia::core::meta::Value) -> Option<String> {
    match value {
        symphonia::core::meta::Value::String(s) => Some(s.clone()),
        symphonia::core::meta::Value::UnsignedInt(u) => Some(u.to_string()),
        symphonia::core::meta::Value::SignedInt(i) => Some(i.to_string()),
        symphonia::core::meta::Value::Float(f) => Some(f.to_string()),
        _ => None,
    }
}

pub(crate) fn tag_value_to_i32(value: &symphonia::core::meta::Value) -> Option<i32> {
    match value {
        symphonia::core::meta::Value::SignedInt(i) => Some(*i as i32),
        symphonia::core::meta::Value::UnsignedInt(u) => Some(*u as i32),
        symphonia::core::meta::Value::String(s) => s.parse::<i32>().ok(),
        _ => None,
    }
}

pub(crate) fn tag_value_to_year(value: &symphonia::core::meta::Value) -> Option<i32> {
    match value {
        symphonia::core::meta::Value::SignedInt(i) => Some(*i as i32),
        symphonia::core::meta::Value::UnsignedInt(u) => Some(*u as i32),
        symphonia::core::meta::Value::String(s) => {
            if let Ok(y) = s.parse::<i32>() {
                return Some(y);
            }
            s.split('-').next().and_then(|p| p.parse::<i32>().ok())
        },
        _ => None,
    }
}
