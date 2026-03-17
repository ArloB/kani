//! File storage utilities for WASM files.

use std::path::PathBuf;
use tokio::fs;

use crate::error::{Error, Result};

/// WASM magic bytes: \0asm
const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D];

/// Saves WASM bytes to disk, creating directories as needed.
/// Returns the full path where the file was saved.
pub async fn save_wasm(wasm_storage_path: &str, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let name = crate::utilities::sanitize_filename(name);

    if !validate_wasm_magic(bytes) {
        return Err(Error::InvalidWasm);
    }

    let dir = PathBuf::from(wasm_storage_path);
    fs::create_dir_all(&dir).await?;

    let filename = format!("{}.wasm", name);
    let path = dir.join(&filename);

    let canonical_dir = dir.canonicalize()?;

    if let Some(parent) = path.parent()
        && parent.canonicalize()? != canonical_dir
    {
        return Err(Error::PathTraversal(name));
    }

    fs::write(&path, bytes).await?;
    Ok(path)
}

/// Deletes a WASM file at the given path.
pub async fn delete_wasm_file(wasm_storage_path: &str, name: &str) -> Result<()> {
    let name = crate::utilities::sanitize_filename(name);
    let dir = PathBuf::from(wasm_storage_path);
    let filename = format!("{}.wasm", name);
    let path = dir.join(&filename);

    if path.exists() {
        let canonical_dir = dir.canonicalize()?;

        if let Some(parent) = path.parent()
            && parent.canonicalize()? != canonical_dir
        {
            return Err(Error::PathTraversal(name));
        }

        fs::remove_file(&path).await?;
    }
    Ok(())
}

/// Validates that the bytes start with WASM magic bytes.
pub fn validate_wasm_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..4] == WASM_MAGIC
}
