//! Structured error types for the TuneCraft core library.
//!
//! Each major subsystem defines its own error enum using `thiserror`,
//! enabling callers to match on specific error variants for targeted
//! recovery strategies instead of string matching on anyhow messages.

// ── Audio Errors ────────────────────────────────────────────────────────

/// Errors that can occur during audio playback operations.
///
/// **Note:** The new three-thread audio engine (v0.9.0+) uses `anyhow::Result`
/// internally instead of this enum. `AudioError` is retained for backward
/// compatibility with external consumers and may be deprecated in a future
/// release. New code should prefer `anyhow::Error` for audio-related errors.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// No track is currently loaded in the audio engine.
    #[error("no track loaded")]
    NoTrackLoaded,

    /// Failed to initialize the GStreamer pipeline.
    #[error("pipeline initialization failed: {0}")]
    PipelineInitFailed(String),

    /// Failed to set the GStreamer pipeline state.
    #[error("failed to set pipeline state: {0}")]
    StateChangeFailed(String),

    /// Failed to open or decode an audio file.
    #[error("failed to open audio file: {0}")]
    FileOpenFailed(String),

    /// The audio format is not supported.
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),

    /// No default audio track found in the file.
    #[error("no default track in audio file")]
    NoDefaultTrack,

    /// Failed to create the audio decoder.
    #[error("failed to create decoder: {0}")]
    DecoderCreationFailed(String),

    /// GStreamer element creation failed.
    #[error("failed to create GStreamer element: {0}")]
    ElementCreationFailed(String),

    /// A GStreamer pad operation failed.
    #[error("GStreamer pad error: {0}")]
    PadError(String),

    /// Seek operation failed.
    #[error("seek failed: {0}")]
    SeekFailed(String),

    /// Crossfade engine error.
    #[error("crossfade error: {0}")]
    CrossfadeError(String),

    /// A mutex was poisoned (another thread panicked while holding the lock).
    /// The inner value is recovered via `into_inner()`.
    #[error("mutex poisoned (recovered)")]
    MutexPoisoned,

    /// An IO error occurred during audio operations.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A GStreamer error occurred.
    #[error("GStreamer error: {0}")]
    GStreamer(String),
}

// ── Database Errors ─────────────────────────────────────────────────────

/// Errors that can occur during database operations.
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    /// Failed to open a connection to the database.
    #[error("failed to open database: {0}")]
    ConnectionFailed(String),

    /// Failed to get a connection from the pool.
    #[error("failed to get connection from pool: {0}")]
    PoolError(String),

    /// Failed to run a database migration.
    #[error("migration failed at version {version}: {message}")]
    MigrationFailed { version: i64, message: String },

    /// A query execution failed.
    #[error("query execution failed: {0}")]
    QueryFailed(String),

    /// A constraint violation occurred (e.g., unique key conflict).
    #[error("constraint violation: {0}")]
    ConstraintViolation(String),

    /// Failed to find the project data directory.
    #[error("failed to determine project directories: {0}")]
    DirectoryError(String),

    /// Failed to open the database file.
    #[error("failed to open database file '{path}': {source}")]
    FileOpenFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// A rusqlite error occurred.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// An r2d2 pool error occurred.
    #[error("connection pool error: {0}")]
    Pool(#[from] r2d2::Error),
}

// ── Scrobbler Errors ────────────────────────────────────────────────────

/// Errors that can occur during Last.fm scrobbling operations.
#[derive(Debug, thiserror::Error)]
pub enum ScrobblerError {
    /// Last.fm API returned an error response.
    #[error("Last.fm API error {code}: {message}")]
    ApiError { code: i64, message: String },

    /// No session key is configured for authenticated requests.
    #[error("not authenticated with Last.fm")]
    NotAuthenticated,

    /// Failed to send HTTP request to Last.fm.
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    /// Failed to parse the Last.fm API response.
    #[error("failed to parse Last.fm response: {0}")]
    ParseError(String),

    /// No session key found in the authentication response.
    #[error("no session key in authentication response")]
    NoSessionKey,

    /// Failed to validate the Last.fm auth URL.
    #[error("auth URL validation failed: {0}")]
    UrlValidation(String),

    /// Failed to encrypt/decrypt credentials.
    #[error("credential encryption error: {0}")]
    CryptoError(String),

    /// A network request error occurred.
    #[cfg(any(feature = "lastfm", feature = "lyrics"))]
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
}

// ── Lyrics Errors ───────────────────────────────────────────────────────

/// Errors that can occur during lyrics fetching operations.
#[derive(Debug, thiserror::Error)]
pub enum LyricsError {
    /// The LRCLIB API returned a non-success HTTP status.
    #[error("LRCLIB returned HTTP {status}: {body}")]
    HttpError { status: u16, body: String },

    /// Failed to send request to the LRCLIB API.
    #[error("failed to fetch lyrics from LRCLIB: {0}")]
    RequestFailed(String),

    /// Failed to parse the LRCLIB API response.
    #[error("failed to parse LRCLIB response: {0}")]
    ParseError(String),

    /// A network error occurred.
    #[cfg(feature = "lyrics")]
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
}

// ── Playlist I/O Errors ─────────────────────────────────────────────────

/// Errors that can occur during playlist import/export operations.
#[derive(Debug, thiserror::Error)]
pub enum PlaylistError {
    /// The playlist file extension is not recognized.
    #[error("unrecognised playlist extension for {path}. Supported: .m3u, .m3u8, .xspf")]
    UnrecognisedFormat { path: String },

    /// Failed to read the playlist file.
    #[error("failed to read playlist file '{path}': {source}")]
    ReadFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Failed to write the playlist file.
    #[error("failed to write playlist file '{path}': {source}")]
    WriteFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// The playlist path has no parent directory.
    #[error("playlist path has no parent directory")]
    NoParentDirectory,

    /// A path in the playlist failed validation.
    #[error("invalid path in playlist: {path} — {reason}")]
    InvalidPath { path: String, reason: String },
}

// ── Mood Analysis Errors ────────────────────────────────────────────────

/// Errors that can occur during mood analysis operations.
#[derive(Debug, thiserror::Error)]
pub enum MoodError {
    /// Failed to open the audio file for decoding.
    #[error("failed to open audio file: {0}")]
    FileOpenFailed(String),

    /// Failed to probe the audio format.
    #[error("failed to probe audio format: {0}")]
    ProbeFailed(String),

    /// No default audio track found.
    #[error("no default track in audio file")]
    NoDefaultTrack,

    /// Unknown sample rate in the audio file.
    #[error("unknown sample rate")]
    UnknownSampleRate,

    /// Failed to create the audio decoder.
    #[error("failed to create decoder: {0}")]
    DecoderFailed(String),

    /// No audio samples were decoded from the file.
    #[error("no audio samples decoded")]
    NoSamples,
}

/// Convert MoodError to String for backward compatibility with existing callers.
impl From<MoodError> for String {
    fn from(err: MoodError) -> String {
        err.to_string()
    }
}
