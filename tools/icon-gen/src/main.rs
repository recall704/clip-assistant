//! Draw the application icon and write it as a multi-resolution `.ico`.
//!
//! The artwork is the same shape the tray icon draws at runtime (an accent
//! rounded square holding a clipboard), rendered here at high resolution with
//! anti-aliasing so the executable, taskbar and Alt-Tab all get a crisp icon.
//!
//! ```text
//! cargo run --manifest-path tools/icon-gen/Cargo.toml -- assets/icon.ico
//! ```

use std::fs::File;
use std::io::BufWriter;

use ico::{IconDir, IconDirEntry, IconImage, ResourceType};

/// `#726CFF`, the accent used by the panel and the tray icon.
const ACCENT: [f64; 3] = [0.447, 0.424, 1.0];
const WHITE: [f64; 3] = [1.0, 1.0, 1.0];
/// Clipboard "rulings": the accent at ~30% over white.
const RULE: [f64; 3] = [0.835, 0.827, 1.0];

const SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "assets/icon.ico".into());

    let mut dir = IconDir::new(ResourceType::Icon);
    for size in SIZES {
        let rgba = render(size);        let image = IconImage::from_rgba_data(size, size, rgba);
        // PNG keeps the 256px entry small; smaller sizes stay as plain DIBs so
        // every shell code path (including the old one) can read them.
        let entry = if size >= 128 {
            IconDirEntry::encode_as_png(&image)?
        } else {
            IconDirEntry::encode_as_bmp(&image)?
        };
        dir.add_entry(entry);
    }

    if let Some(dir) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = BufWriter::new(File::create(&path)?);
    dir.write(&mut out)?;
    println!("wrote {path} ({} sizes)", SIZES.len());
    Ok(())
}

/// Render one square icon into straight RGBA bytes.
fn render(size: u32) -> Vec<u8> {
    // The 16–24px entries get a simplified drawing: at that size the white ring
    // around the clip and the third ruling turn into mush.
    let small = size < 32;
    // Supersample, then box-filter down. Everything is drawn in 0..1 space so
    // the geometry is identical at every size.
    let ss = if size <= 64 { 8 } else { 4 };
    let n = size * ss;
    let mut out = vec![0u8; (size * size * 4) as usize];

    for py in 0..size {
        for px in 0..size {
            let mut acc = [0f64; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = (px * ss + sx) as f64 / n as f64;
                    let y = (py * ss + sy) as f64 / n as f64;
                    let (rgb, a) = shade(x, y, small);
                    acc[0] += rgb[0] * a;
                    acc[1] += rgb[1] * a;
                    acc[2] += rgb[2] * a;
                    acc[3] += a;
                }
            }
            let samples = (ss * ss) as f64;
            let a = acc[3] / samples;
            let i = ((py * size + px) * 4) as usize;
            if a > 0.0 {
                // Un-premultiply back to straight alpha.
                out[i] = to_u8(acc[0] / acc[3]);
                out[i + 1] = to_u8(acc[1] / acc[3]);
                out[i + 2] = to_u8(acc[2] / acc[3]);
            }
            out[i + 3] = to_u8(a);
        }
    }
    out
}

fn to_u8(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Colour and coverage of one sub-pixel sample.
fn shade(x: f64, y: f64, small: bool) -> ([f64; 3], f64) {
    // Rounded accent tile.
    if !rounded_rect(x, y, 0.015, 0.015, 0.985, 0.985, 0.235) {
        return ([0.0; 3], 0.0);
    }
    // Slight top-left light so the tile does not look flat on the taskbar.
    let lift = 0.10 * ((1.0 - x) * 0.6 + (1.0 - y) * 0.4);
    let mut rgb = [
        ACCENT[0] + (1.0 - ACCENT[0]) * lift,
        ACCENT[1] + (1.0 - ACCENT[1]) * lift,
        ACCENT[2] + (1.0 - ACCENT[2]) * lift,
    ];

    // Clipboard body.
    if rounded_rect(x, y, 0.285, 0.175, 0.715, 0.845, 0.105) {
        rgb = WHITE;
        let rules: &[(f64, f64)] = if small {
            &[(0.44, 0.07), (0.62, 0.07)]
        } else {
            &[(0.395, 0.05), (0.515, 0.05), (0.635, 0.05)]
        };
        for &(top, height) in rules {
            if rounded_rect(x, y, 0.365, top, 0.635, top + height, height / 2.0) {
                rgb = RULE;
            }
        }
    }

    // Clip at the top of the board.
    if small {
        // Solid tab: over the body it reads as the notch the tray icon draws,
        // and above the body it merges into the tile.
        if rounded_rect(x, y, 0.385, 0.115, 0.615, 0.30, 0.075) {
            rgb = ACCENT;
        }
    } else {
        // A white ring around an accent tab, so the clip still reads as a
        // clipboard where it pokes above the board.
        if rounded_rect(x, y, 0.375, 0.095, 0.625, 0.295, 0.09) {
            rgb = WHITE;
        }
        if rounded_rect(x, y, 0.398, 0.118, 0.602, 0.272, 0.065) {
            rgb = ACCENT;
        }
    }

    (rgb, 1.0)
}

/// Point-in-rounded-rectangle test; `r` is the corner radius.
fn rounded_rect(x: f64, y: f64, x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> bool {
    let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    let (dx, dy) = (x - cx, y - cy);
    dx * dx + dy * dy <= r * r
}
