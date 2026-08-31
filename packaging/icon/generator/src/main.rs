//! Generates the OptiScaler Manager icon: three rounded squares stepping up
//! in size and opacity (the upscaling metaphor) on a violet→indigo gradient.

use image::{Rgba, RgbaImage, imageops::FilterType};

const SIZE: u32 = 1024;

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Signed distance to a rounded rectangle centered at (cx, cy).
fn sd_rounded_rect(px: f32, py: f32, cx: f32, cy: f32, half: f32, radius: f32) -> f32 {
    let dx = (px - cx).abs() - (half - radius);
    let dy = (py - cy).abs() - (half - radius);
    let ox = dx.max(0.0);
    let oy = dy.max(0.0);
    (ox * ox + oy * oy).sqrt() + dx.max(dy).min(0.0) - radius
}

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

fn main() {
    let mut img = RgbaImage::new(SIZE, SIZE);
    let s = SIZE as f32;

    // Background gradient: violet (top-left) -> deep indigo (bottom-right).
    let top = (109.0, 40.0, 217.0);    // #6D28D9
    let bot = (55.0, 48.0, 163.0);     // #3730A3
    let bg_half = s / 2.0;
    let bg_radius = s * 0.225;

    // The mark: three squares along the up-right diagonal.
    // (center_x, center_y, half_size, alpha)
    let squares = [
        (s * 0.28, s * 0.72, s * 0.082, 0.55f32),
        (s * 0.49, s * 0.51, s * 0.13, 0.78),
        (s * 0.715, s * 0.285, s * 0.195, 1.0),
    ];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            // Rounded-square canvas mask.
            let d_bg = sd_rounded_rect(px, py, bg_half, bg_half, bg_half, bg_radius);
            let bg_alpha = 1.0 - smoothstep(-1.5, 1.5, d_bg);
            if bg_alpha <= 0.0 {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                continue;
            }

            let t = ((px + py) / (2.0 * s)).clamp(0.0, 1.0);
            let mut r = lerp(top.0, bot.0, t);
            let mut g = lerp(top.1, bot.1, t);
            let mut b = lerp(top.2, bot.2, t);

            // Soft inner highlight top-left for depth.
            let hl = (1.0 - ((px / s - 0.3).powi(2) + (py / s - 0.25).powi(2)).sqrt()).max(0.0);
            let hl = hl * hl * 26.0;
            r = (r + hl).min(255.0);
            g = (g + hl).min(255.0);
            b = (b + hl).min(255.0);

            // Composite the white squares.
            for (cx, cy, half, alpha) in squares {
                let corner = half * 0.32;
                let d = sd_rounded_rect(px, py, cx, cy, half, corner);
                let a = (1.0 - smoothstep(-1.5, 1.5, d)) * alpha;
                if a > 0.0 {
                    r = lerp(r, 255.0, a);
                    g = lerp(g, 255.0, a);
                    b = lerp(b, 255.0, a);
                }
            }

            img.put_pixel(x, y, Rgba([r as u8, g as u8, b as u8, (bg_alpha * 255.0) as u8]));
        }
    }

    let out = std::path::Path::new(&std::env::args().nth(1).expect("out dir")).to_path_buf();
    std::fs::create_dir_all(&out).unwrap();

    // Master + preview PNGs.
    img.save(out.join("icon_1024.png")).unwrap();
    let png256 = image::imageops::resize(&img, 256, 256, FilterType::Lanczos3);
    png256.save(out.join("icon_256.png")).unwrap();

    // Multi-size .ico for Windows.
    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16u32, 24, 32, 48, 64, 128, 256] {
        let resized = image::imageops::resize(&img, size, size, FilterType::Lanczos3);
        let entry = ico::IconImage::from_rgba_data(size, size, resized.into_raw());
        dir.add_entry(ico::IconDirEntry::encode(&entry).unwrap());
    }
    let file = std::fs::File::create(out.join("optiscaler-manager.ico")).unwrap();
    dir.write(file).unwrap();
    println!("done");
}
