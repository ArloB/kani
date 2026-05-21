use std::path::Path;
use resvg::usvg;
use tiny_skia::{Pixmap, Transform};
use crate::error::CliError;

pub fn run() -> Result<(), CliError> {
    let svg_path = Path::new("static/icons/kani-mark.svg");
    let out_dir  = Path::new("static/icons");
    generate_icons(svg_path, out_dir)
}

/// Renders `svg_path` to the three PWA icon sizes required by the web app manifest.
///
/// Outputs written to `out_dir`:
/// - `icon-192.png`          — 192×192, standard
/// - `icon-512.png`          — 512×512, standard
/// - `icon-512-maskable.png` — 512×512 with 20% safe-zone padding for maskable use
pub fn generate_icons(svg_path: &Path, out_dir: &Path) -> Result<(), CliError> {
    let svg_data = std::fs::read(svg_path)
        .map_err(|e| CliError::Other(format!("failed to read {}: {e}", svg_path.display())))?;

    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(&svg_data, &opt)
        .map_err(|e| CliError::Other(format!("failed to parse SVG: {e}")))?;

    std::fs::create_dir_all(out_dir)?;

    render_at_size(&tree, 192, out_dir.join("icon-192.png"), 0.0)?;
    render_at_size(&tree, 512, out_dir.join("icon-512.png"), 0.0)?;
    render_at_size(&tree, 512, out_dir.join("icon-512-maskable.png"), 0.10)?;

    println!("Icons written to {}", out_dir.display());
    Ok(())
}

/// Renders the tree into a square PNG of `size` × `size` pixels.
///
/// `safe_zone_ratio` adds blank padding on each side as a fraction of `size`
/// (e.g. 0.10 = 10% per side, leaving 80% for the mark — satisfying the 20% total
/// safe-zone requirement for maskable icons).
fn render_at_size(
    tree: &usvg::Tree,
    size: u32,
    output: impl AsRef<Path>,
    safe_zone_ratio: f32,
) -> Result<(), CliError> {
    let mut pixmap = Pixmap::new(size, size)
        .ok_or_else(|| CliError::Other(format!("failed to allocate {size}×{size} pixmap")))?;

    let padding  = (size as f32 * safe_zone_ratio).round();
    let inner    = size as f32 - padding * 2.0;
    let svg_size = tree.size();
    let scale    = inner / svg_size.width().max(svg_size.height());
    let tx       = padding + (inner - svg_size.width()  * scale) / 2.0;
    let ty       = padding + (inner - svg_size.height() * scale) / 2.0;

    let transform = Transform::from_scale(scale, scale).post_translate(tx, ty);
    resvg::render(tree, transform, &mut pixmap.as_mut());

    pixmap.save_png(output.as_ref())
        .map_err(|e| CliError::Other(format!("failed to save PNG: {e}")))?;

    println!("  wrote {}  ({}×{})", output.as_ref().display(), size, size);
    Ok(())
}
