//! Input validation utilities to prevent directory traversal and other security issues.
//!
//! # Security Model
//!
//! TuneCraft operates as a local offline music player. Users specify "watch directories"
//! that contain their music library. The scanner walks these directories and reads
//! metadata from discovered files. This module validates:
//!
//! 1. **File paths** — ensure they don't escape allowed directories via `..` or symlinks
//! 2. **URLs** — validate scheme before opening in the browser
//! 3. **File extensions** — only process known audio formats

use std::path::{Component, Path, PathBuf};

/// Validate that a file path does not contain directory traversal sequences
/// and resolves to a location within the allowed base directory.
pub fn validate_file_path(
    path: &Path,
    allowed_dirs: &[PathBuf],
) -> Result<PathBuf, PathValidationError> {
    let path_str = path.to_string_lossy();
    if path_str.contains('\0') {
        return Err(PathValidationError::NullByte(path_str.into_owned()));
    }

    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(PathValidationError::DirectoryTraversal(
                    path_str.into_owned(),
                ));
            }
            Component::Prefix(_) => {
                #[cfg(not(target_family = "windows"))]
                return Err(PathValidationError::UnexpectedPrefix(path_str.into_owned()));
            }
            _ => {}
        }
    }

    let canonical =
        path.canonicalize()
            .map_err(|e| PathValidationError::CanonicalizationFailed {
                path: path_str.into_owned(),
                source: e,
            })?;

    if !allowed_dirs.is_empty() {
        let mut is_within_allowed = false;
        for dir in allowed_dirs {
            if let Ok(canonical_dir) = dir.canonicalize() {
                if canonical.starts_with(&canonical_dir) {
                    is_within_allowed = true;
                    break;
                }
            }
        }
        if !is_within_allowed {
            return Err(PathValidationError::OutsideAllowedDirectory {
                path: canonical,
                allowed_dirs: allowed_dirs.to_vec(),
            });
        }
    }

    Ok(canonical)
}

/// Validate that a path's syntax is safe without requiring the file to exist.
pub fn validate_path_syntax(path: &Path) -> Result<(), PathValidationError> {
    let path_str = path.to_string_lossy();
    if path_str.contains('\0') {
        return Err(PathValidationError::NullByte(path_str.into_owned()));
    }

    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(PathValidationError::DirectoryTraversal(
                    path_str.into_owned(),
                ));
            }
            Component::Prefix(_) => {
                #[cfg(not(target_family = "windows"))]
                return Err(PathValidationError::UnexpectedPrefix(path_str.into_owned()));
            }
            _ => {}
        }
    }

    Ok(())
}

/// Validate that a path is safe to load as an audio file.
///
/// This combines:
/// - Directory traversal checks
/// - Extension validation against known audio formats
/// - File existence and readability check
pub fn validate_audio_file_path(
    path: &Path,
    allowed_dirs: &[PathBuf],
) -> Result<PathBuf, PathValidationError> {
    let canonical = validate_file_path(path, allowed_dirs)?;

    const AUDIO_EXTENSIONS: &[&str] = &[
        "mp3", "flac", "wav", "ogg", "opus", "aac", "m4a", "wma", "aiff", "ape", "alac",
    ];

    let is_audio = canonical
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false);

    if !is_audio {
        return Err(PathValidationError::UnsupportedFormat(canonical));
    }

    if !canonical.is_file() {
        return Err(PathValidationError::NotAFile(canonical));
    }

    Ok(canonical)
}

/// Validate a URL before opening it in the user's browser.
///
/// This prevents:
/// - `javascript:` scheme injection
/// - `file:` scheme access to local files
/// - `data:` scheme for phishing
/// - Any non-HTTPS URL when HTTPS is expected
///
/// Only `http://` and `https://` schemes are allowed.
pub fn validate_url(url: &str) -> Result<url::Url, UrlValidationError> {
    let parsed = url::Url::parse(url).map_err(|e| UrlValidationError::ParseFailed {
        url: url.to_string(),
        source: e,
    })?;

    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" => {
            tracing::warn!("Opening non-HTTPS URL: {}", url);
            Ok(parsed)
        }
        "javascript" => Err(UrlValidationError::DangerousScheme {
            scheme: "javascript".to_string(),
            url: url.to_string(),
        }),
        "file" => Err(UrlValidationError::DangerousScheme {
            scheme: "file".to_string(),
            url: url.to_string(),
        }),
        "data" => Err(UrlValidationError::DangerousScheme {
            scheme: "data".to_string(),
            url: url.to_string(),
        }),
        other => Err(UrlValidationError::UnsupportedScheme {
            scheme: other.to_string(),
            url: url.to_string(),
        }),
    }
}

/// Validate that a Last.fm auth URL is well-formed and points to the expected domain.
///
/// Uses exact domain suffix matching to prevent spoofing via domains like
/// `notlast.fm` or `evilast.fm`. Only `last.fm`, `*.last.fm`, and
/// `*.audioscrobbler.com` are allowed.
pub fn validate_lastfm_auth_url(url: &str) -> Result<url::Url, UrlValidationError> {
    let parsed = validate_url(url)?;

    let host = parsed.host_str().unwrap_or("");
    let is_valid_host = host == "last.fm"
        || host == "www.last.fm"
        || host.ends_with(".last.fm")
        || host == "audioscrobbler.com"
        || host == "ws.audioscrobbler.com"
        || host.ends_with(".audioscrobbler.com");

    if !is_valid_host {
        return Err(UrlValidationError::UnexpectedHost {
            host: host.to_string(),
            url: url.to_string(),
        });
    }

    Ok(parsed)
}

/// Errors that can occur during file path validation.
#[derive(Debug, thiserror::Error)]
pub enum PathValidationError {
    /// The path contains a null byte which could cause truncation.
    #[error("path contains null byte: {0}")]
    NullByte(String),

    /// The path contains `..` which could traverse outside allowed directories.
    #[error("path contains directory traversal: {0}")]
    DirectoryTraversal(String),

    /// The path contains an unexpected prefix (e.g., Windows UNC path).
    #[error("path contains unexpected prefix: {0}")]
    UnexpectedPrefix(String),

    /// The path could not be canonicalized (doesn't exist or permission denied).
    /// Use `validate_path_syntax()` if you need to validate a path without
    /// requiring it to exist on disk.
    #[error("failed to canonicalize path '{path}': {source}")]
    CanonicalizationFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// The path resolves to a location outside all allowed directories.
    #[error("path '{path}' is outside allowed directories")]
    OutsideAllowedDirectory {
        path: PathBuf,
        allowed_dirs: Vec<PathBuf>,
    },

    /// The file extension is not a supported audio format.
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(PathBuf),

    /// The path does not point to a regular file.
    #[error("not a regular file: {0}")]
    NotAFile(PathBuf),
}

/// Errors that can occur during URL validation.
#[derive(Debug, thiserror::Error)]
pub enum UrlValidationError {
    /// The URL could not be parsed.
    #[error("failed to parse URL '{url}': {source}")]
    ParseFailed {
        url: String,
        #[source]
        source: url::ParseError,
    },

    /// The URL uses a dangerous scheme (javascript:, file:, data:).
    #[error("dangerous URL scheme '{scheme}' in: {url}")]
    DangerousScheme { scheme: String, url: String },

    /// The URL uses a scheme that is not allowed.
    #[error("unsupported URL scheme '{scheme}' in: {url}")]
    UnsupportedScheme { scheme: String, url: String },

    /// The URL points to an unexpected host.
    #[error("unexpected host '{host}' in URL: {url}")]
    UnexpectedHost { host: String, url: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_syntax_rejects_traversal() {
        let path = Path::new("/music/../../../etc/passwd");
        assert!(validate_path_syntax(path).is_err());
    }

    #[test]
    fn test_validate_path_syntax_rejects_null_byte() {
        let path = Path::new("/music/file.mp3\0.exe");
        assert!(validate_path_syntax(path).is_err());
    }

    #[test]
    fn test_validate_path_syntax_accepts_nonexistent() {
        let path = Path::new("/nonexistent/path/to/file.mp3");
        assert!(validate_path_syntax(path).is_ok());
    }

    #[test]
    fn test_validate_file_path_rejects_traversal() {
        let path = Path::new("/music/../../../etc/passwd");
        let result = validate_file_path(path, &[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            PathValidationError::DirectoryTraversal(p) => {
                assert!(p.contains(".."));
            }
            other => panic!("expected DirectoryTraversal, got: {}", other),
        }
    }

    #[test]
    fn test_validate_file_path_rejects_null_byte() {
        let path = Path::new("/music/file.mp3\0.exe");
        let result = validate_file_path(path, &[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            PathValidationError::NullByte(p) => {
                assert!(p.contains('\0'));
            }
            other => panic!("expected NullByte, got: {}", other),
        }
    }

    #[test]
    fn test_validate_url_rejects_javascript() {
        let result = validate_url("javascript:alert(1)");
        assert!(result.is_err());
        match result.unwrap_err() {
            UrlValidationError::DangerousScheme { scheme, .. } => {
                assert_eq!(scheme, "javascript");
            }
            other => panic!("expected DangerousScheme, got: {}", other),
        }
    }

    #[test]
    fn test_validate_url_rejects_file() {
        let result = validate_url("file:///etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_url_accepts_https() {
        let result = validate_url("https://www.last.fm/api/auth/?api_key=test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_lastfm_auth_url_rejects_wrong_host() {
        let result = validate_lastfm_auth_url("https://evil.com/api/auth/?api_key=test");
        assert!(result.is_err());
        match result.unwrap_err() {
            UrlValidationError::UnexpectedHost { host, .. } => {
                assert_eq!(host, "evil.com");
            }
            other => panic!("expected UnexpectedHost, got: {}", other),
        }
    }

    #[test]
    fn test_validate_lastfm_auth_url_accepts_lastfm() {
        let result = validate_lastfm_auth_url("https://www.last.fm/api/auth/?api_key=test");
        assert!(result.is_ok());
    }
}
