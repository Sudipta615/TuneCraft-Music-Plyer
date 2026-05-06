use anyhow::{Context, Result};
use std::path::Path;
use tracing::warn;

use crate::util::validation::validate_path_syntax;

/// Supported playlist formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistFormat {
    M3u,
    Xspf,
}

/// Detect the playlist format from the file extension.
///
/// - `.m3u` / `.m3u8` → M3U
/// - `.xspf`          → XSPF
///
/// Returns an error for unrecognised extensions.
pub fn detect_format(path: &Path) -> Result<PlaylistFormat> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("m3u") | Some("m3u8") => Ok(PlaylistFormat::M3u),
        Some("xspf") => Ok(PlaylistFormat::Xspf),
        _ => anyhow::bail!(
            "Unrecognised playlist extension for {:?}. Supported: .m3u, .m3u8, .xspf",
            path
        ),
    }
}

/// Import a playlist file and return the list of file paths it contains.
///
/// Relative paths in the playlist are resolved against the directory that
/// contains the playlist file itself.
pub fn import_playlist(path: &Path) -> Result<Vec<String>> {
    let format = detect_format(path)?;
    match format {
        PlaylistFormat::M3u => import_m3u(path),
        PlaylistFormat::Xspf => import_xspf(path),
    }
}

/// Export a list of file paths as a playlist file.
pub fn export_playlist(path: &Path, tracks: &[String], format: PlaylistFormat) -> Result<()> {
    match format {
        PlaylistFormat::M3u => export_m3u(path, tracks),
        PlaylistFormat::Xspf => export_xspf(path, tracks),
    }
}

// ── M3U ──────────────────────────────────────────────────────────────────────

/// Import an M3U (or M3U8) playlist.
///
/// Lines starting with `#` are treated as comments (including `#EXTM3U` and
/// `#EXTINF` directives). Blank lines are skipped. Each remaining line is a
/// file path – relative paths are resolved against the playlist's parent
/// directory.
fn import_m3u(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read M3U playlist {:?}", path))?;

    let base_dir = path
        .parent()
        .context("Playlist path has no parent directory")?;

    let mut tracks = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip blank lines and comment / directive lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Resolve relative paths against the playlist's directory
        let track_path = if Path::new(trimmed).is_absolute() {
            trimmed.to_string()
        } else {
            let resolved = base_dir.join(trimmed);
            match resolved.to_str() {
                Some(s) => s.to_string(),
                None => {
                    warn!("Skipping non-UTF-8 path in M3U: {:?}", resolved);
                    continue;
                }
            }
        };

        // Fix Bug #24: Use validate_path_syntax instead of validate_file_path.
        // validate_file_path calls canonicalize() which requires the file to
        // exist on disk, causing playlist entries for offline/missing files
        // to be silently dropped. validate_path_syntax performs the same
        // security checks (null bytes, directory traversal, encoding) without
        // requiring file existence, which is the correct behavior for playlist
        // imports — the file may be on a removable drive or network share.
        if let Err(e) = validate_path_syntax(Path::new(&track_path)) {
            warn!(
                "Skipping invalid path in M3U (syntax error): {:?} — {}",
                track_path, e
            );
            continue;
        }

        tracks.push(track_path);
    }

    Ok(tracks)
}

/// Export an M3U playlist.
///
/// Writes the standard `#EXTM3U` header followed by `#EXTINF` entries for each
/// track. The file paths are written as-is (absolute paths are preserved).
///
/// Fix L12: Previously the EXTINF line showed the raw file path, which is
/// unhelpful in most players. Now extracts the filename (without extension)
/// as a more readable display label, falling back to the full path.
fn export_m3u(path: &Path, tracks: &[String]) -> Result<()> {
    let mut out = String::from("#EXTM3U\n");

    for track in tracks {
        // Fix L12: Extract a readable label from the file path.
        // Use the filename without extension as the EXTINF display text,
        // which is more useful than the raw path in most players.
        let label = std::path::Path::new(track)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(track);
        out.push_str(&format!("#EXTINF:-1,{}\n", label));
        out.push_str(&format!("{}\n", track));
    }

    std::fs::write(path, out).with_context(|| format!("Failed to write M3U playlist {:?}", path))
}

// ── XSPF ─────────────────────────────────────────────────────────────────────

/// Import an XSPF (XML Shareable Playlist Format) playlist.
///
/// Extracts every `<location>` value inside `<track>` elements. Both
/// `file:///` URIs and plain paths are accepted. Relative paths are resolved
/// against the playlist's parent directory.
///
/// Fix Bug #25: Replaced the line-based XML parser with a proper event-based
/// XML parser. The previous implementation only worked if each XML tag appeared
/// on its own line. It failed on minified/one-line XSPF files, multi-line tag
/// content, CDATA sections, and XML namespaced elements (e.g. `<xspf:location>`).
/// The new parser handles all these cases correctly.
fn import_xspf(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read XSPF playlist {:?}", path))?;

    let base_dir = path
        .parent()
        .context("Playlist path has no parent directory")?;

    let mut tracks = Vec::new();

    // Event-based XSPF parser using quick-xml style manual parsing.
    // Handles: minified XML, multi-line content, CDATA sections, and
    // namespaced elements (strips namespace prefix before comparing).
    let mut in_track = false;
    let mut in_location = false;
    let mut location_buf = String::new();

    // We parse the XML content by scanning for tags. This is more robust
    // than line-based parsing because it works regardless of whitespace.
    let mut remaining = content.as_str();

    while !remaining.is_empty() {
        // Find the next tag
        if let Some(tag_start) = remaining.find('<') {
            // Any text before the tag is character data
            let text_before = &remaining[..tag_start];
            if in_location {
                location_buf.push_str(text_before);
            }

            // Find the end of the tag
            let after_open = &remaining[tag_start..];
            if let Some(tag_end) = after_open.find('>') {
                let tag_content = &after_open[1..tag_end];
                let is_closing = tag_content.starts_with('/');
                let is_self_closing = tag_content.ends_with('/');

                // Strip the closing slash if present
                let tag_name_raw = if is_closing {
                    &tag_content[1..]
                } else if is_self_closing {
                    &tag_content[..tag_content.len() - 1]
                } else {
                    tag_content
                };

                // Strip attributes and namespace prefix from the tag name
                let tag_name = tag_name_raw.split_whitespace().next().unwrap_or("");
                let local_name = if let Some(colon_pos) = tag_name.find(':') {
                    // Strip namespace prefix (e.g. "xspf:location" → "location")
                    &tag_name[colon_pos + 1..]
                } else {
                    tag_name
                };

                // Check for CDATA section
                if tag_content.starts_with("![CDATA[") {
                    // CDATA section: extract content until ]]>
                    let cdata_start = tag_start + 9; // after "<![CDATA["
                    if let Some(cdata_end) = content[cdata_start..].find("]]>") {
                        let cdata_content = &content[cdata_start..cdata_start + cdata_end];
                        if in_location {
                            location_buf.push_str(cdata_content);
                        }
                        // Skip past ]]>
                        remaining = &content[cdata_start + cdata_end + 3..];
                        continue;
                    }
                }

                match (is_closing, local_name) {
                    (false, "track") => {
                        in_track = true;
                    }
                    (true, "track") => {
                        in_track = false;
                        in_location = false;
                    }
                    (false, "location") if in_track => {
                        in_location = true;
                        location_buf.clear();
                    }
                    (true, "location") if in_track => {
                        in_location = false;
                        let loc_trimmed = location_buf.trim();
                        if !loc_trimmed.is_empty() {
                            let raw_path = decode_xml_entities(loc_trimmed);

                            // Strip file:/// URI scheme if present
                            let file_path = if raw_path.starts_with("file:///") {
                                match url_to_path(&raw_path) {
                                    Some(p) => p,
                                    None => {
                                        warn!(
                                            "Skipping unparseable file URI in XSPF: {}",
                                            raw_path
                                        );
                                        continue;
                                    }
                                }
                            } else if raw_path.starts_with("file://") {
                                raw_path.trim_start_matches("file://").to_string()
                            } else {
                                raw_path
                            };

                            // Resolve relative paths
                            let resolved = if Path::new(&file_path).is_absolute() {
                                file_path
                            } else {
                                match base_dir.join(&file_path).to_str() {
                                    Some(s) => s.to_string(),
                                    None => {
                                        warn!(
                                            "Skipping non-UTF-8 path in XSPF: {:?}",
                                            base_dir.join(&file_path)
                                        );
                                        continue;
                                    }
                                }
                            };

                            // Fix Bug #24: Use validate_path_syntax instead of
                            // validate_file_path so offline files are not dropped.
                            if let Err(e) = validate_path_syntax(Path::new(&resolved)) {
                                warn!(
                                    "Skipping invalid path in XSPF (syntax error): {:?} — {}",
                                    resolved, e
                                );
                                continue;
                            }

                            tracks.push(resolved);
                        }
                    }
                    _ => {}
                }

                remaining = &after_open[tag_end + 1..];
            } else {
                // Malformed tag without closing '>' — stop parsing
                break;
            }
        } else {
            // No more tags — trailing text
            if in_location {
                location_buf.push_str(remaining);
            }
            break;
        }
    }

    Ok(tracks)
}

/// Export an XSPF playlist.
///
/// Produces a minimal but valid XSPF 1.0 document. Each track's path is
/// encoded as a `file:///` URI.
fn export_xspf(path: &Path, tracks: &[String]) -> Result<()> {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <playlist version=\"1\" xmlns=\"http://xspf.org/ns/0/\">\n\
         \t<trackList>\n",
    );

    for track in tracks {
        let uri = path_to_file_uri(track);
        let escaped = encode_xml_entities(&uri);
        out.push_str(&format!(
            "\t\t<track>\n\t\t\t<location>{}</location>\n\t\t</track>\n",
            escaped
        ));
    }

    out.push_str("\t</trackList>\n</playlist>\n");

    std::fs::write(path, out).with_context(|| format!("Failed to write XSPF playlist {:?}", path))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Decode XML character entities, including the five standard named entities
/// and numeric character references (decimal `&#DD;` and hexadecimal `&#xHH;`).
///
/// Fix Bug #66: Previously only handled the five named entities. Now also handles
/// numeric references like `&#39;` (decimal for apostrophe) and `&#x27;` (hex for
/// apostrophe), which are commonly produced by XML serializers.
fn decode_xml_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        if ch == '&' {
            // Find the closing semicolon by scanning ahead in the string
            if let Some(end_pos) = s[i + 1..].find(';') {
                let entity = &s[i + 1..i + 1 + end_pos]; // strip & and ;
                let decoded: Option<String> = match entity {
                    "lt" => Some("<".to_string()),
                    "gt" => Some(">".to_string()),
                    "apos" => Some("'".to_string()),
                    "quot" => Some("\"".to_string()),
                    "amp" => Some("&".to_string()),
                    _ => {
                        // Fix Bug #66: Handle numeric character references
                        if let Some(hex_str) = entity.strip_prefix('#') {
                            if let Some(hex_digits) = hex_str.strip_prefix('x') {
                                // Hexadecimal: &#xHH;
                                if let Ok(code_point) = u32::from_str_radix(hex_digits, 16) {
                                    char::from_u32(code_point).map(|c| c.to_string())
                                } else {
                                    None
                                }
                            } else {
                                // Decimal: &#DD;
                                if let Ok(code_point) = hex_str.parse::<u32>() {
                                    char::from_u32(code_point).map(|c| c.to_string())
                                } else {
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    }
                };
                if let Some(replacement) = decoded {
                    result.push_str(&replacement);
                    // Skip past the semicolon: advance the char iterator past the entity
                    let entity_end = i + 1 + end_pos + 1; // index after ';'
                    while let Some(&(pos, _)) = chars.peek() {
                        if pos < entity_end {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    continue;
                }
            }
            // Not a recognized entity — keep as-is
            result.push(ch);
        } else {
            result.push(ch);
        }
    }

    result
}

/// Encode special characters into XML entities for safe output.
fn encode_xml_entities(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
        .replace('"', "&quot;")
}

/// Convert an absolute filesystem path to a `file:///` URI.
fn path_to_file_uri(path: &str) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        // Percent-encode each component for URI safety
        let mut uri = "file://".to_string();
        // On Unix the path already starts with '/', producing file:///…
        for component in p.components() {
            if let std::path::Component::Normal(os_str) = component {
                if let Some(s) = os_str.to_str() {
                    uri.push('/');
                    uri.push_str(&percent_encode(s));
                }
            } else if let std::path::Component::RootDir = component {
                uri.push('/');
            } else if let std::path::Component::Prefix(prefix) = component {
                // Windows path prefixes: drive letters (C:) and UNC paths (\\server\share)
                #[cfg(target_family = "windows")]
                {
                    uri.push('/');
                    uri.push_str(&percent_encode(&prefix.as_os_str().to_string_lossy()));
                }
                #[cfg(not(target_family = "windows"))]
                {
                    let _ = prefix;
                }
            }
        }
        uri
    } else {
        // Best-effort for relative paths
        format!("file:///{}", percent_encode(path))
    }
}

/// Convert a `file:///` URI back to a filesystem path.
///
/// Handles percent-encoded characters and the `file:///` prefix.
/// Fix H9: On Windows, `file:///C:/Users/...` must become `C:/Users/...`
/// (not `/C:/Users/...` which is invalid). Detects Windows drive letter.
fn url_to_path(uri: &str) -> Option<String> {
    // Handle file:/// prefix (3 slashes for absolute paths)
    if let Some(path_part) = uri.strip_prefix("file:///") {
        let decoded = percent_decode(path_part);
        if decoded.is_empty() {
            return None;
        }
        // Fix H9: On Windows, check for drive letter pattern (e.g. "C:/")
        // If found, don't prepend "/" — the path is already absolute
        if decoded.len() >= 3 && decoded.as_bytes()[1] == b':' && decoded.as_bytes()[2] == b'/' {
            Some(decoded)
        } else {
            Some(format!("/{}", decoded))
        }
    } else if let Some(path_part) = uri.strip_prefix("file://") {
        let decoded = percent_decode(path_part);
        if decoded.is_empty() {
            return None;
        }
        Some(decoded)
    } else {
        None
    }
}

/// Percent-encode a string for use in a URI.
///
/// Encodes everything except unreserved characters (A-Z a-z 0-9 - . _ ~).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}

/// Decode a percent-encoded URI string.
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.bytes();

    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next();
            let lo = chars.next();
            if let (Some(h), Some(l)) = (hi, lo) {
                if let (Some(hv), Some(lv)) = (hex_digit(h), hex_digit(l)) {
                    out.push(char::from(hv << 4 | lv));
                    continue;
                }
            }
            // Malformed percent encoding – keep as-is
            out.push(b as char);
            if let Some(h) = hi {
                out.push(h as char);
            }
            if let Some(l) = lo {
                out.push(l as char);
            }
        } else if b == b'+' {
            // Some URI schemes use + for space, though XSPF normally uses %20
            out.push(' ');
        } else {
            out.push(b as char);
        }
    }
    out
}

/// Parse a single hex digit byte into its numeric value.
fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── M3U tests ────────────────────────────────────────────────────────

    #[test]
    fn test_import_m3u_basic() {
        let dir = TempDir::new().unwrap();
        let playlist_path = dir.path().join("test.m3u");
        fs::write(
            &playlist_path,
            "#EXTM3U\n\
             #EXTINF:123,Artist - Song\n\
             /music/song1.flac\n\
             \n\
             #EXTINF:456,Other - Track\n\
             /music/song2.mp3\n",
        )
        .unwrap();

        let tracks = import_m3u(&playlist_path).unwrap();
        assert_eq!(tracks, vec!["/music/song1.flac", "/music/song2.mp3"]);
    }

    #[test]
    fn test_import_m3u_relative_paths() {
        let dir = TempDir::new().unwrap();
        let playlist_path = dir.path().join("playlist.m3u");
        fs::write(
            &playlist_path,
            "#EXTM3U\n\
             ../music/song.flac\n\
             ./relative/track.mp3\n",
        )
        .unwrap();

        let tracks = import_m3u(&playlist_path).unwrap();
        // Resolved against the playlist's parent directory
        assert!(tracks[0].ends_with("music/song.flac"));
        assert!(tracks[1].ends_with("relative/track.mp3"));
    }

    #[test]
    fn test_export_m3u() {
        let dir = TempDir::new().unwrap();
        let playlist_path = dir.path().join("out.m3u");
        let tracks = vec!["/music/a.flac".to_string(), "/music/b.mp3".to_string()];
        export_m3u(&playlist_path, &tracks).unwrap();

        let content = fs::read_to_string(&playlist_path).unwrap();
        assert!(content.starts_with("#EXTM3U\n"));
        assert!(content.contains("#EXTINF:-1,a\n"));
        assert!(content.contains("/music/a.flac\n"));
        assert!(content.contains("#EXTINF:-1,b\n"));
        assert!(content.contains("/music/b.mp3\n"));
    }

    #[test]
    fn test_roundtrip_m3u() {
        let dir = TempDir::new().unwrap();
        let playlist_path = dir.path().join("roundtrip.m3u");
        let original = vec![
            "/music/alpha.flac".to_string(),
            "/music/beta.mp3".to_string(),
        ];

        export_m3u(&playlist_path, &original).unwrap();
        let imported = import_m3u(&playlist_path).unwrap();
        assert_eq!(imported, original);
    }

    // ── XSPF tests ───────────────────────────────────────────────────────

    #[test]
    fn test_import_xspf_basic() {
        let dir = TempDir::new().unwrap();
        let playlist_path = dir.path().join("test.xspf");
        fs::write(
            &playlist_path,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <playlist version=\"1\" xmlns=\"http://xspf.org/ns/0/\">\n\
               <trackList>\n\
                 <track>\n\
                   <location>file:///music/song1.flac</location>\n\
                 </track>\n\
                 <track>\n\
                   <location>file:///music/song2.mp3</location>\n\
                 </track>\n\
               </trackList>\n\
             </playlist>\n",
        )
        .unwrap();

        let tracks = import_xspf(&playlist_path).unwrap();
        assert_eq!(tracks, vec!["/music/song1.flac", "/music/song2.mp3"]);
    }

    #[test]
    fn test_import_xspf_plain_path() {
        let dir = TempDir::new().unwrap();
        let playlist_path = dir.path().join("plain.xspf");
        fs::write(
            &playlist_path,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <playlist version=\"1\">\n\
               <trackList>\n\
                 <track>\n\
                   <location>/music/song.flac</location>\n\
                 </track>\n\
               </trackList>\n\
             </playlist>\n",
        )
        .unwrap();

        let tracks = import_xspf(&playlist_path).unwrap();
        assert_eq!(tracks, vec!["/music/song.flac"]);
    }

    #[test]
    fn test_export_xspf() {
        let dir = TempDir::new().unwrap();
        let playlist_path = dir.path().join("out.xspf");
        let tracks = vec!["/music/a.flac".to_string(), "/music/b.mp3".to_string()];
        export_xspf(&playlist_path, &tracks).unwrap();

        let content = fs::read_to_string(&playlist_path).unwrap();
        assert!(content.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(content.contains("<playlist version=\"1\""));
        assert!(content.contains("<trackList>"));
        assert!(content.contains("<location>file:///music/a.flac</location>"));
        assert!(content.contains("<location>file:///music/b.mp3</location>"));
        assert!(content.contains("</trackList>"));
        assert!(content.contains("</playlist>"));
    }

    #[test]
    fn test_roundtrip_xspf() {
        let dir = TempDir::new().unwrap();
        let playlist_path = dir.path().join("roundtrip.xspf");
        let original = vec![
            "/music/alpha.flac".to_string(),
            "/music/beta.mp3".to_string(),
        ];

        export_xspf(&playlist_path, &original).unwrap();
        let imported = import_xspf(&playlist_path).unwrap();
        assert_eq!(imported, original);
    }

    // ── Format detection ─────────────────────────────────────────────────

    #[test]
    fn test_detect_format() {
        assert_eq!(
            detect_format(Path::new("playlist.m3u")).unwrap(),
            PlaylistFormat::M3u
        );
        assert_eq!(
            detect_format(Path::new("playlist.m3u8")).unwrap(),
            PlaylistFormat::M3u
        );
        assert_eq!(
            detect_format(Path::new("playlist.xspf")).unwrap(),
            PlaylistFormat::Xspf
        );
        assert!(detect_format(Path::new("playlist.txt")).is_err());
    }

    // ── URI helpers ──────────────────────────────────────────────────────

    #[test]
    fn test_path_to_file_uri() {
        assert_eq!(
            path_to_file_uri("/music/my song.flac"),
            "file:///music/my%20song.flac"
        );
        assert_eq!(
            path_to_file_uri("/music/track.flac"),
            "file:///music/track.flac"
        );
    }

    #[test]
    fn test_url_to_path() {
        assert_eq!(
            url_to_path("file:///music/song.flac"),
            Some("/music/song.flac".to_string())
        );
        assert_eq!(
            url_to_path("file:///music/my%20song.flac"),
            Some("/music/my song.flac".to_string())
        );
    }

    #[test]
    fn test_percent_roundtrip() {
        let original = "café & résumé.ogg";
        let encoded = percent_encode(original);
        let decoded = percent_decode(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_xml_entity_roundtrip() {
        let original = "rock & roll <classics> \"hits\"";
        let encoded = encode_xml_entities(original);
        let decoded = decode_xml_entities(&encoded);
        assert_eq!(decoded, original);
    }
}
