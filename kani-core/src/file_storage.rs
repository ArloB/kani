//! File storage utilities for WASM files.

use std::path::PathBuf;
use tokio::fs;

use crate::error::{Error, Result};

/// WASM magic bytes: \0asm
const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D];

/// Saves WASM bytes to disk, creating directories as needed.
/// Returns the full path where the file was saved.
pub async fn save_wasm(wasm_storage_path: &str, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    if !validate_wasm_magic(bytes) {
        return Err(Error::InvalidWasm);
    }

    let dir = PathBuf::from(wasm_storage_path);
    fs::create_dir_all(&dir).await?;

    let filename = format!("{}.wasm", name);
    let path = dir.join(&filename);

    fs::write(&path, bytes).await?;
    Ok(path)
}

/// Deletes a WASM file at the given path.
pub async fn delete_wasm_file(wasm_storage_path: &str, name: &str) -> Result<()> {
    let path = PathBuf::from(wasm_storage_path).join(format!("{}.wasm", name));
    if path.exists() {
        fs::remove_file(&path).await?;
    }
    Ok(())
}

/// Validates that the bytes start with WASM magic bytes.
pub fn validate_wasm_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..4] == WASM_MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_wasm_magic_valid() {
        let bytes = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        assert!(validate_wasm_magic(&bytes));
    }

    #[test]
    fn test_validate_wasm_magic_invalid() {
        let bytes = [0x00, 0x00, 0x00, 0x00];
        assert!(!validate_wasm_magic(&bytes));
    }

    #[test]
    fn test_validate_wasm_magic_too_short() {
        let bytes = [0x00, 0x61, 0x73];
        assert!(!validate_wasm_magic(&bytes));
    }
}
