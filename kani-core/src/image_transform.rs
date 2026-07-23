//! Generic image post-processing for content sources.
//!
//! Extensions declare a per-page `transform` hint (e.g. `"lcg-tile-5x5-from-header"`)
//! in the WIT `page` record. The proxy and downloader call [`resolve_scramble_seed`]
//! while the upstream response headers are still available, then call
//! [`lcg_tile_descramble`] after the body is buffered.

use crate::error::{Error, Result};
use image::{DynamicImage, ImageFormat};

const GRID: usize = 5;
const TILES: usize = GRID * GRID; // 25

/// Resolve the LCG scramble seed for a page, given the per-page transform hint
/// declared by the extension and the HTTP response headers from the upstream CDN.
pub fn resolve_scramble_seed(hint: &str, headers: &rquest::header::HeaderMap) -> Option<i32> {
    match hint {
        "lcg-tile-5x5-from-header" => headers
            .get("x-scramble-seed")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_scramble_seed),
        h if h.starts_with("lcg-tile-5x5:") => {
            parse_scramble_seed(h.trim_start_matches("lcg-tile-5x5:"))
        }
        _ => None,
    }
}

/// Parse the raw string value of an `x-scramble-seed` header.
/// Returns `None` for a zero seed or any value that does not parse as a decimal integer.
pub fn parse_scramble_seed(raw: &str) -> Option<i32> {
    let seed = raw.trim().parse::<i64>().ok()? as i32;
    if seed == 0 { None } else { Some(seed) }
}

/// Build the 25-element source→destination tile permutation for the given seed.
fn build_order(seed: i32) -> [usize; TILES] {
    let mut order: [usize; TILES] = std::array::from_fn(|i| i);
    let mut state = seed;
    for i in (1..TILES).rev() {
        state = state
            .wrapping_mul(1_664_525_i32)
            .wrapping_add(1_013_904_223_i32);
        let j = (state as u32 as u64 % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

/// Descramble an image whose tiles have been permuted with the given LCG seed.
pub fn lcg_tile_descramble(data: &[u8], seed: i32) -> Result<Vec<u8>> {
    let src_img = image::load_from_memory(data)
        .map_err(|e| Error::Other(format!("image decode error: {e}")))?
        .into_rgba8();

    let (width, height) = src_img.dimensions();
    let tile_w = (width / GRID as u32).max(1);
    let tile_h = (height / GRID as u32).max(1);

    let perm = build_order(seed);
    let mut dst_img = src_img.clone();

    for (src_idx, &dst_idx) in perm.iter().enumerate() {
        let src_col = (src_idx % GRID) as u32;
        let src_row = (src_idx / GRID) as u32;
        let dst_col = (dst_idx % GRID) as u32;
        let dst_row = (dst_idx / GRID) as u32;

        let src_x = src_col * tile_w;
        let src_y = src_row * tile_h;
        let dst_x = dst_col * tile_w;
        let dst_y = dst_row * tile_h;

        for dy in 0..tile_h {
            for dx in 0..tile_w {
                // Bounds-checked: `tile_w`/`tile_h` are floored to 1, so on an
                // image narrower or shorter than the 5×5 grid a tile column can
                // start past the edge (src_x = 4 on a 2px-wide image). Copying
                // that would panic and unwind the download worker; skipping it
                // leaves a nonsensically small image best-effort rather than
                // crashing the whole chapter.
                let (sx, sy) = (src_x + dx, src_y + dy);
                let (dx2, dy2) = (dst_x + dx, dst_y + dy);
                if sx < width && sy < height && dx2 < width && dy2 < height {
                    let pixel = *src_img.get_pixel(sx, sy);
                    dst_img.put_pixel(dx2, dy2, pixel);
                }
            }
        }
    }

    let mut out = Vec::new();
    DynamicImage::ImageRgba8(dst_img)
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Jpeg)
        .map_err(|e| Error::Other(format!("image encode error: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn descramble_does_not_panic_on_an_image_smaller_than_the_grid() {
        // A 2x2 image cannot be tiled into a 5x5 grid; the tile columns run
        // past its edge. The old code indexed out of bounds and panicked,
        // unwinding the download worker.
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([10, 20, 30, 255]),
        ));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();

        // Must return Ok (a best-effort image) rather than panic.
        let out = lcg_tile_descramble(&png, 12345);
        assert!(out.is_ok(), "a tiny image must not crash the descrambler");
    }

    #[test]
    fn parse_scramble_seed_zero_returns_none() {
        assert_eq!(parse_scramble_seed("0"), None);
    }

    #[test]
    fn parse_scramble_seed_nonzero_returns_some() {
        assert_eq!(parse_scramble_seed("12345"), Some(12345));
        assert_eq!(parse_scramble_seed("-1"), Some(-1));
    }

    #[test]
    fn parse_scramble_seed_whitespace_trimmed() {
        assert_eq!(parse_scramble_seed("  42  "), Some(42));
    }

    #[test]
    fn parse_scramble_seed_invalid_returns_none() {
        assert_eq!(parse_scramble_seed("abc"), None);
        assert_eq!(parse_scramble_seed(""), None);
    }

    #[test]
    fn build_order_is_permutation() {
        let order = build_order(99999);
        let mut seen = [false; TILES];
        for &v in &order {
            assert!(v < TILES);
            seen[v] = true;
        }
        assert!(seen.iter().all(|&b| b), "order is not a valid permutation");
    }

    #[test]
    fn build_order_seed_zero_is_identity() {
        let order = build_order(0);
        let mut seen = [false; TILES];
        for &v in &order {
            seen[v] = true;
        }
        assert!(seen.iter().all(|&b| b));
    }

    #[test]
    fn build_order_known_seed() {
        let order = build_order(12345);
        assert_eq!(order.len(), TILES);
        let mut counts = [0u8; TILES];
        for &v in &order {
            counts[v] += 1;
        }
        assert!(counts.iter().all(|&c| c == 1));
    }

    #[test]
    fn parse_scramble_seed_accepts_u32_range() {
        assert_eq!(parse_scramble_seed("3000000000"), Some(-1_294_967_296_i32));
    }

    #[test]
    fn descramble_round_trip_recovers_original() {
        use image::{Rgba, RgbaImage};

        const TILE: u32 = 8;
        const SIZE: u32 = TILE * GRID as u32;
        let seed: i32 = 12345;

        let mut original = RgbaImage::new(SIZE, SIZE);
        for row in 0..GRID as u32 {
            for col in 0..GRID as u32 {
                let r = (col * 40) as u8;
                let g = (row * 40) as u8;
                for dy in 0..TILE {
                    for dx in 0..TILE {
                        original.put_pixel(
                            col * TILE + dx,
                            row * TILE + dy,
                            Rgba([r, g, 200, 255]),
                        );
                    }
                }
            }
        }

        let perm = build_order(seed);
        let mut scrambled = RgbaImage::new(SIZE, SIZE);
        for (scrambled_idx, &orig_idx) in perm.iter().enumerate() {
            let sc = (scrambled_idx % GRID) as u32;
            let sr = (scrambled_idx / GRID) as u32;
            let oc = (orig_idx % GRID) as u32;
            let orow = (orig_idx / GRID) as u32;
            for dy in 0..TILE {
                for dx in 0..TILE {
                    let p = *original.get_pixel(oc * TILE + dx, orow * TILE + dy);
                    scrambled.put_pixel(sc * TILE + dx, sr * TILE + dy, p);
                }
            }
        }

        let mut scrambled_png = Vec::new();
        DynamicImage::ImageRgba8(scrambled)
            .write_to(
                &mut std::io::Cursor::new(&mut scrambled_png),
                ImageFormat::Png,
            )
            .unwrap();

        let descrambled_jpeg = lcg_tile_descramble(&scrambled_png, seed).unwrap();
        let descrambled = image::load_from_memory(&descrambled_jpeg)
            .unwrap()
            .into_rgba8();

        for row in 0..GRID as u32 {
            for col in 0..GRID as u32 {
                let want = *original.get_pixel(col * TILE + TILE / 2, row * TILE + TILE / 2);
                let got = *descrambled.get_pixel(col * TILE + TILE / 2, row * TILE + TILE / 2);
                for c in 0..3 {
                    let diff = (want.0[c] as i32 - got.0[c] as i32).abs();
                    assert!(
                        diff < 16,
                        "tile ({col},{row}) channel {c}: want {} got {} (diff {diff})",
                        want.0[c],
                        got.0[c],
                    );
                }
            }
        }
    }
}
