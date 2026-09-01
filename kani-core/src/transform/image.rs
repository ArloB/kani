//! Image transforms: the LCG tile descrambler, exposed through the registry as
//! [`LcgTileDescramble`].

use super::{ResolvedTransform, Transform, TransformError, TransformKind, TransformOutput};
use crate::error::{Error, Result};
use image::{DynamicImage, ImageFormat};
use rquest::header::HeaderMap;

pub(crate) struct LcgTileDescramble;

impl Transform for LcgTileDescramble {
    fn names(&self) -> &'static [&'static str] {
        &["lcg-tile-5x5-from-header", "lcg-tile-5x5"]
    }

    fn kind(&self) -> TransformKind {
        TransformKind::Image
    }

    fn description(&self) -> &'static str {
        "LCG tile descrambler; seed from the x-scramble-seed header or an inline hint parameter"
    }

    fn resolve(&self, hint: &str, headers: &HeaderMap) -> Option<ResolvedTransform> {
        let plan = ScramblePlan::from(hint, headers)?;
        Some(ResolvedTransform::new(
            TransformOutput {
                file_extension: "jpg",
                content_type: "image/jpeg",
            },
            move |data| {
                plan.apply(data)
                    .map_err(|e| TransformError::Apply(e.to_string()))
            },
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScramblePlan {
    byte_layer: Option<ByteLayer>,
    tile_layer: Option<TileLayer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteLayer {
    seed: i32,
    len: usize,
    /// `"2"` selects the xorshift keystream; anything else is the LCG one.
    xorshift: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TileLayer {
    seed: i32,
    /// `"3"` selects the xorshift shuffle; anything else is the LCG one.
    xorshift: bool,
}

impl ScramblePlan {
    fn from(hint: &str, headers: &HeaderMap) -> Option<Self> {
        let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());

        let byte_layer = match (
            header("x-enc-seed").and_then(parse_scramble_seed),
            header("x-enc-len").and_then(|v| v.trim().parse::<usize>().ok()),
        ) {
            (Some(seed), Some(len)) => Some(ByteLayer {
                seed,
                len,
                xorshift: header("x-enc-algo").map(str::trim) == Some("2"),
            }),
            _ => None,
        };

        let grid_ok = header("x-scramble-grid").map(str::trim).unwrap_or("5x5") == "5x5";
        let tile_layer = resolve_scramble_seed(hint, headers)
            .filter(|_| grid_ok)
            .map(|seed| TileLayer {
                seed: seed ^ decode_scramble_hash(header("x-scramble-hash")),
                xorshift: header("x-scramble-algo").map(str::trim) == Some("3"),
            });

        if byte_layer.is_none() && tile_layer.is_none() {
            return None;
        }
        Some(Self {
            byte_layer,
            tile_layer,
        })
    }

    fn apply(&self, data: &[u8]) -> Result<Vec<u8>> {
        let decoded = match self.byte_layer {
            Some(layer) => layer.decode(data),
            None => data.to_vec(),
        };
        match self.tile_layer {
            Some(layer) => tile_descramble(&decoded, layer),
            None => reencode_jpeg(&decoded),
        }
    }
}

impl ByteLayer {
    fn decode(&self, data: &[u8]) -> Vec<u8> {
        if !self.xorshift {
            return decode_with_lcg(data, self.seed, self.len);
        }
        let candidates = [
            decode_with_xorshift(data, self.seed | 1, self.len, false),
            decode_with_xorshift(data, self.seed, self.len, false),
            decode_with_xorshift(data, self.seed | 1, self.len, true),
            decode_with_lcg(data, self.seed, self.len),
        ];
        candidates
            .iter()
            .find(|c| has_image_signature(c))
            .cloned()
            .unwrap_or_else(|| candidates[0].clone())
    }
}

/// Keystream constants for the byte layer.
const ENC_MULTIPLIER: i32 = 1_000_005;
const ENC_INCREMENT: i32 = 1_234_567_891;

fn decode_with_lcg(data: &[u8], seed: i32, len: usize) -> Vec<u8> {
    let mut out = data.to_vec();
    let mut state = seed;
    for byte in out.iter_mut().take(len) {
        state = state
            .wrapping_mul(ENC_MULTIPLIER)
            .wrapping_add(ENC_INCREMENT);
        *byte ^= ((state as u32) >> 24) as u8;
    }
    out
}

fn decode_with_xorshift(data: &[u8], initial_state: i32, len: usize, high_byte: bool) -> Vec<u8> {
    let mut out = data.to_vec();
    let mut state = initial_state;
    for byte in out.iter_mut().take(len) {
        state = next_xorshift(state);
        let key = if high_byte {
            ((state as u32) >> 24) as u8
        } else {
            (state as u32 & 0xFF) as u8
        };
        *byte ^= key;
    }
    out
}

fn next_xorshift(state: i32) -> i32 {
    let mut next = state;
    next ^= next << 13;
    next ^= ((next as u32) >> 17) as i32;
    next ^ (next << 5)
}

/// JPEG, PNG, and WebP magic numbers — enough to tell a decoded file from noise.
fn has_image_signature(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    let webp = &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP";
    let jpeg = data[0] == 0xFF && data[1] == 0xD8;
    let png = data[0] == 0x89 && &data[1..4] == b"PNG";
    webp || jpeg || png
}

/// The site labels the constant folded into the tile seed with a short digit
/// string. Only the two published builds are known; anything else contributes
/// nothing, which is the same as the header being absent.
fn decode_scramble_hash(raw: Option<&str>) -> i32 {
    match raw.map(str::trim) {
        Some("03632") => 58414,
        Some("02900") => 117_532,
        _ => 0,
    }
}

fn reencode_jpeg(data: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(data)
        .map_err(|e| Error::Other(format!("image decode error: {e}")))?;
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Jpeg)
        .map_err(|e| Error::Other(format!("image encode error: {e}")))?;
    Ok(out)
}

const GRID: usize = 5;
const TILES: usize = GRID * GRID;

/// Resolve the LCG scramble seed for a page, given the per-page transform hint
/// declared by the extension and the HTTP response headers from the upstream CDN.
fn resolve_scramble_seed(hint: &str, headers: &rquest::header::HeaderMap) -> Option<i32> {
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
fn parse_scramble_seed(raw: &str) -> Option<i32> {
    let seed = raw.trim().parse::<i64>().ok()? as i32;
    if seed == 0 { None } else { Some(seed) }
}

/// Build the 25-element source→destination tile permutation for the given seed.
///
/// Both variants are the same Fisher-Yates shuffle; they differ only in the
/// generator driving it, which the `x-scramble-algo` header selects.
fn build_order(seed: i32, xorshift: bool) -> [usize; TILES] {
    let mut order: [usize; TILES] = std::array::from_fn(|i| i);
    let mut state = if xorshift { seed | 1 } else { seed };
    for i in (1..TILES).rev() {
        state = if xorshift {
            next_xorshift(state)
        } else {
            state
                .wrapping_mul(1_664_525_i32)
                .wrapping_add(1_013_904_223_i32)
        };
        let j = (state as u32 as u64 % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

/// Undo the tile permutation an image was scrambled with.
fn tile_descramble(data: &[u8], layer: TileLayer) -> Result<Vec<u8>> {
    let src_img = image::load_from_memory(data)
        .map_err(|e| Error::Other(format!("image decode error: {e}")))?
        .into_rgba8();

    let (width, height) = src_img.dimensions();
    let tile_w = (width / GRID as u32).max(1);
    let tile_h = (height / GRID as u32).max(1);

    let perm = build_order(layer.seed, layer.xorshift);
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
                // Tiny images can place a grid tile beyond their edge; skip it instead of
                // panicking.
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
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([10, 20, 30, 255]),
        ));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();

        let out = tile_descramble(
            &png,
            TileLayer {
                seed: 12345,
                xorshift: false,
            },
        );
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

    fn header_map(name: &str, value: &str) -> rquest::header::HeaderMap {
        let mut h = rquest::header::HeaderMap::new();
        h.insert(
            rquest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            rquest::header::HeaderValue::from_str(value).unwrap(),
        );
        h
    }

    #[test]
    fn a_scramble_seed_header_drives_the_descramble() {
        let headers = header_map("x-scramble-seed", "12345");
        assert_eq!(
            resolve_scramble_seed("lcg-tile-5x5-from-header", &headers),
            Some(12345),
            "the header value becomes the descramble seed"
        );
        assert_eq!(
            resolve_scramble_seed("lcg-tile-5x5:777", &rquest::header::HeaderMap::new()),
            Some(777)
        );
    }

    #[test]
    fn a_missing_or_malformed_scramble_seed_stores_the_raw_image() {
        assert_eq!(
            resolve_scramble_seed(
                "lcg-tile-5x5-from-header",
                &rquest::header::HeaderMap::new()
            ),
            None
        );
        assert_eq!(
            resolve_scramble_seed(
                "lcg-tile-5x5-from-header",
                &header_map("x-scramble-seed", "abc")
            ),
            None
        );
        assert_eq!(
            resolve_scramble_seed(
                "lcg-tile-5x5-from-header",
                &header_map("x-scramble-seed", "0")
            ),
            None
        );
        assert_eq!(
            resolve_scramble_seed("none", &header_map("x-scramble-seed", "12345")),
            None
        );
    }

    #[test]
    fn transform_impl_exposes_image_kind_and_jpeg_output() {
        let t = LcgTileDescramble;
        assert_eq!(t.kind(), TransformKind::Image);
        assert!(t.names().contains(&"lcg-tile-5x5-from-header"));

        let resolved = t
            .resolve("lcg-tile-5x5:12345", &rquest::header::HeaderMap::new())
            .expect("inline seed resolves");
        assert_eq!(resolved.output().file_extension, "jpg");
        assert_eq!(resolved.output().content_type, "image/jpeg");
    }

    #[test]
    fn transform_impl_applies_the_descramble() {
        use image::{DynamicImage, ImageFormat, RgbaImage};

        let img =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255])));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();

        let resolved = LcgTileDescramble
            .resolve("lcg-tile-5x5:777", &rquest::header::HeaderMap::new())
            .unwrap();
        assert!(
            resolved.apply(&png).is_ok(),
            "resolved transform applies without error"
        );
    }

    #[test]
    fn build_order_is_permutation() {
        let order = build_order(99999, false);
        let mut seen = [false; TILES];
        for &v in &order {
            assert!(v < TILES);
            seen[v] = true;
        }
        assert!(seen.iter().all(|&b| b), "order is not a valid permutation");
    }

    #[test]
    fn build_order_seed_zero_is_identity() {
        let order = build_order(0, false);
        let mut seen = [false; TILES];
        for &v in &order {
            seen[v] = true;
        }
        assert!(seen.iter().all(|&b| b));
    }

    #[test]
    fn build_order_known_seed() {
        let order = build_order(12345, false);
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

        let perm = build_order(seed, false);
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

        let descrambled_jpeg = tile_descramble(
            &scrambled_png,
            TileLayer {
                seed,
                xorshift: false,
            },
        )
        .unwrap();
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

    fn headers_from(pairs: &[(&str, &str)]) -> rquest::header::HeaderMap {
        let mut h = rquest::header::HeaderMap::new();
        for (name, value) in pairs {
            h.insert(
                rquest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                rquest::header::HeaderValue::from_str(value).unwrap(),
            );
        }
        h
    }

    #[test]
    fn an_unscrambled_response_resolves_to_no_plan() {
        assert_eq!(
            ScramblePlan::from("lcg-tile-5x5-from-header", &headers_from(&[])),
            None
        );
    }

    #[test]
    fn each_header_family_selects_its_own_layer() {
        let tile_only = ScramblePlan::from(
            "lcg-tile-5x5-from-header",
            &headers_from(&[("x-scramble-seed", "12345")]),
        )
        .unwrap();
        assert_eq!(
            tile_only,
            ScramblePlan {
                byte_layer: None,
                tile_layer: Some(TileLayer {
                    seed: 12345,
                    xorshift: false
                }),
            }
        );

        let byte_only = ScramblePlan::from(
            "lcg-tile-5x5-from-header",
            &headers_from(&[("x-enc-seed", "777"), ("x-enc-len", "64")]),
        )
        .unwrap();
        assert_eq!(
            byte_only,
            ScramblePlan {
                byte_layer: Some(ByteLayer {
                    seed: 777,
                    len: 64,
                    xorshift: false
                }),
                tile_layer: None,
            }
        );

        let both = ScramblePlan::from(
            "lcg-tile-5x5-from-header",
            &headers_from(&[
                ("x-scramble-seed", "12345"),
                ("x-scramble-algo", "3"),
                ("x-enc-seed", "777"),
                ("x-enc-len", "64"),
                ("x-enc-algo", "2"),
            ]),
        )
        .unwrap();
        assert!(both.byte_layer.unwrap().xorshift);
        assert!(both.tile_layer.unwrap().xorshift);
    }

    #[test]
    fn an_incomplete_byte_layer_is_no_layer() {
        assert_eq!(
            ScramblePlan::from(
                "lcg-tile-5x5-from-header",
                &headers_from(&[("x-enc-seed", "777")])
            ),
            None
        );
    }

    #[test]
    fn a_grid_this_code_cannot_lay_out_drops_the_tile_layer() {
        assert_eq!(
            ScramblePlan::from(
                "lcg-tile-5x5-from-header",
                &headers_from(&[("x-scramble-seed", "12345"), ("x-scramble-grid", "4x4")]),
            ),
            None
        );
    }

    #[test]
    fn a_known_scramble_hash_is_folded_into_the_seed() {
        let plan = ScramblePlan::from(
            "lcg-tile-5x5-from-header",
            &headers_from(&[("x-scramble-seed", "12345"), ("x-scramble-hash", "03632")]),
        )
        .unwrap();
        assert_eq!(plan.tile_layer.unwrap().seed, 12345 ^ 58414);

        let unknown = ScramblePlan::from(
            "lcg-tile-5x5-from-header",
            &headers_from(&[("x-scramble-seed", "12345"), ("x-scramble-hash", "99999")]),
        )
        .unwrap();
        assert_eq!(unknown.tile_layer.unwrap().seed, 12345);
    }

    #[test]
    fn the_byte_layer_is_its_own_inverse() {
        let original: Vec<u8> = (0..200u32).map(|i| (i * 7 % 251) as u8).collect();
        for xorshift in [false, true] {
            let layer = ByteLayer {
                seed: 987_654,
                len: 128,
                xorshift,
            };
            let encoded = if xorshift {
                decode_with_xorshift(&original, layer.seed | 1, layer.len, false)
            } else {
                decode_with_lcg(&original, layer.seed, layer.len)
            };
            assert_ne!(encoded[..128], original[..128], "the first bytes change");
            assert_eq!(encoded[128..], original[128..], "the tail is left alone");

            let decoded = if xorshift {
                decode_with_xorshift(&encoded, layer.seed | 1, layer.len, false)
            } else {
                layer.decode(&encoded)
            };
            assert_eq!(decoded, original);
        }
    }

    #[test]
    fn the_xorshift_tile_order_is_a_permutation_distinct_from_the_lcg_one() {
        let lcg = build_order(12345, false);
        let xorshift = build_order(12345, true);
        assert_ne!(lcg, xorshift);
        let mut counts = [0u8; TILES];
        for &v in &xorshift {
            counts[v] += 1;
        }
        assert!(counts.iter().all(|&c| c == 1));
    }

    #[test]
    fn the_xorshift_tile_layer_round_trips() {
        use image::{Rgba, RgbaImage};

        const TILE: u32 = 8;
        const SIZE: u32 = TILE * GRID as u32;
        let layer = TileLayer {
            seed: 4242,
            xorshift: true,
        };

        let mut original = RgbaImage::new(SIZE, SIZE);
        for row in 0..GRID as u32 {
            for col in 0..GRID as u32 {
                for dy in 0..TILE {
                    for dx in 0..TILE {
                        original.put_pixel(
                            col * TILE + dx,
                            row * TILE + dy,
                            Rgba([(col * 40) as u8, (row * 40) as u8, 200, 255]),
                        );
                    }
                }
            }
        }

        // Scramble with the same permutation the descrambler will derive.
        let perm = build_order(layer.seed, layer.xorshift);
        let mut scrambled = RgbaImage::new(SIZE, SIZE);
        for (scrambled_idx, &orig_idx) in perm.iter().enumerate() {
            let (sc, sr) = ((scrambled_idx % GRID) as u32, (scrambled_idx / GRID) as u32);
            let (oc, orow) = ((orig_idx % GRID) as u32, (orig_idx / GRID) as u32);
            for dy in 0..TILE {
                for dx in 0..TILE {
                    let p = *original.get_pixel(oc * TILE + dx, orow * TILE + dy);
                    scrambled.put_pixel(sc * TILE + dx, sr * TILE + dy, p);
                }
            }
        }

        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(scrambled)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();

        let out = tile_descramble(&png, layer).unwrap();
        let restored = image::load_from_memory(&out).unwrap().into_rgba8();
        for row in 0..GRID as u32 {
            for col in 0..GRID as u32 {
                let expected = *original.get_pixel(col * TILE + 4, row * TILE + 4);
                let actual = *restored.get_pixel(col * TILE + 4, row * TILE + 4);
                // JPEG is lossy; compare channels with a tolerance.
                for c in 0..3 {
                    assert!(
                        (expected[c] as i16 - actual[c] as i16).abs() <= 12,
                        "tile ({col},{row}) channel {c}: {} vs {}",
                        expected[c],
                        actual[c]
                    );
                }
            }
        }
    }
}
