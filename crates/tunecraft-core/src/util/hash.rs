use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Compute SHA-256 hash of a file using streaming to avoid loading
/// the entire file into memory. This is safe for large audio files
/// (FLAC, WAV, etc.) that can be hundreds of megabytes.
pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)
        .context(format!("failed to open {:?}", path))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)
            .context(format!("failed to read from {:?}", path))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}
