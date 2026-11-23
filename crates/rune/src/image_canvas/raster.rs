//! Software anti-aliased rasterizer for exporting [`ImageCanvasState`](super::ImageCanvasState)
//! annotations and crop rectangles to RGBA8 / PNG bytes.

use crate::color::Color;
use crate::image::{ImageError, Pixels};

use super::{CanvasAnnotation, CropRect, arrow_wings};

pub(super) fn render_rgba(
    pixels: &Pixels,
    annotations: &[CanvasAnnotation],
    crop: Option<CropRect>,
) -> Result<(u32, u32, Vec<u8>), ImageError> {
    let (src_w, src_h, src) = pixels.to_rgba8();
    if src_w == 0 || src_h == 0 || src.len() != (src_w as usize) * (src_h as usize) * 4 {
        return Err(ImageError(
            "source image has no decoded pixels to export".into(),
        ));
    }
    let (cx, cy, out_w, out_h) = match crop {
        Some(c) => c.to_pixels(src_w, src_h),
        None => (0, 0, src_w, src_h),
    };

    let mut buf = vec![0u8; (out_w as usize) * (out_h as usize) * 4];
    for row in 0..out_h {
        let src_start = (((cy + row) as usize) * (src_w as usize) + (cx as usize)) * 4;
        let dst_start = ((row as usize) * (out_w as usize)) * 4;
        let row_bytes = (out_w as usize) * 4;
        buf[dst_start..dst_start + row_bytes]
            .copy_from_slice(&src[src_start..src_start + row_bytes]);
    }

    let ox = cx as f32;
    let oy = cy as f32;
    for ann in annotations {
        match ann {
            CanvasAnnotation::Stroke {
                points,
                color,
                width,
            } => {
                if points.len() == 1 {
                    let p = [points[0][0] - ox, points[0][1] - oy];
                    draw_capsule(&mut buf, out_w, out_h, p, p, width * 0.5, *color);
                } else {
                    for seg in points.windows(2) {
                        let a = [seg[0][0] - ox, seg[0][1] - oy];
                        let b = [seg[1][0] - ox, seg[1][1] - oy];
                        draw_capsule(&mut buf, out_w, out_h, a, b, width * 0.5, *color);
                    }
                }
            }
            CanvasAnnotation::Arrow {
                from,
                to,
                color,
                width,
            } => {
                let a = [from[0] - ox, from[1] - oy];
                let b = [to[0] - ox, to[1] - oy];
                draw_capsule(&mut buf, out_w, out_h, a, b, width * 0.5, *color);
                if let Some((w1, w2)) = arrow_wings(a, b, *width) {
                    draw_capsule(&mut buf, out_w, out_h, b, w1, width * 0.5, *color);
                    draw_capsule(&mut buf, out_w, out_h, b, w2, width * 0.5, *color);
                }
            }
            CanvasAnnotation::Rect {
                x,
                y,
                w,
                h,
                color,
                width,
                fill,
            } => {
                let rx = *x - ox;
                let ry = *y - oy;
                if let Some(bg) = fill {
                    fill_rect(&mut buf, out_w, out_h, rx, ry, *w, *h, *bg);
                }
                if *width > 0.0 {
                    let half = width * 0.5;
                    fill_rect(
                        &mut buf,
                        out_w,
                        out_h,
                        rx - half,
                        ry - half,
                        *w + *width,
                        *width,
                        *color,
                    );
                    fill_rect(
                        &mut buf,
                        out_w,
                        out_h,
                        rx - half,
                        ry + *h - half,
                        *w + *width,
                        *width,
                        *color,
                    );
                    fill_rect(
                        &mut buf,
                        out_w,
                        out_h,
                        rx - half,
                        ry + half,
                        *width,
                        (*h - *width).max(0.0),
                        *color,
                    );
                    fill_rect(
                        &mut buf,
                        out_w,
                        out_h,
                        rx + *w - half,
                        ry + half,
                        *width,
                        (*h - *width).max(0.0),
                        *color,
                    );
                }
            }
        }
    }
    Ok((out_w, out_h, buf))
}

fn fill_rect(buf: &mut [u8], bw: u32, bh: u32, x: f32, y: f32, w: f32, h: f32, color: Color) {
    if w <= 0.0 || h <= 0.0 || color.a <= 0.0 {
        return;
    }
    let x0 = (x.floor().max(0.0) as u32).min(bw);
    let y0 = (y.floor().max(0.0) as u32).min(bh);
    let x1 = ((x + w).ceil().max(0.0) as u32).min(bw);
    let y1 = ((y + h).ceil().max(0.0) as u32).min(bh);
    for py in y0..y1 {
        let cy = ((py as f32 + 1.0).min(y + h) - (py as f32).max(y)).clamp(0.0, 1.0);
        for px in x0..x1 {
            let cx = ((px as f32 + 1.0).min(x + w) - (px as f32).max(x)).clamp(0.0, 1.0);
            blend_pixel(buf, bw, px, py, color, cx * cy);
        }
    }
}

fn draw_capsule(
    buf: &mut [u8],
    bw: u32,
    bh: u32,
    a: [f32; 2],
    b: [f32; 2],
    radius: f32,
    color: Color,
) {
    let r = radius.max(0.5);
    let pad = r + 1.0;
    let x0 = (a[0].min(b[0]) - pad).floor().max(0.0) as u32;
    let y0 = (a[1].min(b[1]) - pad).floor().max(0.0) as u32;
    let x1 = ((a[0].max(b[0]) + pad).ceil().max(0.0) as u32).min(bw);
    let y1 = ((a[1].max(b[1]) + pad).ceil().max(0.0) as u32).min(bh);
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len_sq = dx * dx + dy * dy;

    for py in y0..y1 {
        let p_y = py as f32 + 0.5;
        for px in x0..x1 {
            let p_x = px as f32 + 0.5;
            let t = if len_sq > 1e-6 {
                (((p_x - a[0]) * dx + (p_y - a[1]) * dy) / len_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let cx = a[0] + t * dx;
            let cy = a[1] + t * dy;
            let dist = (p_x - cx).hypot(p_y - cy);
            let cov = (r + 0.5 - dist).clamp(0.0, 1.0);
            if cov > 0.0 {
                blend_pixel(buf, bw, px, py, color, cov);
            }
        }
    }
}

fn blend_pixel(buf: &mut [u8], bw: u32, x: u32, y: u32, color: Color, coverage: f32) {
    let sa = (color.a * coverage).clamp(0.0, 1.0);
    if sa <= 0.0 {
        return;
    }
    let idx = ((y as usize) * (bw as usize) + (x as usize)) * 4;
    let da = f32::from(buf[idx + 3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return;
    }
    let mix = |sc: f32, dc: u8| -> u8 {
        let df = f32::from(dc) / 255.0;
        let out = (sc * sa + df * da * (1.0 - sa)) / out_a;
        (out.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    };
    buf[idx] = mix(color.r, buf[idx]);
    buf[idx + 1] = mix(color.g, buf[idx + 1]);
    buf[idx + 2] = mix(color.b, buf[idx + 2]);
    buf[idx + 3] = (out_a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
}
