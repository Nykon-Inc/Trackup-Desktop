use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::imageops::{resize, FilterType};
use image::{ImageBuffer, Rgba};
use imageproc::drawing::{draw_text_mut, text_size};
use tauri::image::Image;

#[cfg(target_os = "windows")]
const BASE_HEIGHT: u32 = 64;

#[cfg(not(target_os = "windows"))]
const BASE_HEIGHT: u32 = 40;

const FONT_DATA: &[u8] = include_bytes!("Roboto-Regular.ttf");
const ICON_DATA: &[u8] = include_bytes!("../icons/32x32.png");
pub fn generate_tray_icon(text: &str) -> Option<Image<'static>> {
    let font = FontRef::try_from_slice(FONT_DATA).expect("Error constructing Font");

    // We want a high-res render to ensure smooth text and rounded corners
    let scale_factor = 4; // Render at 4x resolution

    // "Bigger" text
    #[cfg(target_os = "windows")]
    let base_text_size = 42.0;

    #[cfg(not(target_os = "windows"))]
    let base_text_size = 32.0;
    let render_scale = PxScale {
        x: base_text_size * scale_factor as f32,
        y: base_text_size * scale_factor as f32,
    };

    let scaled_font = font.as_scaled(render_scale);
    let ascent = scaled_font.ascent();
    let (text_w, _) = text_size(render_scale, &font, text);

    // "Less packed" -> More padding
    let base_icon_size = BASE_HEIGHT; // New size to match base_height
    let render_icon_size = base_icon_size * scale_factor;

    #[cfg(target_os = "windows")]
    let base_padding_h = 32;

    #[cfg(not(target_os = "windows"))]
    let base_padding_h = 24;
    let render_padding_h = base_padding_h * scale_factor;

    // "Bigger" overall -> Taller height
    let base_height = BASE_HEIGHT;
    let render_height = base_height * scale_factor;

    let render_width = render_icon_size + render_padding_h + text_w as u32 + render_padding_h;

    let mut canvas =
        ImageBuffer::from_pixel(render_width, render_height, Rgba([255, 255, 255, 255]));

    // 1. Draw Icon
    if let Ok(icon_img) = image::load_from_memory(ICON_DATA) {
        let icon_resized = resize(
            &icon_img.to_rgba8(),
            render_icon_size,
            render_icon_size,
            FilterType::Lanczos3,
        );
        image::imageops::overlay(&mut canvas, &icon_resized, 0, 0);
    }

    // 2. Centering Logic (Adjusted)
    let text_x = (render_icon_size + render_padding_h) as i32;

    // OPTICAL ADJUSTMENT:
    // We take the center of the canvas and subtract half the ascent,
    // then subtract a bit more (15% of render_height) to push it "up".
    let optical_offset = (render_height as f32 * 0.10) as i32;
    let text_y = ((render_height as f32 / 2.0 - ascent / 2.0) as i32) - optical_offset;

    let text_color = Rgba([10, 10, 10, 255]);

    // 3. Draw Thicker Text (Increased thickness multiplier)
    let thickness = (0.1 * scale_factor as f32) as i32;
    for dx in 0..=thickness {
        for dy in 0..=thickness {
            draw_text_mut(
                &mut canvas,
                text_color,
                text_x + dx,
                text_y + dy,
                render_scale,
                &font,
                text,
            );
        }
    }

    // 4. Downscale & Corners
    let target_width = render_width / scale_factor;
    let target_height = render_height / scale_factor;
    let mut small = resize(&canvas, target_width, target_height, FilterType::Lanczos3);

    #[cfg(target_os = "windows")]
    apply_rounded_corners(&mut small, 8.0);

    #[cfg(not(target_os = "windows"))]
    apply_rounded_corners(&mut small, 4.0);

    let raw_pixels = small.into_raw();
    Some(Image::new_owned(raw_pixels, target_width, target_height))
}

fn apply_rounded_corners(img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, radius: f32) {
    let (w, h) = img.dimensions();
    let width = w as f32;
    let height = h as f32;

    // We iterate over all pixels, but logic only applies to corners
    for y in 0..h {
        for x in 0..w {
            let px = x as f32 + 0.5; // pixel center x
            let py = y as f32 + 0.5; // pixel center y

            // Vector from the nearest corner center
            let dx;
            let dy;

            if x < radius as u32 && y < radius as u32 {
                // Top-Left
                dx = px - radius;
                dy = py - radius;
            } else if x >= (w - radius as u32) && y < radius as u32 {
                // Top-Right
                dx = px - (width - radius);
                dy = py - radius;
            } else if x < radius as u32 && y >= (h - radius as u32) {
                // Bottom-Left
                dx = px - radius;
                dy = py - (height - radius);
            } else if x >= (w - radius as u32) && y >= (h - radius as u32) {
                // Bottom-Right
                dx = px - (width - radius);
                dy = py - (height - radius);
            } else {
                continue; // Not in a corner region
            }

            // Distance from corner center
            let dist_sq = dx * dx + dy * dy;

            // Optimization: if fully inside circle (dist < radius-1), do nothing
            // if fully outside (dist > radius), make transparent
            // if on edge, blend

            // We are solving for opacity.
            // The corner is formed by the circle. Pixels outside the circle (dist > radius) should be transparent.
            // Pixels inside (dist < radius) should be consistent with image.

            // Let's optimize calculation: avoid sqrt if obvious
            if dist_sq > (radius + 1.0) * (radius + 1.0) {
                img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                continue;
            }

            let dist = dist_sq.sqrt();

            // Alpha factor: 0.0 (fully outside) to 1.0 (fully inside)
            // Smooth transition around `radius`
            // dist = radius + 0.5 -> alpha = 0
            // dist = radius - 0.5 -> alpha = 1

            let alpha_factor = (radius + 0.5 - dist).clamp(0.0, 1.0);

            if alpha_factor < 1.0 {
                let p = img.get_pixel(x, y);
                let new_alpha = (p[3] as f32 * alpha_factor) as u8;
                img.put_pixel(x, y, Rgba([p[0], p[1], p[2], new_alpha]));
            }
        }
    }
}
