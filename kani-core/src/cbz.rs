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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    /// Builds a CBZ at `dir/name.cbz` containing the given entries (name → bytes).
    fn make_cbz(dir: &TempDir, name: &str, entries: &[(&str, &[u8])]) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        for (entry_name, data) in entries {
            writer.start_file(*entry_name, opts).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    #[test]
    fn lists_only_image_entries() {
        let dir = TempDir::new().unwrap();
        let cbz = make_cbz(
            &dir,
            "test.cbz",
            &[
                ("0001.jpg", b"fake-jpg"),
                ("0002.png", b"fake-png"),
                ("0003.webp", b"fake-webp"),
                ("ComicInfo.xml", b"<ComicInfo/>"),
                ("readme.txt", b"ignore me"),
            ],
        );
        let pages = list_cbz_pages(&cbz).unwrap();
        assert_eq!(pages, vec!["0001.jpg", "0002.png", "0003.webp"]);
    }

    #[test]
    fn pages_are_sorted_lexicographically() {
        let dir = TempDir::new().unwrap();
        let cbz = make_cbz(
            &dir,
            "sorted.cbz",
            &[
                ("0003.jpg", b"c"),
                ("0001.jpg", b"a"),
                ("0002.jpg", b"b"),
            ],
        );
        let pages = list_cbz_pages(&cbz).unwrap();
        assert_eq!(pages, vec!["0001.jpg", "0002.jpg", "0003.jpg"]);
    }

    #[test]
    fn reads_page_content_by_index() {
        let dir = TempDir::new().unwrap();
        let cbz = make_cbz(
            &dir,
            "read.cbz",
            &[
                ("0001.jpg", b"image-data-1"),
                ("0002.png", b"image-data-2"),
            ],
        );
        let (data, ext) = read_cbz_page(&cbz, 0).unwrap();
        assert_eq!(data, b"image-data-1");
        assert_eq!(ext, "jpg");

        let (data2, ext2) = read_cbz_page(&cbz, 1).unwrap();
        assert_eq!(data2, b"image-data-2");
        assert_eq!(ext2, "png");
    }

    #[test]
    fn out_of_bounds_page_returns_error() {
        let dir = TempDir::new().unwrap();
        let cbz = make_cbz(&dir, "oob.cbz", &[("0001.jpg", b"data")]);
        let err = read_cbz_page(&cbz, 5).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[test]
    fn missing_file_returns_not_found() {
        let err = list_cbz_pages(std::path::Path::new("/nonexistent/path.cbz")).unwrap_err();
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[test]
    fn empty_archive_has_no_pages() {
        let dir = TempDir::new().unwrap();
        let cbz = make_cbz(&dir, "empty.cbz", &[("readme.txt", b"nothing here")]);
        let pages = list_cbz_pages(&cbz).unwrap();
        assert!(pages.is_empty());
    }

    #[test]
    fn case_insensitive_extension_matching() {
        let dir = TempDir::new().unwrap();
        let cbz = make_cbz(
            &dir,
            "caps.cbz",
            &[
                ("0001.JPG", b"uppercase"),
                ("0002.JPEG", b"also-jpeg"),
                ("0003.PNG", b"uppercase-png"),
            ],
        );
        let pages = list_cbz_pages(&cbz).unwrap();
        assert_eq!(pages.len(), 3);
    }
}
