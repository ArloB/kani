use crate::manifest::ChapterManifest;

/// dHash: grayscale, resize to 9x8, compare each pixel with its right neighbour.
/// 64 comparisons produce 64 bits. Resilient to re-encoding and mild rescaling,
/// which is exactly what a source silently re-uploading a chapter does.
pub fn perceptual_hash_page(decoded: &image::DynamicImage) -> u64 {
    use image::imageops::FilterType;

    let small = decoded
        .grayscale()
        .resize_exact(9, 8, FilterType::Triangle)
        .to_luma8();

    let mut hash = 0u64;
    let mut bit = 0;
    for y in 0..8u32 {
        for x in 0..8u32 {
            let left = small.get_pixel(x, y).0[0];
            let right = small.get_pixel(x + 1, y).0[0];
            if left > right {
                hash |= 1 << bit;
            }
            bit += 1;
        }
    }
    hash
}

pub fn phash_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityScore {
    pub median_long_edge_px: u32,
    pub bytes_per_megapixel: f32,
    pub page_count: u32,
}

pub fn score_from_manifest(m: &ChapterManifest) -> QualityScore {
    let mut long_edges: Vec<u32> = m
        .pages
        .iter()
        .filter_map(|p| match (p.width, p.height) {
            (Some(w), Some(h)) => Some(w.max(h)),
            _ => None,
        })
        .collect();
    long_edges.sort_unstable();

    let median_long_edge_px = if long_edges.is_empty() {
        0
    } else {
        long_edges[long_edges.len() / 2]
    };

    let total_pixels: u64 = m
        .pages
        .iter()
        .filter_map(|p| match (p.width, p.height) {
            (Some(w), Some(h)) => Some(u64::from(w) * u64::from(h)),
            _ => None,
        })
        .sum();

    let bytes_per_megapixel = if total_pixels == 0 {
        0.0
    } else {
        (m.total_bytes as f64 / (total_pixels as f64 / 1_000_000.0)) as f32
    };

    QualityScore {
        median_long_edge_px,
        bytes_per_megapixel,
        page_count: m.page_count,
    }
}

/// A candidate only wins on a clear resolution gain, or — at the same resolution
/// tier — a large bytes-per-megapixel margin. The margin is the noise guard that
/// stops a trivially larger re-encode from being announced as an upgrade.
pub fn is_meaningfully_better(
    candidate: &QualityScore,
    current: &QualityScore,
    min_res_gain: f32,
) -> bool {
    if candidate.median_long_edge_px == 0 {
        return false;
    }
    if current.median_long_edge_px == 0 {
        return true;
    }

    let res_ratio = candidate.median_long_edge_px as f32 / current.median_long_edge_px as f32;
    if res_ratio >= min_res_gain {
        return true;
    }
    if res_ratio < 1.0 {
        return false;
    }

    const BYTES_MARGIN: f32 = 1.5;
    current.bytes_per_megapixel > 0.0
        && candidate.bytes_per_megapixel >= current.bytes_per_megapixel * BYTES_MARGIN
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::manifest::PageDigest;

    fn manifest(pages: Vec<(Option<u32>, Option<u32>)>, total_bytes: u64) -> ChapterManifest {
        ChapterManifest {
            schema: 1,
            archive_hash: "x".into(),
            page_count: pages.len() as u32,
            total_bytes,
            created_at: 0,
            pages: pages
                .into_iter()
                .enumerate()
                .map(|(i, (w, h))| PageDigest {
                    name: format!("{i:04}.jpg"),
                    bytes: 0,
                    content_hash: "h".into(),
                    perceptual_hash: 0,
                    width: w,
                    height: h,
                })
                .collect(),
        }
    }

    fn solid(w: u32, h: u32, v: u8) -> image::DynamicImage {
        image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(w, h, image::Luma([v])))
    }

    #[test]
    fn identical_images_have_distance_zero() {
        let a = perceptual_hash_page(&solid(64, 64, 128));
        let b = perceptual_hash_page(&solid(64, 64, 128));
        assert_eq!(phash_distance(a, b), 0);
    }

    #[test]
    fn phash_distance_counts_differing_bits() {
        assert_eq!(phash_distance(0b1011, 0b1000), 2);
        assert_eq!(phash_distance(u64::MAX, 0), 64);
    }

    #[test]
    fn gradient_survives_a_rescale() {
        // A downscale is the cheap stand-in for a re-encode: the perceptual hash
        // should stay close, unlike a byte hash which changes completely.
        let mut img = image::GrayImage::new(90, 80);
        for (x, _y, p) in img.enumerate_pixels_mut() {
            *p = image::Luma([(x * 2) as u8]);
        }
        let full = image::DynamicImage::ImageLuma8(img);
        let half = full.resize_exact(45, 40, image::imageops::FilterType::Triangle);

        let d = phash_distance(perceptual_hash_page(&full), perceptual_hash_page(&half));
        assert!(
            d < 6,
            "rescaled gradient should stay close, distance was {d}"
        );
    }

    #[test]
    fn score_uses_median_long_edge_and_skips_unknown_dims() {
        let m = manifest(
            vec![
                (Some(800), Some(1200)),
                (Some(900), Some(1400)),
                (None, None),
                (Some(1000), Some(1600)),
            ],
            3_000_000,
        );
        let s = score_from_manifest(&m);
        assert_eq!(s.median_long_edge_px, 1400, "median of 1200/1400/1600");
        assert_eq!(s.page_count, 4);
        assert!(s.bytes_per_megapixel > 0.0);
    }

    #[test]
    fn manifest_without_known_dimensions_scores_zero_and_never_wins() {
        let unknown = score_from_manifest(&manifest(vec![(None, None)], 100));
        assert_eq!(unknown.median_long_edge_px, 0);
        let known = score_from_manifest(&manifest(vec![(Some(800), Some(1200))], 100));
        assert!(!is_meaningfully_better(&unknown, &known, 1.2));
    }

    #[test]
    fn higher_resolution_wins() {
        let current = score_from_manifest(&manifest(vec![(Some(800), Some(1200))], 1_000_000));
        let better = score_from_manifest(&manifest(vec![(Some(1600), Some(2400))], 2_000_000));
        assert!(is_meaningfully_better(&better, &current, 1.2));
    }

    #[test]
    fn equal_resolution_slightly_larger_file_does_not_win() {
        let current = score_from_manifest(&manifest(vec![(Some(800), Some(1200))], 1_000_000));
        let barely = score_from_manifest(&manifest(vec![(Some(800), Some(1200))], 1_100_000));
        assert!(
            !is_meaningfully_better(&barely, &current, 1.2),
            "a 10% larger re-encode is not an upgrade"
        );
    }

    #[test]
    fn equal_resolution_much_larger_file_wins() {
        let current = score_from_manifest(&manifest(vec![(Some(800), Some(1200))], 1_000_000));
        let much = score_from_manifest(&manifest(vec![(Some(800), Some(1200))], 2_000_000));
        assert!(is_meaningfully_better(&much, &current, 1.2));
    }

    #[test]
    fn lower_resolution_never_wins() {
        let current = score_from_manifest(&manifest(vec![(Some(1600), Some(2400))], 1_000_000));
        let worse = score_from_manifest(&manifest(vec![(Some(800), Some(1200))], 9_000_000));
        assert!(
            !is_meaningfully_better(&worse, &current, 1.2),
            "a downgrade cannot win on file size alone"
        );
    }
}
