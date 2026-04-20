//! CBZ archive reading utilities.

use std::path::Path;

use crate::error::{Error, Result};

fn is_image_entry(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
        || lower.ends_with(".avif")
}

/// Returns a sorted list of image entry names from a CBZ archive.
///
/// Entries are sorted lexicographically, which matches the `0001.jpg` naming
/// convention used by the downloader.
pub fn list_cbz_pages(path: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(path)
        .map_err(|_| Error::NotFound(format!("CBZ not found: {}", path.display())))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Internal(format!("Failed to open CBZ: {e}")))?;

    let mut names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_owned();
            if is_image_entry(&name) {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    names.sort();
    Ok(names)
}

/// Reads a specific page by sorted index from a CBZ archive.
///
/// Returns the raw image bytes and the lowercase file extension (without dot).
/// Opens the archive once, builds the sorted page list, then reads the entry.
pub fn read_cbz_page(path: &Path, page_num: usize) -> Result<(Vec<u8>, String)> {
    let file = std::fs::File::open(path)
        .map_err(|_| Error::NotFound(format!("CBZ not found: {}", path.display())))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Internal(format!("Failed to open CBZ: {e}")))?;

    let mut names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_owned();
            if is_image_entry(&name) {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    names.sort();

    let name = names
        .get(page_num)
        .ok_or_else(|| {
            Error::NotFound(format!(
                "Page {page_num} out of range ({} pages)",
                names.len()
            ))
        })?
        .clone();

    let ext = Path::new(&name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();

    let mut entry = archive
        .by_name(&name)
        .map_err(|_| Error::Internal(format!("Entry '{name}' missing from CBZ")))?;

    let mut buf = Vec::with_capacity(entry.size() as usize);
    std::io::Read::read_to_end(&mut entry, &mut buf)
        .map_err(|e| Error::Internal(format!("CBZ read error: {e}")))?;

    Ok((buf, ext))
}
