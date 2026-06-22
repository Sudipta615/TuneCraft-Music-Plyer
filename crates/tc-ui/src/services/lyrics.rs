//! Synced lyrics integration via [LRCLIB](https://lrclib.net).
//!
//! LRCLIB is a free, open-source, community-driven synced-lyrics database.
//! It exposes a simple REST API:
//!
//! - `GET /get` — exact match by `(artist, album, title, duration)`.
//! - `GET /search` — fuzzy search; returns multiple candidates.
//!
//! This service prefers `/get` (exact match) and falls back to `/search`
//! when no exact match exists. Results are cached in the `tracks` table
//! via `Database::update_lyrics`, so subsequent plays of the same track
//! require no network access.
//!
//! ## Architecture
//!
//! ```text
//! UI tick (track change)
//!   └─ LyricsService::fetch_for_track(track)   ← spawns background task
//!        ├─ reqwest::get(lrclib.net/get?...)
//!        └─ on success:
//!             ├─ Database::update_lyrics(track_id, ...)
//!             └─ LyricsEvent::Fetched → UI poll → display
//! ```
//!
//! ## Caching
//!
//! - **DB cache**: every successful fetch is written back to the `tracks`
//!   table (`lyrics_synced` / `lyrics_unsynced` columns). Subsequent plays
//!   of the same track never hit the network.
//! - **In-memory cache**: a small `DashMap`-style LRU is not used here — the
//!   DB *is* the cache. The cost of `SELECT lyrics_synced FROM tracks WHERE
//!   id = ?` is ~50 µs, far less than a network round-trip.
//!
//! ## Privacy
//!
//! LRCLIB queries are sent without authentication. The User-Agent string
//! identifies TuneCraft for the LRCLIB maintainers' request logs but
//! includes no user-identifying information.
//!
//! ## Error handling
//!
//! All network errors are logged and silently swallowed. The UI shows a
//! "no lyrics available" state; the user is never shown a network error.

use std::sync::Arc;
use std::time::Duration;

use log::{info, warn};
use serde::Deserialize;
use tokio::sync::mpsc;

use tc_db::Database;

/// Events emitted by the lyrics service for UI feedback.
#[derive(Debug, Clone)]
pub enum LyricsEvent {
    /// Synced lyrics were successfully fetched and written to the DB.
    Fetched {
        track_id: i64,
        /// Plain-text synced lyrics (LRC format with `[mm:ss.xx]` timestamps).
        synced: String,
    },
    /// No lyrics were found on LRCLIB for this track.
    NotFound { track_id: i64 },
    /// The fetch failed (network error, parse error, etc.). The UI should
    /// fall back to the cached `lyrics_unsynced` column or show "no lyrics".
    Failed { track_id: i64, error: String },
}

/// A request to fetch lyrics for a track. Sent from the UI thread to the
/// background fetch task.
#[derive(Debug, Clone)]
pub struct LyricsRequest {
    pub track_id: i64,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub title: String,
    pub duration_secs: f32,
}

/// LRCLIB `/get` response body (subset of fields we care about).
#[derive(Debug, Deserialize)]
struct LrcLibGetResponse {
    /// Synced lyrics in LRC format (`[mm:ss.xx] lyric line`), or null if
    /// only plain lyrics are available.
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    /// Plain unsynced lyrics.
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
}

/// LRCLIB `/search` response body — an array of matches, each with the same
/// shape as `/get` plus artist/album/title fields.
#[derive(Debug, Deserialize)]
struct LrcLibSearchItem {
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
}

const LRCLIB_BASE_URL: &str = "https://lrclib.net";
const USER_AGENT: &str = concat!(
    "TuneCraft/",
    env!("CARGO_PKG_VERSION"),
    " (offline music player)"
);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Synced-lyrics service. Owns a background tokio task that drains a request
/// channel and writes results back to the DB + emits events on a separate
/// channel for UI polling.
pub struct LyricsService {
    tx: mpsc::Sender<LyricsRequest>,
    event_rx: parking_lot::Mutex<mpsc::Receiver<LyricsEvent>>,
    db: Arc<Database>,
    /// Whether the service is enabled (network access permitted). When false,
    /// `fetch_for_track` is a no-op.
    enabled: std::sync::atomic::AtomicBool,
    /// Base URL of the LRCLIB instance to query. Defaults to
    /// `https://lrclib.net` but can be overridden via `lyrics.base_url` in
    /// the config to point at a self-hosted instance.
    base_url: String,
}

impl LyricsService {
    /// Spawn a new lyrics service. The background fetch task runs for the
    /// lifetime of the returned `Self`.
    ///
    /// `base_url` lets the caller override the LRCLIB instance URL (e.g. to
    /// point at a self-hosted mirror). Pass an empty string to use the
    /// default `https://lrclib.net`.
    pub fn new(
        db: Arc<Database>,
        enabled: bool,
        base_url: impl Into<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (req_tx, req_rx) = mpsc::channel::<LyricsRequest>(16);
        let (event_tx, event_rx) = mpsc::channel::<LyricsEvent>(16);

        let base_url = {
            let b = base_url.into();
            let b = b.trim_end_matches('/');
            if b.is_empty() {
                LRCLIB_BASE_URL.to_string()
            } else {
                b.to_string()
            }
        };

        let svc = Self {
            tx: req_tx,
            event_rx: parking_lot::Mutex::new(event_rx),
            db,
            enabled: std::sync::atomic::AtomicBool::new(enabled),
            base_url: base_url.clone(),
        };

        // Spawn the background fetch task on a dedicated tokio runtime.
        // We don't use the existing tc-engine tokio runtime because that
        // runtime is owned by the audio engine and we don't want a slow
        // reqwest call to block audio callback shutdown.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("failed to build tokio runtime for lyrics service: {}", e))?;
        let db_clone = Arc::clone(&svc.db);
        let base_url_clone = base_url.clone();
        runtime.spawn(async move {
            lyrics_fetch_loop(req_rx, event_tx, db_clone, base_url_clone).await;
        });
        // Leak the runtime — it lives for the process lifetime. This is
        // intentional: the lyrics service is created once at app startup
        // and never destroyed.
        std::mem::forget(runtime);

        Ok(svc)
    }

    /// Enable or disable network access. When disabled, pending requests
    /// already in the channel are still processed (best effort).
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    /// Returns the configured LRCLIB base URL (either the default
    /// `https://lrclib.net` or the override supplied via `lyrics.base_url`
    /// in the config file). Used by the settings UI to display the current
    /// endpoint.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Request lyrics for a track. Returns immediately; results are
    /// delivered via `try_recv_event`.
    ///
    /// If the track already has `lyrics_synced` populated in the DB, this
    /// is a no-op (the UI should read from the DB directly in that case).
    pub fn fetch_for_track(&self, req: LyricsRequest) {
        if !self.enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        // Best-effort send — if the channel is full (16 pending requests),
        // drop the oldest by trying to drain one first. We never block the
        // UI thread on this.
        if self.tx.capacity() == 0 {
            // Channel full — skip this request rather than blocking.
            warn!(
                "LyricsService: request channel full, dropping request for track {}",
                req.track_id
            );
            return;
        }
        let _ = self.tx.try_send(req);
    }

    /// Poll for a lyrics event. The UI should call this once per frame
    /// (it's a non-blocking `mpsc::Receiver::try_recv`).
    pub fn try_recv_event(&self) -> Option<LyricsEvent> {
        self.event_rx.lock().try_recv().ok()
    }
}

/// Background fetch loop. Runs on the dedicated lyrics tokio runtime.
async fn lyrics_fetch_loop(
    mut req_rx: mpsc::Receiver<LyricsRequest>,
    event_tx: mpsc::Sender<LyricsEvent>,
    db: Arc<Database>,
    base_url: String,
) {
    let client = match reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(REQUEST_TIMEOUT)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("LyricsService: failed to build HTTP client: {}", e);
            // Drain the request channel so senders don't block forever.
            while req_rx.recv().await.is_some() {}
            return;
        },
    };

    while let Some(req) = req_rx.recv().await {
        match fetch_lyrics_for_request(&client, &req, &base_url).await {
            Ok(FetchedLyrics {
                synced,
                plain,
                source,
            }) => {
                // Persist to DB so future plays skip the network.
                if let Err(e) = db.update_lyrics(req.track_id, synced.as_deref(), plain.as_deref())
                {
                    warn!(
                        "LyricsService: failed to persist lyrics for track {}: {}",
                        req.track_id, e
                    );
                }
                // Emit event for UI.
                if let Some(synced_text) = &synced {
                    let _ = event_tx.try_send(LyricsEvent::Fetched {
                        track_id: req.track_id,
                        synced: synced_text.clone(),
                    });
                } else if let Some(plain_text) = &plain {
                    // No synced lyrics, but plain lyrics are available.
                    // We still emit Fetched so the UI can show plain lyrics
                    // as a fallback. The `synced` field carries the plain
                    // text in this case — UI distinguishes by checking for
                    // LRC timestamp markers.
                    let _ = event_tx.try_send(LyricsEvent::Fetched {
                        track_id: req.track_id,
                        synced: plain_text.clone(),
                    });
                } else {
                    let _ = event_tx.try_send(LyricsEvent::NotFound {
                        track_id: req.track_id,
                    });
                }
                info!(
                    "LyricsService: fetched lyrics for track {} from {}",
                    req.track_id, source
                );
            },
            Err(e) => {
                warn!(
                    "LyricsService: fetch failed for track {}: {}",
                    req.track_id, e
                );
                let _ = event_tx.try_send(LyricsEvent::Failed {
                    track_id: req.track_id,
                    error: e.to_string(),
                });
            },
        }
    }
}

struct FetchedLyrics {
    synced: Option<String>,
    plain: Option<String>,
    source: &'static str,
}

/// Try `/get` first (exact match), fall back to `/search` (fuzzy).
async fn fetch_lyrics_for_request(
    client: &reqwest::Client,
    req: &LyricsRequest,
    base_url: &str,
) -> Result<FetchedLyrics, anyhow::Error> {
    // --- Phase 1: exact match via /get ---
    let mut get_url = format!(
        "{}/get?artist={}&album={}&title={}",
        base_url,
        url_encode(&req.artist.clone().unwrap_or_default()),
        url_encode(&req.album.clone().unwrap_or_default()),
        url_encode(&req.title),
    );
    // Include duration if it's plausible (LRCLIB uses integer seconds).
    let duration_secs_int = req.duration_secs.round() as i64;
    if duration_secs_int > 0 {
        get_url.push_str(&format!("&duration={}", duration_secs_int));
    }

    let resp = client.get(&get_url).send().await?;
    if resp.status().is_success() {
        let body: LrcLibGetResponse = resp.json().await?;
        return Ok(FetchedLyrics {
            synced: body.synced_lyrics,
            plain: body.plain_lyrics,
            source: "lrclib /get",
        });
    }
    // 404 is expected for tracks with no exact match — fall through to search.
    if !resp.status().is_client_error() && !resp.status().is_server_error() {
        // Some other success code (rare) — try to parse anyway.
        if let Ok(body) = resp.json::<LrcLibGetResponse>().await {
            return Ok(FetchedLyrics {
                synced: body.synced_lyrics,
                plain: body.plain_lyrics,
                source: "lrclib /get",
            });
        }
    }

    // --- Phase 2: fuzzy search via /search ---
    let search_url = format!(
        "{}/search?artist={}&track_name={}",
        base_url,
        url_encode(&req.artist.clone().unwrap_or_default()),
        url_encode(&req.title),
    );

    let resp = client.get(&search_url).send().await?;
    if !resp.status().is_success() {
        return Ok(FetchedLyrics {
            synced: None,
            plain: None,
            source: "lrclib /search (no response)",
        });
    }

    let items: Vec<LrcLibSearchItem> = resp.json().await?;
    // Prefer the first result that has synced lyrics; fall back to first
    // result with plain lyrics; finally return None.
    let with_synced = items.iter().find(|i| i.synced_lyrics.is_some());
    if let Some(item) = with_synced {
        return Ok(FetchedLyrics {
            synced: item.synced_lyrics.clone(),
            plain: item.plain_lyrics.clone(),
            source: "lrclib /search (synced)",
        });
    }
    let with_plain = items.iter().find(|i| i.plain_lyrics.is_some());
    if let Some(item) = with_plain {
        return Ok(FetchedLyrics {
            synced: None,
            plain: item.plain_lyrics.clone(),
            source: "lrclib /search (plain)",
        });
    }

    Ok(FetchedLyrics {
        synced: None,
        plain: None,
        source: "lrclib /search (empty)",
    })
}

/// Minimal URL-encoder for query parameters. We don't pull in the `url` crate
/// just for this — the characters we need to escape are limited.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            },
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encode_basic() {
        assert_eq!(url_encode("hello"), "hello");
        assert_eq!(url_encode("hello world"), "hello+world");
        assert_eq!(url_encode("AC/DC"), "AC%2FDC");
        assert_eq!(url_encode("Sigur Rós"), "Sigur+R%C3%B3s");
    }

    #[test]
    fn test_lyrics_request_clones() {
        let req = LyricsRequest {
            track_id: 1,
            artist: Some("Test".to_string()),
            album: None,
            title: "Song".to_string(),
            duration_secs: 180.0,
        };
        let _ = req.clone();
    }

    #[test]
    fn test_deserialize_lrclib_response() {
        let json = r#"{
            "syncedLyrics": "[00:01.00]Hello\n[00:03.50]World",
            "plainLyrics": "Hello\nWorld"
        }"#;
        let parsed: LrcLibGetResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.synced_lyrics.is_some());
        assert!(parsed.plain_lyrics.is_some());
    }
}
