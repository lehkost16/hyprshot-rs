use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb};
use rayon::prelude::*;
use std::{
    io::{self, Read},
    path::Path,
    process::{Command, Stdio},
};

// ── Configuration ────────────────────────────────────────────────────

/// Number of column groups for 1D sampling (3 regions × 3 columns each).
const COL_GROUPS: usize = 9;
/// Minimum overlap fraction for matching.
const MIN_OVERLAP_FRAC: f32 = 0.10;
/// Number of frames of history for velocity estimation.
const VELOCITY_HISTORY: usize = 5;
/// Acceptable match: SAD < threshold * this → reliable.
const RELIABLE_MULTIPLIER: f32 = 2.5;

// ── Helpers ──────────────────────────────────────────────────────────

pub fn get_video_dimensions(video_path: &Path) -> Result<(usize, usize)> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height")
        .arg("-of")
        .arg("csv=p=0")
        .arg(video_path)
        .output()
        .context("ffprobe failed")?;
    if !output.status.success() {
        anyhow::bail!("ffprobe: {}", String::from_utf8_lossy(&output.stderr));
    }
    let s = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = s.trim().split(',').collect();
    if parts.len() != 2 {
        anyhow::bail!("invalid ffprobe output");
    }
    Ok((parts[0].parse()?, parts[1].parse()?))
}

/// Detect static top (Header) and bottom (Footer) borders by analyzing row variance across frames.
pub fn detect_static_borders(y_planes: &[&[u8]], w: usize, h: usize) -> (usize, usize) {
    if y_planes.len() < 2 || w == 0 || h == 0 {
        return (0, 0);
    }

    let max_header = h / 3;
    let max_footer = h / 3;

    // Check top rows
    let mut top_border = 0;
    for r in 0..max_header {
        let row_off = r * w;
        let mut row_diff = 0u64;
        let first_row = &y_planes[0][row_off..row_off + w];

        for plane in &y_planes[1..] {
            let other_row = &plane[row_off..row_off + w];
            for col in (0..w).step_by(2) {
                row_diff += (first_row[col] as i16 - other_row[col] as i16).unsigned_abs() as u64;
            }
        }
        let sampled_cols = w.div_ceil(2);
        let avg_diff = row_diff as f32 / ((y_planes.len() - 1) * sampled_cols) as f32;
        if avg_diff < 2.5 {
            top_border = r + 1;
        } else {
            break;
        }
    }

    // Check bottom rows
    let mut bottom_border = 0;
    for b in 0..max_footer {
        let r = h - 1 - b;
        let row_off = r * w;
        let mut row_diff = 0u64;
        let first_row = &y_planes[0][row_off..row_off + w];

        for plane in &y_planes[1..] {
            let other_row = &plane[row_off..row_off + w];
            for col in (0..w).step_by(2) {
                row_diff += (first_row[col] as i16 - other_row[col] as i16).unsigned_abs() as u64;
            }
        }
        let sampled_cols = w.div_ceil(2);
        let avg_diff = row_diff as f32 / ((y_planes.len() - 1) * sampled_cols) as f32;
        if avg_diff < 2.5 {
            bottom_border = b + 1;
        } else {
            break;
        }
    }

    (top_border, bottom_border)
}

/// Column sampling within active content bounds [top_border, H - bottom_border].
fn column_sample(
    gray: &[u8],
    w: usize,
    h: usize,
    top_border: usize,
    bottom_border: usize,
) -> Vec<f32> {
    let active_h = h.saturating_sub(top_border + bottom_border);
    if active_h == 0 {
        return Vec::new();
    }

    let cw = (w as f32) / 12.0;
    let mut col_ranges = Vec::with_capacity(COL_GROUPS);
    for g in 0..COL_GROUPS {
        let center = (cw * (g as f32 + 1.0) + cw * (g as f32 / 3.0).floor() * 9.0) as usize;
        let start = center.saturating_sub(1).min(w.saturating_sub(3));
        let end = (start + 3).min(w);
        col_ranges.push((start, end));
    }

    let mut out = vec![0.0f32; active_h * COL_GROUPS];
    out.par_chunks_exact_mut(COL_GROUPS)
        .enumerate()
        .for_each(|(row_idx, row_out)| {
            let actual_row = top_border + row_idx;
            let row_base = actual_row * w;
            for g in 0..COL_GROUPS {
                let (start, end) = col_ranges[g];
                let mut sum: u32 = 0;
                let mut count: u32 = 0;
                for col in start..end {
                    sum += gray[row_base + col] as u32;
                    count += 1;
                }
                row_out[g] = if count > 0 {
                    sum as f32 / count as f32
                } else {
                    0.0
                };
            }
        });
    out
}

/// Generate ordered offset candidates centered on `predict`, expanding outward.
fn predict_offset_sequence(max: i32, predict: i32) -> Vec<i32> {
    if max < 1 {
        return vec![0];
    }
    let p = predict.clamp(-max, max);
    let mut offsets = Vec::with_capacity((2 * max + 1) as usize);
    offsets.push(0);
    offsets.push(p);
    let mut d = 1i32;
    while offsets.len() < (2 * max + 1) as usize {
        for &cand in &[p + d, p - d, d, -d] {
            if cand != 0 && cand.abs() <= max && !offsets.contains(&cand) {
                offsets.push(cand);
            }
        }
        d += 1;
    }
    offsets
}

/// 2D patch verification to eliminate line skipping and disambiguate top candidates.
fn verify_2d_mad(
    y_a: &[u8],
    y_b: &[u8],
    w: usize,
    active_h: usize,
    top_border: usize,
    off: i32,
) -> f32 {
    let (a_start, b_start, overlap) = if off > 0 {
        let o = off as usize;
        (o, 0, active_h - o)
    } else if off < 0 {
        let o = (-off) as usize;
        (0, o, active_h - o)
    } else {
        (0, 0, active_h)
    };

    if overlap < 10 {
        return 255.0;
    }

    let strip_count = 5;
    let strip_h = 8;
    let step = overlap / (strip_count + 1);

    let mut total_diff: u64 = 0;
    let mut total_pixels: usize = 0;

    for s in 1..=strip_count {
        let rel_row = s * step;
        if rel_row + strip_h > overlap {
            break;
        }
        let a_row_base = (top_border + a_start + rel_row) * w;
        let b_row_base = (top_border + b_start + rel_row) * w;

        for r in 0..strip_h {
            let row_a_start = a_row_base + r * w;
            let row_b_start = b_row_base + r * w;
            for col in (0..w).step_by(2) {
                total_diff += (y_a[row_a_start + col] as i16 - y_b[row_b_start + col] as i16)
                    .unsigned_abs() as u64;
                total_pixels += 1;
            }
        }
    }

    if total_pixels == 0 {
        255.0
    } else {
        total_diff as f32 / total_pixels as f32
    }
}

/// Coarse-to-Fine matching: 1D SAD candidates + 2D MAD verification.
struct FrameSample<'a> {
    cols: &'a [f32],
    y: &'a [u8],
}

struct MatchInput<'a> {
    previous: FrameSample<'a>,
    current: FrameSample<'a>,
    w: usize,
    active_h: usize,
    top_border: usize,
    predict: i32,
    min_overlap: usize,
    sad_threshold: f32,
    debug: bool,
}

fn match_columns(input: MatchInput<'_>) -> (i32, f32) {
    let MatchInput {
        previous,
        current,
        w,
        active_h,
        top_border,
        predict,
        min_overlap,
        sad_threshold,
        debug,
    } = input;
    let max = active_h - min_overlap;
    if max < 1 {
        return (0, 255.0);
    }

    let offsets = predict_offset_sequence(max as i32, predict);
    let nc = COL_GROUPS;

    let mut min_1d_sad = 255.0f32;
    let mut candidates: Vec<(i32, f32)> = Vec::new();
    let mut stable_count: u32 = 0;

    for &off in &offsets {
        if off.unsigned_abs() as usize > max {
            continue;
        }

        let (a_start, b_start, overlap) = if off > 0 {
            let o = off as usize;
            (o, 0, active_h - o)
        } else if off < 0 {
            let o = (-off) as usize;
            (0, o, active_h - o)
        } else {
            (0, 0, active_h)
        };

        let mut sad = 0.0f64;
        for row in 0..overlap {
            let a_row = (a_start + row) * nc;
            let b_row = (b_start + row) * nc;
            for c in 0..nc {
                let diff = previous.cols[a_row + c] - current.cols[b_row + c];
                sad += (diff * diff) as f64;
            }
        }
        let avg = (sad / (overlap * nc) as f64).sqrt() as f32;

        if avg < min_1d_sad {
            min_1d_sad = avg;
            candidates.push((off, avg));
            if avg < sad_threshold {
                stable_count += 1;
                if stable_count > 8 {
                    break;
                }
            }
        } else if avg < min_1d_sad * 1.15 {
            candidates.push((off, avg));
        }

        if avg < sad_threshold * 0.1 {
            break;
        }
    }

    if candidates.is_empty() {
        return (0, 255.0);
    }

    candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    candidates.truncate(5);

    let mut best: (i32, f32) = candidates[0];
    let mut best_2d_mad = verify_2d_mad(previous.y, current.y, w, active_h, top_border, best.0);

    for &(off, avg_1d) in candidates.iter().skip(1) {
        let mad_2d = verify_2d_mad(previous.y, current.y, w, active_h, top_border, off);
        if mad_2d < best_2d_mad * 0.95 {
            best_2d_mad = mad_2d;
            best = (off, avg_1d);
        }
    }

    if debug {
        eprintln!(
            "    col_match: best off={} 1d_sad={:.2} 2d_mad={:.2}",
            best.0, best.1, best_2d_mad
        );
    }
    best
}

/// Velocity estimation from match history.
fn estimate_velocity(history: &[(usize, i32, f32)]) -> Option<f32> {
    if history.len() < 2 {
        return None;
    }
    let recent = if history.len() > VELOCITY_HISTORY {
        &history[history.len() - VELOCITY_HISTORY..]
    } else {
        history
    };
    let first = recent[0];
    let last = recent[recent.len() - 1];
    let frame_diff = (last.0 - first.0) as f32;
    if frame_diff < 1.0 {
        return None;
    }
    Some(last.1 as f32 / frame_diff)
}

/// Decide how many frames to skip and predict the offset.
fn calc_skip_and_predict(
    history: &[(usize, i32, f32)],
    h: usize,
    max_skip: usize,
    target_overlap: f32,
) -> (usize, i32) {
    if history.is_empty() {
        return (1, 0);
    }

    let vel = estimate_velocity(history);
    match vel {
        None => (1, history.last().map(|h| h.1).unwrap_or(0)),
        Some(v) if v.abs() < 0.3 => (max_skip, history.last().map(|h| h.1).unwrap_or(0)),
        Some(v) => {
            let target_px = (h as f32 * target_overlap) as i32;
            let target = target_px.max(50);
            let abs_v = v.abs();
            let frames_needed = (target as f32 / abs_v).ceil() as usize;
            let step = frames_needed.clamp(1, max_skip);
            let pred = (step as f32 * v) as i32;
            (step, pred)
        }
    }
}

// ── 2D Canvas ────────────────────────────────────────────────────────

struct Canvas2D {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
    origin_x: i32,
    origin_y: i32,
}

struct FramePlacement<'a> {
    frame: &'a [u8],
    frame_w: usize,
    frame_h: usize,
    top_border: usize,
    bottom_border: usize,
    gx: i32,
    gy: i32,
}

impl Canvas2D {
    fn new(w: usize, h: usize) -> Self {
        Self {
            pixels: vec![0u8; w * h * 3],
            width: w,
            height: h,
            origin_x: 0,
            origin_y: 0,
        }
    }

    fn grow_to_fit(&mut self, x: i32, y: i32, w: usize, h: usize) {
        let canvas_left = self.origin_x;
        let canvas_right = self.origin_x + self.width as i32;
        let canvas_top = self.origin_y;
        let canvas_bottom = self.origin_y + self.height as i32;

        let rect_left = x;
        let rect_right = x + w as i32;
        let rect_top = y;
        let rect_bottom = y + h as i32;

        let need_left = (canvas_left - rect_left).max(0) as usize;
        let need_top = (canvas_top - rect_top).max(0) as usize;
        let need_right = (rect_right - canvas_right).max(0) as usize;
        let need_bottom = (rect_bottom - canvas_bottom).max(0) as usize;

        let new_origin_x = self.origin_x - need_left as i32;
        let new_origin_y = self.origin_y - need_top as i32;
        let new_w = self.width + need_left + need_right;
        let new_h = self.height + need_top + need_bottom;

        if new_w == self.width && new_h == self.height {
            return;
        }

        if new_w as u64 * new_h as u64 > 200_000_000 {
            return;
        }

        let mut new_pixels = vec![0u8; new_w * new_h * 3];
        let old_x = (self.origin_x - new_origin_x) as usize;
        let old_y = (self.origin_y - new_origin_y) as usize;
        for y in 0..self.height {
            let src_start = y * self.width * 3;
            let dst_start = ((old_y + y) * new_w + old_x) * 3;
            new_pixels[dst_start..dst_start + self.width * 3]
                .copy_from_slice(&self.pixels[src_start..src_start + self.width * 3]);
        }
        self.pixels = new_pixels;
        self.width = new_w;
        self.height = new_h;
        self.origin_x = new_origin_x;
        self.origin_y = new_origin_y;
    }

    fn place_frame_cropped(&mut self, placement: FramePlacement<'_>) {
        let active_h = placement
            .frame_h
            .saturating_sub(placement.top_border + placement.bottom_border);
        if active_h == 0 {
            return;
        }

        let cx = (placement.gx - self.origin_x) as usize;
        let cy = (placement.gy - self.origin_y) as usize;
        for y in 0..active_h {
            let src_y = placement.top_border + y;
            let src = src_y * placement.frame_w * 3;
            let dst = ((cy + y) * self.width + cx) * 3;
            let len = placement.frame_w * 3;
            if dst + len <= self.pixels.len() && src + len <= placement.frame.len() {
                self.pixels[dst..dst + len].copy_from_slice(&placement.frame[src..src + len]);
            }
        }
    }

    fn overlay_header(&mut self, frame: &[u8], fw: usize, top_border: usize) {
        if top_border == 0 {
            return;
        }
        for y in 0..top_border.min(self.height) {
            let src = y * fw * 3;
            let dst = y * self.width * 3;
            let len = (fw * 3).min(self.width * 3);
            if dst + len <= self.pixels.len() && src + len <= frame.len() {
                self.pixels[dst..dst + len].copy_from_slice(&frame[src..src + len]);
            }
        }
    }

    fn overlay_footer(&mut self, frame: &[u8], fw: usize, fh: usize, bottom_border: usize) {
        if bottom_border == 0 || bottom_border > self.height || bottom_border > fh {
            return;
        }
        let start_y = self.height - bottom_border;
        let src_start_y = fh - bottom_border;
        for y in 0..bottom_border {
            let src = (src_start_y + y) * fw * 3;
            let dst = (start_y + y) * self.width * 3;
            let len = (fw * 3).min(self.width * 3);
            if dst + len <= self.pixels.len() && src + len <= frame.len() {
                self.pixels[dst..dst + len].copy_from_slice(&frame[src..src + len]);
            }
        }
    }

    fn to_image(&self) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
            self.width as u32,
            self.height as u32,
            self.pixels.clone(),
        )
        .unwrap()
    }
}

// ── Main pipeline ────────────────────────────────────────────────────

pub fn stitch_video(
    video_path: &Path,
    output_path: &Path,
    w_logical: i32,
    h_logical: i32,
    scale: f64,
    debug: bool,
    config: &crate::config::Config,
) -> Result<()> {
    let sad_threshold = config.longshot.sad_threshold;
    let max_skip = config.longshot.max_skip;
    let target_overlap = config.longshot.target_overlap;
    let analysis_fps = config.longshot.fps.max(1).to_string();

    let (w_phys, h_phys) = get_video_dimensions(video_path).unwrap_or_else(|e| {
        if debug {
            eprintln!("ffprobe fallback: {}", e);
        }
        (
            (w_logical as f64 * scale).round() as usize,
            (h_logical as f64 * scale).round() as usize,
        )
    });

    if debug {
        eprintln!(
            "Stitcher: {}x{} COL_GROUPS={} sad_threshold={:.1} max_skip={} overlap={:.2}",
            w_phys, h_phys, COL_GROUPS, sad_threshold, max_skip, target_overlap
        );
    }

    // Use YUV444p — Y plane is grayscale, skip RGB→gray conversion.
    let mut ffmpeg = Command::new("ffmpeg")
        .arg("-i")
        .arg(video_path)
        .arg("-vf")
        .arg(format!("fps={analysis_fps}"))
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("yuv444p")
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("ffmpeg spawn failed")?;

    let mut stdout = ffmpeg.stdout.take().context("no ffmpeg stdout")?;

    let frame_total_bytes = w_phys * h_phys * 3;
    let y_plane_bytes = w_phys * h_phys;

    struct FrameData {
        cols: Vec<f32>,
        yuv: Vec<u8>,
    }

    let mut frames: Vec<FrameData> = Vec::new();
    let mut buf = vec![0u8; frame_total_bytes];

    loop {
        match stdout.read_exact(&mut buf) {
            Ok(()) => {}
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(err).context("reading ffmpeg"),
        }

        frames.push(FrameData {
            cols: Vec::new(),
            yuv: buf.clone(),
        });
    }
    let _ = ffmpeg.wait();
    if frames.is_empty() {
        anyhow::bail!("No frames read");
    }
    if debug {
        eprintln!("Read {} frames", frames.len());
    }

    // ── Auto-Detect Static Borders ──
    let sample_indices: Vec<usize> = if frames.len() <= 5 {
        (0..frames.len()).collect()
    } else {
        let step = frames.len() / 5;
        (0..5).map(|k| (k * step).min(frames.len() - 1)).collect()
    };
    let sampled_y_planes: Vec<&[u8]> = sample_indices
        .iter()
        .map(|&idx| &frames[idx].yuv[..y_plane_bytes])
        .collect();
    let (top_border, bottom_border) = detect_static_borders(&sampled_y_planes, w_phys, h_phys);
    if debug {
        eprintln!(
            "Auto-Masking: detected top_border={}px, bottom_border={}px",
            top_border, bottom_border
        );
    }

    let active_h = h_phys.saturating_sub(top_border + bottom_border);
    if active_h == 0 {
        anyhow::bail!("Active height is 0 after border detection");
    }

    for frame in &mut frames {
        frame.cols = column_sample(
            &frame.yuv[..y_plane_bytes],
            w_phys,
            h_phys,
            top_border,
            bottom_border,
        );
    }

    // ── Match & stitch ──
    let min_overlap = (active_h as f32 * MIN_OVERLAP_FRAC).max(20.0) as usize;

    let mut canvas = Canvas2D::new(w_phys, active_h);
    let rgb_0 = yuv_to_rgb(&frames[0].yuv, w_phys, h_phys);
    canvas.place_frame_cropped(FramePlacement {
        frame: &rgb_0,
        frame_w: w_phys,
        frame_h: h_phys,
        top_border,
        bottom_border,
        gx: 0,
        gy: 0,
    });

    let mut history: Vec<(usize, i32, f32)> = Vec::new();

    let mut last_placed: usize = 0;
    let mut last_gx: i32 = 0;
    let mut last_gy: i32 = 0;

    let mut i = 1usize;
    while i < frames.len() {
        let (offset, sad) = match_columns(MatchInput {
            previous: FrameSample {
                cols: &frames[last_placed].cols,
                y: &frames[last_placed].yuv[..y_plane_bytes],
            },
            current: FrameSample {
                cols: &frames[i].cols,
                y: &frames[i].yuv[..y_plane_bytes],
            },
            w: w_phys,
            active_h,
            top_border,
            predict: history.last().map(|h| h.1).unwrap_or(0),
            min_overlap,
            sad_threshold,
            debug,
        });

        let is_good = sad < sad_threshold * RELIABLE_MULTIPLIER;
        let is_static = offset.abs() < 3 && sad < sad_threshold * 0.3;

        if debug {
            eprintln!(
                "  match {}->{}: offset={}, sad={:.2}, good={}, static={}",
                last_placed, i, offset, sad, is_good, is_static
            );
        }

        if is_good && !is_static {
            let dy = offset;
            let new_gx = last_gx;
            let new_gy = last_gy + dy;

            canvas.grow_to_fit(new_gx, new_gy, w_phys, active_h);
            let rgb_i = yuv_to_rgb(&frames[i].yuv, w_phys, h_phys);
            canvas.place_frame_cropped(FramePlacement {
                frame: &rgb_i,
                frame_w: w_phys,
                frame_h: h_phys,
                top_border,
                bottom_border,
                gx: new_gx,
                gy: new_gy,
            });

            history.push((i, offset, sad));
            last_placed = i;
            last_gx = new_gx;
            last_gy = new_gy;

            let (skip, _pred) = calc_skip_and_predict(&history, active_h, max_skip, target_overlap);
            if debug {
                eprintln!("    placed, skip={}, next={}", skip, i + skip);
            }
            i += skip;
        } else if is_static {
            if debug {
                eprintln!("    static frame, skipping");
            }
            i += 1;
        } else {
            if debug {
                eprintln!("    bad match (sad={:.2}), trying next frame", sad);
            }
            i += 1;
        }
    }

    if top_border > 0 {
        canvas.grow_to_fit(0, -(top_border as i32), w_phys, top_border);
        canvas.overlay_header(&rgb_0, w_phys, top_border);
    }
    if bottom_border > 0 {
        let last_rgb = yuv_to_rgb(&frames[last_placed].yuv, w_phys, h_phys);
        canvas.grow_to_fit(0, canvas.height as i32, w_phys, bottom_border);
        canvas.overlay_footer(&last_rgb, w_phys, h_phys, bottom_border);
    }

    let img = canvas.to_image();
    if debug {
        eprintln!(
            "Canvas: {}x{}, placed {}/{} frames",
            img.width(),
            img.height(),
            history.len() + 1,
            frames.len()
        );
    }
    img.save(output_path).context("Failed to save")?;

    Ok(())
}

/// YUV444p (BT.601 limited range) → RGB conversion.
fn yuv_to_rgb(yuv: &[u8], w: usize, h: usize) -> Vec<u8> {
    let plane_size = w * h;
    let y_plane = &yuv[..plane_size];
    let u_plane = &yuv[plane_size..2 * plane_size];
    let v_plane = &yuv[2 * plane_size..];

    let mut rgb = vec![0u8; plane_size * 3];
    rgb.par_chunks_exact_mut(3)
        .enumerate()
        .for_each(|(i, pixel)| {
            let y = y_plane[i] as f32;
            let u = u_plane[i] as f32 - 128.0;
            let v = v_plane[i] as f32 - 128.0;

            let r = (y + 1.402 * v).round().clamp(0.0, 255.0) as u8;
            let g = (y - 0.344136 * u - 0.714136 * v).round().clamp(0.0, 255.0) as u8;
            let b = (y + 1.772 * u).round().clamp(0.0, 255.0) as u8;

            pixel[0] = r;
            pixel[1] = g;
            pixel[2] = b;
        });
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_col_sample() {
        let w = 16;
        let h = 4;
        let gray: Vec<u8> = (0..(w * h) as u8).collect();
        let cols = column_sample(&gray, w, h, 0, 0);
        assert_eq!(cols.len(), h * COL_GROUPS);
        for row in 0..h {
            let row_cols = &cols[row * COL_GROUPS..(row + 1) * COL_GROUPS];
            assert_eq!(row_cols.len(), COL_GROUPS);
            for &v in row_cols {
                assert!((0.0..=255.0).contains(&v), "value {} out of range", v);
            }
        }
    }

    #[test]
    fn test_predict_offset_sequence() {
        let seq = predict_offset_sequence(10, 5);
        assert_eq!(seq[0], 0);
        assert!(seq.contains(&5));
        for i in -10i32..=10 {
            assert!(seq.contains(&i), "missing offset {}", i);
        }
    }

    #[test]
    fn test_match_columns_identical() {
        let h = 100;
        let w = 16;
        let y: Vec<u8> = (0..w * h).map(|i| (i % 256) as u8).collect();
        let cols = column_sample(&y, w, h, 0, 0);
        let (off, sad) = match_columns(MatchInput {
            previous: FrameSample { cols: &cols, y: &y },
            current: FrameSample { cols: &cols, y: &y },
            w,
            active_h: h,
            top_border: 0,
            predict: 0,
            min_overlap: 10,
            sad_threshold: 8.0,
            debug: false,
        });
        assert_eq!(off, 0);
        assert!(
            sad < 1.0,
            "sad={} should be near 0 for identical signals",
            sad
        );
    }

    #[test]
    fn test_detect_static_borders() {
        let w = 100;
        let h = 100;
        let mut f1 = vec![0u8; w * h];
        let mut f2 = vec![0u8; w * h];

        for r in 0..20 {
            for c in 0..w {
                f1[r * w + c] = 200;
                f2[r * w + c] = 200;
            }
        }
        for r in 20..100 {
            for c in 0..w {
                f1[r * w + c] = (r % 256) as u8;
                f2[r * w + c] = ((r + 10) % 256) as u8;
            }
        }
        let planes = vec![f1.as_slice(), f2.as_slice()];
        let (top, bottom) = detect_static_borders(&planes, w, h);
        assert_eq!(top, 20);
        assert_eq!(bottom, 0);
    }

    #[test]
    fn test_yuv_to_rgb_black() {
        let w = 2;
        let h = 2;
        let plane_size = w * h;
        let mut yuv = vec![0u8; plane_size * 3];
        yuv[..plane_size].fill(16);
        yuv[plane_size..2 * plane_size].fill(128);
        yuv[2 * plane_size..].fill(128);

        let rgb = yuv_to_rgb(&yuv, w, h);
        for i in 0..4 {
            assert!(rgb[i * 3] < 20, "R should be near 0, got {}", rgb[i * 3]);
            assert!(rgb[i * 3 + 1] < 20, "G should be near 0");
            assert!(rgb[i * 3 + 2] < 20, "B should be near 0");
        }
    }

    #[test]
    fn test_weread_video() {
        let config = crate::config::Config::default();
        let video = Path::new("/home/nana/Downloads/weread.MP4");
        if !video.exists() {
            eprintln!("Skipping: video file not found");
            return;
        }
        let res = stitch_video(
            video,
            Path::new("/tmp/weread_sad_output.png"),
            1334,
            1920,
            1.0,
            true,
            &config,
        );
        if let Err(e) = res {
            eprintln!("Stitch failed: {}", e);
        }
    }
}
