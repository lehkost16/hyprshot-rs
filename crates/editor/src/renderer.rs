use crate::annotations::{Annotation, StrokeType};
use fontdue::{Font, FontSettings};
use image::RgbaImage;
use std::sync::Arc;
use tiny_skia::*;

fn load_font_bytes() -> Vec<u8> {
    for query in [":lang=zh-cn", "Source Han Sans CN", "Noto Sans CJK SC", "WenQuanYi Micro Hei"] {
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["--format=%{file}", query])
            .output()
        {
            if output.status.success() {
                if let Ok(path_str) = String::from_utf8(output.stdout) {
                    let path = path_str.trim();
                    if let Ok(bytes) = std::fs::read(path) {
                        if !bytes.is_empty() {
                            return bytes;
                        }
                    }
                }
            }
        }
    }
    for fallback in [
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/adobe-source-han-sans/SourceHanSansCN-Regular.otf",
        "/usr/share/fonts/TTF/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/WindowsFonts/msyh.ttc",
        "/usr/share/fonts/inter/Inter.ttc",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(fallback) {
            if !bytes.is_empty() {
                return bytes;
            }
        }
    }
    Vec::new()
}

fn lazy_static_font() -> &'static Font {
    static FONT: std::sync::OnceLock<Font> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        let font_data = load_font_bytes();
        Font::from_bytes(font_data, FontSettings::default()).expect("Failed to load system font")
    })
}

pub fn fast_copy_rgba_to_pixmap(base: &RgbaImage, pixmap: &mut Pixmap) {
    let src = base.as_raw();
    let dst = pixmap.data_mut();
    for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
        let a = s[3];
        if a == 255 {
            d.copy_from_slice(s);
        } else if a == 0 {
            d[0] = 0;
            d[1] = 0;
            d[2] = 0;
            d[3] = 0;
        } else {
            let alpha = a as u32;
            d[0] = ((s[0] as u32 * alpha) / 255) as u8;
            d[1] = ((s[1] as u32 * alpha) / 255) as u8;
            d[2] = ((s[2] as u32 * alpha) / 255) as u8;
            d[3] = a;
        }
    }
}

pub fn pixmap_to_rgba_image(pixmap: &Pixmap) -> RgbaImage {
    let width = pixmap.width();
    let height = pixmap.height();
    let mut output = RgbaImage::new(width, height);
    for (src_chunk, dst_chunk) in pixmap.data().chunks_exact(4).zip(output.chunks_exact_mut(4)) {
        let a = src_chunk[3];
        if a == 0 {
            dst_chunk[0] = 0;
            dst_chunk[1] = 0;
            dst_chunk[2] = 0;
            dst_chunk[3] = 0;
        } else if a == 255 {
            dst_chunk.copy_from_slice(src_chunk);
        } else {
            let inv_alpha = 255.0 / a as f32;
            dst_chunk[0] = ((src_chunk[0] as f32 * inv_alpha).min(255.0)).round() as u8;
            dst_chunk[1] = ((src_chunk[1] as f32 * inv_alpha).min(255.0)).round() as u8;
            dst_chunk[2] = ((src_chunk[2] as f32 * inv_alpha).min(255.0)).round() as u8;
            dst_chunk[3] = a;
        }
    }
    output
}

pub fn pixmap_to_render_image(mut pixmap: Pixmap) -> Arc<gpui::RenderImage> {
    let width = pixmap.width();
    let height = pixmap.height();
    for chunk in pixmap.data_mut().chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    let img = RgbaImage::from_raw(width, height, pixmap.take()).unwrap();
    let frame = image::Frame::new(img);
    Arc::new(gpui::RenderImage::new(vec![frame]))
}

pub fn render_composed_image(base: &Arc<RgbaImage>, annotations: &[Annotation]) -> RgbaImage {
    let width = base.width();
    let height = base.height();
    let mut pixmap = Pixmap::new(width, height).expect("Failed to create tiny-skia pixmap");
    fast_copy_rgba_to_pixmap(base, &mut pixmap);

    for ann in annotations {
        render_annotation(&mut pixmap, base, ann);
    }

    pixmap_to_rgba_image(&pixmap)
}

fn to_color(rgba: [u8; 4]) -> Color {
    Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

fn apply_stroke_style(stroke: &mut Stroke, stroke_type: StrokeType) {
    match stroke_type {
        StrokeType::SolidLine => {}
        StrokeType::DashedLine => {
            stroke.dash = StrokeDash::new(vec![6.0, 3.0], 0.0);
        }
        StrokeType::DottedLine => {
            stroke.dash = StrokeDash::new(vec![2.0, 4.0], 0.0);
            stroke.line_cap = LineCap::Round;
        }
    }
}

pub fn render_annotation(pixmap: &mut Pixmap, base: &Arc<RgbaImage>, ann: &Annotation) {
    match ann {
        Annotation::Rectangle {
            start,
            end,
            color,
            width,
            stroke_type,
            fill_color,
        } => {
            let x = start.x.min(end.x);
            let y = start.y.min(end.y);
            let w = (start.x - end.x).abs();
            let h = (start.y - end.y).abs();
            if w < 1.0 || h < 1.0 {
                return;
            }
            if let Some(rect) = Rect::from_xywh(x, y, w, h) {
                let path = PathBuilder::from_rect(rect);
                // Optional fill
                if let Some(fill_c) = fill_color {
                    let mut fill_paint = Paint::default();
                    fill_paint.set_color(to_color(*fill_c));
                    fill_paint.anti_alias = true;
                    pixmap.fill_path(&path, &fill_paint, FillRule::Winding, Transform::identity(), None);
                }
                // Stroke
                let mut paint = Paint::default();
                paint.set_color(to_color(*color));
                paint.anti_alias = true;
                let mut stroke = Stroke {
                    width: *width,
                    ..Default::default()
                };
                apply_stroke_style(&mut stroke, *stroke_type);
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        Annotation::Ellipse {
            start,
            end,
            color,
            width,
            stroke_type,
            fill_color,
        } => {
            let x = start.x.min(end.x);
            let y = start.y.min(end.y);
            let w = (start.x - end.x).abs();
            let h = (start.y - end.y).abs();
            if w < 1.0 || h < 1.0 {
                return;
            }
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let rx = w / 2.0;
            let ry = h / 2.0;
            let kappa = 0.5522847498;
            let ox = rx * kappa;
            let oy = ry * kappa;
            let mut pb = PathBuilder::new();
            pb.move_to(cx - rx, cy);
            pb.cubic_to(cx - rx, cy - oy, cx - ox, cy - ry, cx, cy - ry);
            pb.cubic_to(cx + ox, cy - ry, cx + rx, cy - oy, cx + rx, cy);
            pb.cubic_to(cx + rx, cy + oy, cx + ox, cy + ry, cx, cy + ry);
            pb.cubic_to(cx - ox, cy + ry, cx - rx, cy + oy, cx - rx, cy);
            pb.close();
            if let Some(path) = pb.finish() {
                if let Some(fill_c) = fill_color {
                    let mut fill_paint = Paint::default();
                    fill_paint.set_color(to_color(*fill_c));
                    fill_paint.anti_alias = true;
                    pixmap.fill_path(&path, &fill_paint, FillRule::Winding, Transform::identity(), None);
                }
                let mut paint = Paint::default();
                paint.set_color(to_color(*color));
                paint.anti_alias = true;
                let mut stroke = Stroke {
                    width: *width,
                    ..Default::default()
                };
                apply_stroke_style(&mut stroke, *stroke_type);
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        Annotation::Line {
            start,
            end,
            color,
            width,
            stroke_type,
        } => {
            let mut pb = PathBuilder::new();
            pb.move_to(start.x, start.y);
            pb.line_to(end.x, end.y);
            if let Some(path) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color(to_color(*color));
                paint.anti_alias = true;
                let mut stroke = Stroke {
                    width: *width,
                    line_cap: LineCap::Round,
                    ..Default::default()
                };
                apply_stroke_style(&mut stroke, *stroke_type);
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        Annotation::Arrow {
            start,
            end,
            color,
            width,
        } => {
            let mut pb = PathBuilder::new();
            pb.move_to(start.x, start.y);
            pb.line_to(end.x, end.y);

            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let len = (dx * dx + dy * dy).sqrt();
            if len > 5.0 {
                let udx = dx / len;
                let udy = dy / len;
                let head_len = (width * 4.0).max(12.0).min(30.0);
                let head_angle = std::f32::consts::PI / 6.0;

                let sin_a = head_angle.sin();
                let cos_a = head_angle.cos();

                let x1 = end.x - head_len * (udx * cos_a - udy * sin_a);
                let y1 = end.y - head_len * (udy * cos_a + udx * sin_a);
                let x2 = end.x - head_len * (udx * cos_a + udy * sin_a);
                let y2 = end.y - head_len * (udy * cos_a - udx * sin_a);

                pb.move_to(x1, y1);
                pb.line_to(end.x, end.y);
                pb.line_to(x2, y2);
            }

            if let Some(path) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color(to_color(*color));
                paint.anti_alias = true;
                let stroke = Stroke {
                    width: *width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        Annotation::Freehand { points, color, width } => {
            if points.len() < 2 {
                return;
            }
            let mut pb = PathBuilder::new();
            pb.move_to(points[0].x, points[0].y);
            for p in &points[1..] {
                pb.line_to(p.x, p.y);
            }
            if let Some(path) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color(to_color(*color));
                paint.anti_alias = true;
                let stroke = Stroke {
                    width: *width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        Annotation::Highlight {
            points,
            color,
            width,
            straight,
        } => {
            if points.len() < 2 {
                return;
            }
            let mut pb = PathBuilder::new();
            if *straight {
                pb.move_to(points[0].x, points[0].y);
                let last = points.last().unwrap();
                pb.line_to(last.x, last.y);
            } else {
                pb.move_to(points[0].x, points[0].y);
                for p in &points[1..] {
                    pb.line_to(p.x, p.y);
                }
            }
            if let Some(path) = pb.finish() {
                let mut paint = Paint::default();
                let mut c = *color;
                c[3] = 112; // Standard 112 alpha for highlighter
                paint.set_color(to_color(c));
                paint.anti_alias = true;
                let stroke = Stroke {
                    width: *width * 3.0,
                    line_cap: LineCap::Square,
                    line_join: LineJoin::Round,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        Annotation::Mosaic { rect, block_size } => {
            let min_x = rect[0].min(rect[2]);
            let min_y = rect[1].min(rect[3]);
            let max_x = rect[0].max(rect[2]);
            let max_y = rect[1].max(rect[3]);
            let x0 = (min_x.max(0.0) as u32).min(pixmap.width());
            let y0 = (min_y.max(0.0) as u32).min(pixmap.height());
            let x1 = (max_x.max(0.0) as u32).min(pixmap.width());
            let y1 = (max_y.max(0.0) as u32).min(pixmap.height());
            let b = (*block_size as u32).max(4);

            let w = pixmap.width();
            let data = pixmap.data_mut();

            for by in (y0..y1).step_by(b as usize) {
                for bx in (x0..x1).step_by(b as usize) {
                    let max_x = (bx + b).min(x1);
                    let max_y = (by + b).min(y1);

                    let mut r_sum = 0u64;
                    let mut g_sum = 0u64;
                    let mut b_sum = 0u64;
                    let mut a_sum = 0u64;
                    let mut count = 0u64;

                    for py in by..max_y {
                        for px in bx..max_x {
                            let idx = ((py * w + px) * 4) as usize;
                            r_sum += data[idx] as u64;
                            g_sum += data[idx + 1] as u64;
                            b_sum += data[idx + 2] as u64;
                            a_sum += data[idx + 3] as u64;
                            count += 1;
                        }
                    }

                    if count > 0 {
                        let avg_r = (r_sum / count) as u8;
                        let avg_g = (g_sum / count) as u8;
                        let avg_b = (b_sum / count) as u8;
                        let avg_a = (a_sum / count) as u8;

                        for py in by..max_y {
                            for px in bx..max_x {
                                let idx = ((py * w + px) * 4) as usize;
                                data[idx] = avg_r;
                                data[idx + 1] = avg_g;
                                data[idx + 2] = avg_b;
                                data[idx + 3] = avg_a;
                            }
                        }
                    }
                }
            }
        }
        Annotation::Blur { rect, sigma: _ } => {
            let min_x = rect[0].min(rect[2]);
            let min_y = rect[1].min(rect[3]);
            let max_x = rect[0].max(rect[2]);
            let max_y = rect[1].max(rect[3]);
            let x0 = (min_x.max(0.0) as u32).min(pixmap.width());
            let y0 = (min_y.max(0.0) as u32).min(pixmap.height());
            let x1 = (max_x.max(0.0) as u32).min(pixmap.width());
            let y1 = (max_y.max(0.0) as u32).min(pixmap.height());
            if x1 <= x0 || y1 <= y0 {
                return;
            }

            let w = pixmap.width() as i32;
            let radius = 7i32; // Smooth box blur passes
            let data = pixmap.data_mut();

            // Two-pass box blur over rect region
            for py in y0..y1 {
                for px in x0..x1 {
                    let mut r_tot = 0u32;
                    let mut g_tot = 0u32;
                    let mut b_tot = 0u32;
                    let mut a_tot = 0u32;
                    let mut cnt = 0u32;

                    for dy in -radius..=radius {
                        let sy = (py as i32 + dy).clamp(y0 as i32, (y1 - 1) as i32);
                        let idx = ((sy * w + px as i32) * 4) as usize;
                        r_tot += data[idx] as u32;
                        g_tot += data[idx + 1] as u32;
                        b_tot += data[idx + 2] as u32;
                        a_tot += data[idx + 3] as u32;
                        cnt += 1;
                    }
                    if cnt > 0 {
                        let idx = ((py as i32 * w + px as i32) * 4) as usize;
                        data[idx] = (r_tot / cnt) as u8;
                        data[idx + 1] = (g_tot / cnt) as u8;
                        data[idx + 2] = (b_tot / cnt) as u8;
                        data[idx + 3] = (a_tot / cnt) as u8;
                    }
                }
            }
        }
        Annotation::Badge {
            pos,
            number,
            color,
            radius,
        } => {
            let r = *radius;
            let cx = pos.x;
            let cy = pos.y;
            let mut pb = PathBuilder::new();
            pb.push_circle(cx, cy, r);
            if let Some(path) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color(to_color(*color));
                paint.anti_alias = true;
                pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);

                let mut stroke_paint = Paint::default();
                stroke_paint.set_color(Color::WHITE);
                stroke_paint.anti_alias = true;
                let stroke = Stroke {
                    width: 2.0,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &stroke_paint, &stroke, Transform::identity(), None);

                let num_str = number.to_string();
                render_text_at(pixmap, &num_str, cx, cy, r * 1.2, [255, 255, 255, 255], true);
            }
        }
        Annotation::Text {
            pos,
            text,
            color,
            font_size,
        } => {
            render_text_at(pixmap, text, pos.x, pos.y, *font_size, *color, false);
        }
        Annotation::Eraser { rect } => {
            // Restore original pixels from base image in rect
            let min_x = rect[0].min(rect[2]);
            let min_y = rect[1].min(rect[3]);
            let max_x = rect[0].max(rect[2]);
            let max_y = rect[1].max(rect[3]);
            let x0 = (min_x.max(0.0) as u32).min(pixmap.width());
            let y0 = (min_y.max(0.0) as u32).min(pixmap.height());
            let x1 = (max_x.max(0.0) as u32).min(pixmap.width());
            let y1 = (max_y.max(0.0) as u32).min(pixmap.height());

            let w = pixmap.width();
            let base_raw = base.as_raw();
            let data = pixmap.data_mut();

            for py in y0..y1 {
                for px in bx_iter(x0, x1) {
                    let idx = ((py * w + px) * 4) as usize;
                    let r = base_raw[idx];
                    let g = base_raw[idx + 1];
                    let b = base_raw[idx + 2];
                    let a = base_raw[idx + 3];
                    let alpha = a as f32 / 255.0;
                    data[idx] = (r as f32 * alpha).round() as u8;
                    data[idx + 1] = (g as f32 * alpha).round() as u8;
                    data[idx + 2] = (b as f32 * alpha).round() as u8;
                    data[idx + 3] = a;
                }
            }
        }
    }
}

fn bx_iter(x0: u32, x1: u32) -> std::ops::Range<u32> {
    x0..x1
}

fn render_text_at(
    pixmap: &mut Pixmap,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [u8; 4],
    center: bool,
) {
    let font = lazy_static_font();
    let mut total_width = 0.0f32;
    for ch in text.chars() {
        let metrics = font.metrics(ch, size);
        total_width += metrics.advance_width;
    }

    let start_x = if center { x - total_width / 2.0 } else { x };
    let start_y = if center { y - size / 2.0 } else { y };

    let w = pixmap.width() as i32;
    let h = pixmap.height() as i32;
    let data = pixmap.data_mut();

    let text_color = to_color(color);
    let alpha_f = text_color.alpha();
    let r_prem = (text_color.red() * 255.0).round() as u32;
    let g_prem = (text_color.green() * 255.0).round() as u32;
    let b_prem = (text_color.blue() * 255.0).round() as u32;

    let mut cur_y = start_y;
    for line in text.split('\n') {
        let mut cur_x = start_x;
        let baseline_y = if let Some(lm) = font.horizontal_line_metrics(size) {
            let total_font_h = lm.ascent - lm.descent;
            let padding_top = (size - total_font_h) / 2.0;
            cur_y + padding_top + lm.ascent
        } else {
            cur_y + size * 0.936
        };

        for ch in line.chars() {
            let (metrics, bitmap) = font.rasterize(ch, size);
            let gx = cur_x + metrics.xmin as f32;
            let gy = baseline_y - metrics.height as f32 - metrics.ymin as f32;

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let coverage = bitmap[row * metrics.width + col] as f32 / 255.0;
                    if coverage <= 0.0 {
                        continue;
                    }
                    let px = (gx + col as f32).round() as i32;
                    let py = (gy + row as f32).round() as i32;

                    if px >= 0 && px < w && py >= 0 && py < h {
                        let idx = ((py * w + px) * 4) as usize;
                        let cov_alpha = coverage * alpha_f;

                        let inv_a = 1.0 - cov_alpha;
                        let dst_r = data[idx] as f32;
                        let dst_g = data[idx + 1] as f32;
                        let dst_b = data[idx + 2] as f32;
                        let dst_a = data[idx + 3] as f32;

                        data[idx] = ((r_prem as f32 * cov_alpha) + dst_r * inv_a).min(255.0) as u8;
                        data[idx + 1] = ((g_prem as f32 * cov_alpha) + dst_g * inv_a).min(255.0) as u8;
                        data[idx + 2] = ((b_prem as f32 * cov_alpha) + dst_b * inv_a).min(255.0) as u8;
                        data[idx + 3] = ((255.0 * cov_alpha) + dst_a * inv_a).min(255.0) as u8;
                    }
                }
            }
            cur_x += metrics.advance_width;
        }
        cur_y += size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_rasterizes_chinese_characters() {
        let mut pixmap = Pixmap::new(200, 100).unwrap();
        render_text_at(&mut pixmap, "汉字测试", 10.0, 10.0, 24.0, [255, 0, 0, 255], false);
        // Ensure some non-zero pixels were drawn
        let non_zero_count = pixmap.data().iter().filter(|&&b| b > 0).count();
        assert!(non_zero_count > 0, "Chinese characters failed to rasterize any pixels");
    }
}
