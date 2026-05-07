use anyhow::{Context, Result};
use md5::{Digest as _, Md5};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

use crate::util::crypto;
use crate::util::validation::{validate_lastfm_auth_url, UrlValidationError};

/// Last.fm scrobble client.
///
/// Fix Bug #68: Previously created a new `reqwest::Client` per request,
/// preventing connection pooling and TLS session reuse. Now stores a
/// persistent `reqwest::Client` instance that is reused across all requests.
///
/// Optimization #15: `#[derive(Clone)]` is intentionally kept. Cloning a
/// `LastfmClient` is cheap because `reqwest::Client` uses `Arc` internally —
/// the clone shares the same connection pool and TLS session cache. This is
/// the correct behavior for passing the client to multiple async tasks or
/// handler closures without creating redundant connection pools.
#[derive(Clone)]
pub struct LastfmClient {
    api_key: String,
    api_secret: String,
    session_key: Option<String>,
    base_url: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrobbleEntry {
    pub track: String,
    pub artist: String,
    pub album: Option<String>,
    pub timestamp: i64,
    pub duration: Option<u64>,
}

impl LastfmClient {
    /// Create a new Last.fm client.
    pub fn new(api_key: impl Into<String>, api_secret: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            api_secret: api_secret.into(),
            session_key: None,
            base_url: "https://ws.audioscrobbler.com/2.0/".to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Set the session key for authenticated requests.
    pub fn set_session_key(&mut self, key: impl Into<String>) {
        self.session_key = Some(key.into());
    }

    /// Generate a method signature as per the Last.fm API spec.
    /// The signature is the MD5 hash of all parameters sorted alphabetically
    /// (excluding the "api_sig" and "format" keys), concatenated with the api_secret.
    fn sign_params(&self, params: &serde_json::Map<String, serde_json::Value>) -> String {
        let mut keys: Vec<&String> = params
            .keys()
            .filter(|k| *k != "api_sig" && *k != "format")
            .collect();
        keys.sort();

        let mut sig_string = String::new();
        for key in keys {
            if let Some(value) = params.get(key) {
                sig_string.push_str(key);
                if let Some(s) = value.as_str() {
                    sig_string.push_str(s);
                } else if let Some(b) = value.as_bool() {
                    sig_string.push_str(if b { "1" } else { "0" });
                } else if let Some(n) = value.as_i64() {
                    sig_string.push_str(&n.to_string());
                } else if let Some(f) = value.as_f64() {
                    sig_string.push_str(&f.to_string());
                } else {
                    tracing::warn!(
                        "Skipping unexpected JSON value type in Last.fm signature for key '{}'",
                        key
                    );
                }
            }
        }
        sig_string.push_str(&self.api_secret);

        md5_hash(&sig_string)
    }

    /// Add a scrobble to the queue for later submission.
    pub fn queue_scrobble(
        &self,
        entry: &ScrobbleEntry,
        db: &crate::database::Database,
        track_id: i64,
    ) -> Result<()> {
        db.queue_scrobble(
            track_id,
            &entry.artist,
            &entry.track,
            entry.album.as_deref(),
            entry.timestamp,
        )?;
        info!(
            "Queued scrobble: {} - {} (ts={})",
            entry.artist, entry.track, entry.timestamp
        );
        Ok(())
    }

    /// Process queued scrobbles by submitting them to Last.fm.
    /// Returns a Vec<bool> indicating per-entry success (true = successfully scrobbled).
    /// The caller is responsible for marking entries as done in the database.
    ///
    /// Fix M5: Previously returned a single `success_count`, which prevented the
    /// caller from knowing which specific entries succeeded. When a batch had
    /// partial failures, ALL entries were marked as done. Now returns per-entry
    /// results so only truly successful entries are marked as done.
    pub async fn process_queue(&self, entries: Vec<ScrobbleEntry>) -> Result<Vec<bool>> {
        match &self.session_key {
            None => return Ok(Vec::new()),
            Some(key) if key.is_empty() => {
                warn!("Scrobble skipped: session key is empty — re-authentication required");
                return Ok(Vec::new());
            }
            _ => {}
        }

        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let total = entries.len();
        let mut results = vec![false; total];
        let mut offset = 0usize;

        for chunk in entries.chunks(50) {
            let mut params = serde_json::Map::new();
            params.insert("method".to_string(), json!("track.scrobble"));
            params.insert("api_key".to_string(), json!(self.api_key));
            params.insert(
                "sk".to_string(),
                json!(self.session_key.as_deref().unwrap_or("")),
            );
            params.insert("format".to_string(), json!("json"));

            for (i, entry) in chunk.iter().enumerate() {
                params.insert(format!("track[{}]", i), json!(entry.track));
                params.insert(format!("artist[{}]", i), json!(entry.artist));
                if let Some(ref album) = entry.album {
                    params.insert(format!("album[{}]", i), json!(album));
                }
                params.insert(format!("timestamp[{}]", i), json!(entry.timestamp));
            }

            let api_sig = self.sign_params(&params);
            params.insert("api_sig".to_string(), json!(api_sig));

            match self.client.post(&self.base_url).form(&params).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let body_text = resp.text().await.unwrap_or_default();
                        let ignored_count: usize =
                            serde_json::from_str::<serde_json::Value>(&body_text)
                                .ok()
                                .and_then(|v| {
                                    let attr = v.get("scrobbles").and_then(|s| s.get("@attr"));
                                    attr.and_then(|a| a.get("ignored").cloned())
                                })
                                .and_then(|i| {
                                    i.as_str()
                                        .and_then(|s| s.parse().ok())
                                        .or_else(|| i.as_u64().map(|u| u as usize))
                                })
                                .unwrap_or(0);

                        if ignored_count > 0 {
                            warn!(
                                "Scrobble partial failure: {} out of {} ignored by Last.fm — \
                                 treating entire batch as failed because ignored indices \
                                 are unknown",
                                ignored_count,
                                chunk.len()
                            );
                        } else {
                            for i in 0..chunk.len() {
                                results[offset + i] = true;
                            }
                            info!("Scrobbled {} tracks successfully", chunk.len());
                        }
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        warn!("Scrobble request failed ({}): {}", status, body);
                    }
                }
                Err(e) => {
                    warn!("Scrobble request error: {}", e);
                }
            }
            offset += chunk.len();
        }

        Ok(results)
    }

    /// Check if the client is authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.session_key.is_some()
    }

    /// Convenience alias for [`is_authenticated`].
    ///
    /// Useful for callers that want a semantically natural check before
    /// attempting to scrobble, e.g. `if manager.client().is_ready() { … }`.
    pub fn is_ready(&self) -> bool {
        self.is_authenticated()
    }

    /// Get the API key.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Get a reference to the session key.
    pub fn session_key(&self) -> Option<&str> {
        self.session_key.as_deref()
    }

    /// Get the validated auth URL for user authorization.
    ///
    /// # Security
    ///
    /// The URL is validated before being returned to ensure:
    /// - It uses the HTTPS scheme
    /// - It points to the expected Last.fm domain
    /// - It does not contain dangerous schemes (javascript:, file:, data:)
    ///
    /// The API key is sanitized before URL construction to prevent injection
    /// of control characters or URL-breaking content via a compromised key.
    ///
    /// Returns the validated URL string, or an error if validation fails
    /// (which would indicate a compromised or malformed API key).
    pub fn get_auth_url(&self) -> Result<String, UrlValidationError> {
        let sanitized_key: String = self
            .api_key
            .chars()
            .filter(|c| !c.is_control() && !c.is_whitespace())
            .collect();

        let encoded_key = urlencoding::encode(&sanitized_key);

        let url = format!("https://www.last.fm/api/auth/?api_key={}", encoded_key);

        validate_lastfm_auth_url(&url)?;

        Ok(url)
    }

    /// Fetch a session key using an auth token.
    pub async fn get_session(&mut self, token: &str) -> Result<String> {
        let mut params = serde_json::Map::new();
        params.insert("method".to_string(), json!("auth.getSession"));
        params.insert("api_key".to_string(), json!(self.api_key));
        params.insert("token".to_string(), json!(token));
        params.insert("format".to_string(), json!("json"));

        let api_sig = self.sign_params(&params);
        params.insert("api_sig".to_string(), json!(api_sig));

        let resp = self
            .client
            .post(&self.base_url)
            .form(&params)
            .send()
            .await?;
        let body: serde_json::Value = resp.json().await?;

        if let Some(error) = body.get("error") {
            let message = body
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            anyhow::bail!("Last.fm auth error {}: {}", error, message);
        }

        let session_key = body
            .pointer("/session/key")
            .and_then(|v| v.as_str())
            .context("no session key in response")?
            .to_string();

        info!("Successfully authenticated with Last.fm");
        self.session_key = Some(session_key.clone());
        Ok(session_key)
    }
}

/// Encrypt a sensitive credential (API key, API secret, session key) for
/// storage in the database. Uses AES-256-GCM with a machine-derived key.
///
/// # Security
///
/// If encryption fails, this function returns an error instead of falling
/// back to plaintext storage. Callers must handle the error — typically by
/// showing a user-facing message and aborting the save operation.
pub fn encrypt_credential(plaintext: &str) -> Result<String, crate::util::crypto::CryptoError> {
    crate::util::crypto::encrypt(plaintext)
}

/// Decrypt a credential that may be encrypted or stored in plaintext.
/// Handles the `enc:v1:` prefix automatically, and passes through
/// unencrypted values for backward compatibility.
///
/// # Security
///
/// If decryption fails (e.g., different machine), returns an error.
/// The caller should treat this as a need to re-authenticate rather than
/// silently using the raw encrypted string as a credential.
pub fn decrypt_credential(value: &str) -> Result<String, crate::util::crypto::CryptoError> {
    crate::util::crypto::decrypt(value)
}

/// Check if a stored credential value is encrypted.
pub fn is_credential_encrypted(value: &str) -> bool {
    crypto::is_encrypted(value)
}

/// Compute MD5 hash using the well-tested md-5 crate.
/// Returns the hex-encoded hash string for Last.fm API signing.
///
/// # Security Note
///
/// MD5 is used **only** for Last.fm API request signing, as required by their
/// specification at <https://www.last.fm/api/desktopauth>. It is NOT used for:
/// - Password hashing
/// - Data integrity verification
/// - Any other security-critical purpose
///
/// For all other hashing needs (file fingerprinting, integrity checks, etc.),
/// SHA-256 is used instead (see `crate::util::hash::file_sha256`).
/// If the project ever needs to store hashes for integrity verification,
/// SHA-256 MUST be used, never MD5.
fn md5_hash(input: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md5_empty_string() {
        assert_eq!(md5_hash(""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_md5_hello_world() {
        assert_eq!(md5_hash("hello world"), "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }

    #[test]
    fn test_md5_lastfm_signature() {
        let client = LastfmClient::new("myapikey", "mysecret");
        let mut params = serde_json::Map::new();
        params.insert("method".to_string(), json!("auth.getSession"));
        params.insert("api_key".to_string(), json!("myapikey"));
        params.insert("token".to_string(), json!("mytoken"));

        let sig = client.sign_params(&params);
        assert_eq!(sig.len(), 32, "MD5 hash should be 32 hex chars");
        assert!(
            sig.chars().all(|c| c.is_ascii_hexdigit()),
            "signature should be hex"
        );
    }

    #[test]
    fn test_lastfm_client_creation() {
        let client = LastfmClient::new("key123", "secret456");
        assert_eq!(client.api_key(), "key123");
        assert!(!client.is_authenticated());
        assert_eq!(client.session_key(), None);
    }

    #[test]
    fn test_lastfm_auth_url() {
        let client = LastfmClient::new("testkey", "testsecret");
        let url = client.get_auth_url().unwrap();
        assert!(url.contains("testkey"));
        assert!(url.starts_with("https://www.last.fm/api/auth/"));
    }

    #[test]
    fn test_lastfm_set_session_key() {
        let mut client = LastfmClient::new("key", "secret");
        assert!(!client.is_authenticated());
        client.set_session_key("session123");
        assert!(client.is_authenticated());
        assert_eq!(client.session_key(), Some("session123"));
    }

    #[test]
    fn test_credential_encrypt_decrypt_roundtrip() {
        let plaintext = "my-super-secret-api-key";
        let encrypted = encrypt_credential(plaintext).unwrap();
        assert_ne!(encrypted, plaintext);
        assert!(is_credential_encrypted(&encrypted));

        let decrypted = decrypt_credential(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_unencrypted_credential_passthrough() {
        let plain = "plain-text-value";
        let result = decrypt_credential(plain).unwrap();
        assert_eq!(result, plain);
    }

    #[test]
    fn test_auth_url_validated() {
        let client = LastfmClient::new("validkey", "validsecret");
        let url = client.get_auth_url().unwrap();
        assert!(validate_lastfm_auth_url(&url).is_ok());
    }
}
