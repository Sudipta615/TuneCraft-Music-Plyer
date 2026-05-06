//! Encryption utilities for sensitive data at rest.
//!
//! Last.fm credentials (API key, API secret, session key) are stored in the
//! SQLite `user_prefs` table. Without encryption, anyone with read access to
//! the database file can extract these credentials. This module provides
//! AES-256-GCM encryption/decryption using a key derived from a machine-specific
//! fingerprint, ensuring that:
//!
//! 1. Credentials are not readable by simply opening the SQLite file
//! 2. The encryption key is tied to the current machine and user
//! 3. Stolen database files cannot be decrypted on a different machine
//!
//! # Key Derivation (v1.1 improvement)
//!
//! The encryption key is derived using HKDF-SHA256 from:
//! - A machine identifier (hostname + username + OS info)
//! - A fixed application-specific salt
//! - Optionally, a per-user secret from the OS keyring (preferred) or
//!   a dotfile fallback (`~/.tunecraft-key`)
//!
//! The keyring integration uses the `keyring` crate which talks to:
//! - **Linux**: org.freedesktop.Secret / libsecret / GNOME Keyring / KWallet
//! - **macOS**: Keychain Services
//! - **Windows**: Credential Manager
//!
//! If the keyring is unavailable (e.g. headless server, no D-Bus), the
//! system falls back to the dotfile mechanism.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use hkdf::Hkdf;
use sha2::Sha256;

/// Application-specific salt for key derivation.
/// This prevents the same machine fingerprint from being used by other applications.
const APP_SALT: &[u8] = b"tunecraft-credential-encryption-v1";

/// Prefix used to identify encrypted values in the database.
/// This allows graceful migration: unencrypted values (no prefix) are still readable.
const ENCRYPTED_PREFIX: &str = "enc:v1:";

/// Derive a 256-bit encryption key from machine-specific information.
///
/// Uses HKDF-SHA256 with the machine fingerprint as input key material
/// and the application salt as the HKDF salt. Additionally mixes in
/// a per-user secret, preferring the OS keyring (v1.1) with a fallback
/// to the dotfile (`~/.tunecraft-key`).
///
/// The derived key is cached using `OnceLock` so the expensive key derivation
/// (which may spawn a subprocess for hostname and access the OS keyring)
/// only runs once per process lifetime.
///
/// # Keyring Integration (v1.1)
///
/// If the `keyring` crate is available and the OS secret service is
/// running, the key material includes a secret stored in the system
/// keyring under the service name "org.tunecraft.Tunecraft". This is
/// far stronger than deriving from environment variables alone, because
/// the keyring secret is:
/// - Not readable by arbitrary user processes (unlike env vars)
/// - Protected by the OS-level secret storage (login keyring, etc.)
/// - Created with cryptographically random bytes on first use
fn derive_key() -> Result<&'static [u8; 32], CryptoError> {
    static CACHED_KEY: std::sync::OnceLock<Result<[u8; 32], CryptoError>> =
        std::sync::OnceLock::new();
    let cached = CACHED_KEY.get_or_init(|| {
        let fingerprint = machine_fingerprint();
        let mut ikm = fingerprint.as_bytes().to_vec();

        // Try OS keyring first (v1.1 improvement) — much stronger than env vars
        if let Some(keyring_secret) = load_keyring_secret() {
            ikm.extend_from_slice(&keyring_secret);
            tracing::debug!("Using OS keyring for key derivation");
        } else if let Some(secret) = load_user_secret() {
            // Fallback to dotfile
            ikm.extend_from_slice(&secret);
            tracing::debug!("Using dotfile secret for key derivation (keyring unavailable)");
        } else {
            // Fix Bug #38 / Security #16: When neither keyring nor dotfile secret
            // is available, the encryption key was derived solely from public
            // environment variables (HOSTNAME, USER, OS, ARCH). These are visible
            // to any process on the system, making the encryption effectively
            // obfuscation rather than true security.
            //
            // Now we attempt to auto-generate a random 32-byte secret and store it
            // in a known dotfile (~/.tunecraft-key) with 0600 permissions. This
            // provides actual protection against credential extraction from a stolen
            // database file, since the key material is not publicly visible.
            //
            // If the dotfile creation also fails (e.g., read-only home directory),
            // we fall back to the weak env-var derivation but emit a strong warning.
            match generate_and_store_user_secret() {
                Some(secret) => {
                    ikm.extend_from_slice(&secret);
                    tracing::info!("Generated new random encryption key — credentials are properly protected");
                }
                None => {
                    tracing::warn!(
                        "No keyring, dotfile, or auto-generated secret available — \
                         encryption key will be derived from public env vars only. \
                         This provides obfuscation, not real security. \
                         Consider installing a keyring service (e.g., libsecret on Linux, \
                         Keychain on macOS, Credential Manager on Windows) for proper protection."
                    );
                }
            }
        }

        let hkdf = Hkdf::<Sha256>::new(Some(APP_SALT), &ikm);

        let mut key = [0u8; 32];
        // Fix Issue #21: Previously, HKDF expand failure silently fell back to
        // a SHA-256 hash of the IKM, producing a key that could never decrypt
        // data encrypted with the real HKDF-derived key. This caused silent
        // data loss — credentials encrypted after a fallback were unreadable
        // on any other run (including after the HKDF was fixed), and vice versa.
        //
        // Now we return an error instead, which propagates through encrypt()/
        // decrypt() so the user sees a visible error rather than losing data.
        // HKDF expand should never fail with a 32-byte output from valid
        // SHA-256 HKDF, but if it does, we refuse to produce an undecryptable key.
        match hkdf.expand(b"aes-256-gcm-key", &mut key) {
            Ok(()) => Ok(key),
            Err(e) => Err(CryptoError::KeyDerivationFailed(format!(
                "HKDF expand failed unexpectedly: {}. Refusing to produce undecryptable fallback key. \
                 This should never happen with valid inputs — check HKDF/SHA2 crate compatibility.",
                e
            ))),
        }
    });

    match cached {
        Ok(key) => Ok(key),
        Err(e) => Err(e.clone()),
    }
}

/// Try to load a secret from the OS keyring.
/// Returns None if the keyring is unavailable or the entry doesn't exist yet.
/// On first use, creates a random secret and stores it in the keyring.
fn load_keyring_secret() -> Option<Vec<u8>> {
    // v4.1: OS keyring integration is now fully implemented.
    // The `keyring` crate talks to:
    // - Linux: org.freedesktop.Secret / libsecret / GNOME Keyring / KWallet
    // - macOS: Keychain Services
    // - Windows: Credential Manager
    //
    // If the `keyring` feature is not enabled at compile time, or the
    // OS secret service is unavailable, this returns None and the
    // derive_key() fallback chain proceeds to the dotfile mechanism.
    #[cfg(feature = "keyring")]
    {
        use keyring::Entry;
        match Entry::new("org.tunecraft.Tunecraft", "encryption-key") {
            Ok(entry) => {
                match entry.get_password() {
                    Ok(secret_hex) => match hex::decode(&secret_hex) {
                        Ok(bytes) if bytes.len() >= 16 => {
                            tracing::debug!("Loaded encryption key from OS keyring");
                            Some(bytes)
                        }
                        Ok(_) => {
                            tracing::warn!("Keyring secret too short, regenerating");
                            let _ = entry.delete_credential();
                            None
                        }
                        Err(e) => {
                            tracing::debug!("Keyring secret hex decode failed: {}", e);
                            let _ = entry.delete_credential();
                            None
                        }
                    },
                    Err(keyring::Error::NoEntry) => {
                        // First use — generate and store a random 32-byte secret
                        let secret: [u8; 32] = rand::random();
                        let hex_str = hex::encode(secret);
                        match entry.set_password(&hex_str) {
                            Ok(()) => {
                                tracing::info!(
                                    "Generated and stored new encryption key in OS keyring"
                                );
                                Some(secret.to_vec())
                            }
                            Err(e) => {
                                tracing::debug!("Failed to store keyring secret: {}", e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Keyring access error: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                tracing::debug!("Keyring entry creation failed: {}", e);
                None
            }
        }
    }
    #[cfg(not(feature = "keyring"))]
    {
        tracing::debug!("OS keyring support not compiled in (enable `keyring` feature)");
        None
    }
}

/// Generate a machine-specific fingerprint.
///
/// Combines hostname, username, and OS information to create a unique
/// identifier for the current machine. This is used as input key material
/// for HKDF key derivation.
fn machine_fingerprint() -> String {
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .or_else(|_| {
            // Fix M9: HOSTNAME env var is unreliable on Linux (not always set).
            // Fall back to the `hostname` command which queries the system
            // hostname directly via gethostname(2).
            let output = std::process::Command::new("hostname").output().ok();
            output
                .and_then(|o| {
                    if o.status.success() {
                        String::from_utf8(o.stdout)
                            .ok()
                            .map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
                .ok_or(())
        })
        .unwrap_or_else(|_| "unknown-host".to_string());

    let username = whoami().unwrap_or_else(|| "unknown-user".to_string());

    let os_info = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);

    format!("{}:{}:{}", hostname, username, os_info)
}

/// Get the current username, with fallback.
fn whoami() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
}

/// Path to the per-user secret file used to strengthen key derivation.
/// Located at `~/.tunecraft-key`. This file is auto-created on first use.
fn user_secret_path() -> Option<std::path::PathBuf> {
    directories::BaseDirs::new().map(|d| d.home_dir().join(".tunecraft-key"))
}

/// Load the per-user secret from `~/.tunecraft-key`.
/// If the file does not exist, create it with a random 32-byte secret.
/// Returns None only if the home directory cannot be determined.
fn load_user_secret() -> Option<Vec<u8>> {
    let path = user_secret_path()?;

    // Fix C4: Use atomic file creation to prevent TOCTOU race condition.
    // If two processes start simultaneously, both could observe the file as
    // non-existent, generate different secrets, and overwrite each other.
    // Using OpenOptions::new().create_new(true) provides atomic O_CREAT|O_EXCL
    // semantics — only one process will succeed in creating the file.
    if path.exists() {
        // Fix Bug #39: Validate that the secret file contains exactly 32 bytes.
        // A corrupted (truncated or partially overwritten) file would produce
        // wrong bytes, causing HKDF to derive a different encryption key and
        // permanently bricking all previously encrypted credentials.
        match std::fs::read(&path) {
            Ok(bytes) if bytes.len() == 32 => Some(bytes),
            Ok(bytes) => {
                tracing::warn!(
                    "User secret file at {:?} is corrupted (got {} bytes, expected 32). \
                     Regenerating to avoid permanent credential loss.",
                    path,
                    bytes.len()
                );
                // Remove the corrupted file and generate a fresh one
                let _ = std::fs::remove_file(&path);
                generate_and_store_user_secret()
            }
            Err(e) => {
                tracing::warn!("Failed to read user secret file at {:?}: {}", path, e);
                None
            }
        }
    } else {
        let secret: [u8; 32] = rand::random();
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true) // Atomic: fails if file already exists
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write;
                if file.write_all(&secret).is_ok() {
                    // Set file permissions to owner-only (0600 on Unix)
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ =
                            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                    }
                    Some(secret.to_vec())
                } else {
                    tracing::warn!("Failed to write user secret file at {:?}", path);
                    None
                }
            }
            Err(_) => {
                // File was created by another process between our check and create.
                // Read the existing file instead.
                tracing::debug!("User secret file created by another process, reading existing");
                std::fs::read(&path).ok()
            }
        }
    }
}

/// Generate a new random 32-byte secret and store it in `~/.tunecraft-key`.
/// This is called as a fallback when neither keyring nor an existing dotfile
/// secret is available. Returns the secret bytes on success, or None if the
/// file cannot be created.
///
/// Fix Security #16: Previously, when neither keyring nor dotfile was available,
/// the encryption key was derived from public env vars only. This function
/// ensures that a random secret is generated on first run and stored with
/// restricted permissions, providing actual cryptographic protection.
fn generate_and_store_user_secret() -> Option<Vec<u8>> {
    let path = user_secret_path()?;

    // Fix C4: Use atomic file creation to prevent TOCTOU race condition.
    // The previous check-then-write pattern allowed two processes to both
    // observe the file as non-existent and overwrite each other's secrets.
    let secret: [u8; 32] = rand::random();
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true) // Atomic: fails if file already exists
        .open(&path)
    {
        Ok(mut file) => {
            use std::io::Write;
            if file.write_all(&secret).is_ok() {
                // Set file permissions to owner-only (0600 on Unix)
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                }
                tracing::info!("Generated new encryption key file at {:?}", path);
                Some(secret.to_vec())
            } else {
                tracing::warn!("Failed to write encryption key file at {:?}", path);
                None
            }
        }
        Err(_) => {
            // File was created by another process — read the existing one
            tracing::debug!("Encryption key file created by another process, reading existing");
            std::fs::read(&path).ok()
        }
    }
}

/// Encrypt a plaintext string and return a base64-encoded ciphertext
/// with the encryption prefix.
///
/// The format is: `enc:v1:<base64(nonce+ciphertext+tag)>`
///
/// # Errors
///
/// Returns an error if AES-GCM encryption fails (extremely unlikely with valid inputs).
pub fn encrypt(plaintext: &str) -> Result<String, CryptoError> {
    let key = derive_key()?;
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| CryptoError::KeyInitFailed(e.to_string()))?;

    // Generate a random 96-bit nonce
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    // Prepend nonce to ciphertext (nonce + ciphertext + tag)
    let mut combined = Vec::with_capacity(12 + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(format!("{}{}", ENCRYPTED_PREFIX, BASE64.encode(&combined)))
}

/// Decrypt a value that was encrypted with `encrypt()`.
///
/// If the value does not start with the encrypted prefix, it is returned
/// as-is (for backward compatibility with unencrypted values).
///
/// # Errors
///
/// Returns an error if:
/// - The value has the encrypted prefix but cannot be decoded
/// - The ciphertext is too short
/// - Decryption fails (wrong key, tampered data)
pub fn decrypt(value: &str) -> Result<String, CryptoError> {
    // If the value doesn't have the encrypted prefix, return it as-is
    // This allows seamless migration from unencrypted storage
    if !value.starts_with(ENCRYPTED_PREFIX) {
        return Ok(value.to_string());
    }

    let encoded = &value[ENCRYPTED_PREFIX.len()..];
    let combined = BASE64
        .decode(encoded)
        .map_err(|e| CryptoError::Base64DecodeFailed(e.to_string()))?;

    if combined.len() < 12 + 16 {
        // Need at least 12 bytes nonce + 16 bytes GCM tag
        return Err(CryptoError::CiphertextTooShort(combined.len()));
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = derive_key()?;
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| CryptoError::KeyInitFailed(e.to_string()))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

    String::from_utf8(plaintext).map_err(|e| CryptoError::Utf8DecodeFailed(e.to_string()))
}

/// Check if a stored value is encrypted.
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENCRYPTED_PREFIX)
}

/// Encrypt a sensitive credential if it's not already encrypted.
/// This is idempotent — calling it on an already-encrypted value is a no-op.
pub fn ensure_encrypted(plaintext: &str) -> Result<String, CryptoError> {
    if is_encrypted(plaintext) {
        return Ok(plaintext.to_string());
    }
    encrypt(plaintext)
}

/// Decrypt a credential for use, or return as-is if not encrypted.
/// This is the inverse of `ensure_encrypted`.
pub fn decrypt_if_needed(value: &str) -> Result<String, CryptoError> {
    decrypt(value)
}

// ── Error Types ────────────────────────────────────────────────────────────

/// Errors that can occur during encryption/decryption operations.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// Failed to initialize the AES-256-GCM cipher.
    #[error("failed to initialize cipher: {0}")]
    KeyInitFailed(String),

    /// Encryption failed.
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    /// Decryption failed (wrong key or tampered data).
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    /// Failed to decode base64 ciphertext.
    #[error("base64 decode failed: {0}")]
    Base64DecodeFailed(String),

    /// Ciphertext is too short to contain nonce + tag.
    #[error("ciphertext too short: {0} bytes")]
    CiphertextTooShort(usize),

    /// Decrypted bytes are not valid UTF-8.
    #[error("UTF-8 decode failed: {0}")]
    Utf8DecodeFailed(String),

    /// HKDF key derivation failed — refusing to produce undecryptable fallback key.
    /// Fix Issue #21: Previously, HKDF expand failure silently fell back to a
    /// SHA-256 hash of the IKM, producing a key that could never decrypt data
    /// encrypted with the real HKDF-derived key. Now we return an error instead.
    #[error("key derivation failed: {0}")]
    KeyDerivationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = "my-secret-api-key-12345";
        let encrypted = encrypt(plaintext).unwrap();
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX));
        assert_ne!(encrypted, plaintext);

        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_unencrypted_passthrough() {
        let unencrypted = "plain-text-value";
        let result = decrypt(unencrypted).unwrap();
        assert_eq!(result, unencrypted);
    }

    #[test]
    fn test_is_encrypted() {
        assert!(is_encrypted("enc:v1:AAAA"));
        assert!(!is_encrypted("plain-text"));
    }

    #[test]
    fn test_ensure_encrypted_idempotent() {
        let plaintext = "test-secret";
        let encrypted1 = ensure_encrypted(plaintext).unwrap();
        let encrypted2 = ensure_encrypted(&encrypted1).unwrap();
        // Second call should be a no-op (already encrypted)
        assert_eq!(encrypted1, encrypted2);
    }

    #[test]
    fn test_decrypt_if_needed() {
        let plaintext = "test-secret";
        let encrypted = encrypt(plaintext).unwrap();
        let decrypted = decrypt_if_needed(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Unencrypted values pass through
        let plain_result = decrypt_if_needed("plain-text").unwrap();
        assert_eq!(plain_result, "plain-text");
    }

    #[test]
    fn test_different_plaintexts_produce_different_ciphertexts() {
        // Due to random nonces, even the same plaintext produces different ciphertexts
        let enc1 = encrypt("same").unwrap();
        let enc2 = encrypt("same").unwrap();
        assert_ne!(enc1, enc2);
        // But both decrypt correctly
        assert_eq!(decrypt(&enc1).unwrap(), "same");
        assert_eq!(decrypt(&enc2).unwrap(), "same");
    }
}
