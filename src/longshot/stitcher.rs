use anyhow::{Context, Result};
use rayon::prelude::*;
use std::{path::Path, process::Command};

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
    let dimensions = (parts[0].parse::<usize>()?, parts[1].parse::<usize>()?);
    anyhow::ensure!(
        dimensions.0 > 0 && dimensions.1 > 0,
        "Video has empty dimensions"
    );
    Ok(dimensions)
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
    let max = active_h.saturating_sub(min_overlap);
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

// The offline source adapter uses two passes: bounded border sampling, then
// incremental matching. Neither the matching engine nor its canvas owns ffmpeg.

use super::canvas::{Canvas, MAX_CANVAS_BYTES};
use super::decoder::FrameReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StitchUpdate {
    FirstFrame,
    Appended,
    NoProgress,
    NoMatch,
}

struct ReferenceFrame {
    gray: Vec<u8>,
    cols: Vec<f32>,
}

pub struct Stitcher {
    width: usize,
    height: usize,
    top: usize,
    bottom: usize,
    canvas: Canvas,
    previous: Option<ReferenceFrame>,
    header: Vec<u8>,
    footer: Vec<u8>,
    position: i64,
    history: Vec<(usize, i32, f32)>,
    frame_index: usize,
    next_match: usize,
    placed: usize,
    rejected: usize,
    sad_threshold: f32,
    max_skip: usize,
    target_overlap: f32,
    debug: bool,
}

impl Stitcher {
    pub fn new(
        width: usize,
        height: usize,
        borders: (usize, usize),
        config: &crate::config::Config,
        debug: bool,
    ) -> Result<Self> {
        let (top, bottom) = borders;
        anyhow::ensure!(
            config.longshot.sad_threshold.is_finite() && config.longshot.sad_threshold > 0.0,
            "longshot.sad_threshold must be finite and positive"
        );
        anyhow::ensure!(
            config.longshot.max_skip > 0
                && config.longshot.target_overlap.is_finite()
                && config.longshot.target_overlap > 0.0
                && config.longshot.target_overlap <= 1.0,
            "Invalid longshot skip/overlap settings"
        );
        anyhow::ensure!(
            top.checked_add(bottom).is_some_and(|sum| sum < height),
            "No active content after border detection"
        );
        Ok(Self {
            width,
            height,
            top,
            bottom,
            canvas: Canvas::new(width, MAX_CANVAS_BYTES)?,
            previous: None,
            header: Vec::new(),
            footer: Vec::new(),
            position: 0,
            history: Vec::new(),
            frame_index: 0,
            next_match: 0,
            placed: 0,
            rejected: 0,
            sad_threshold: config.longshot.sad_threshold,
            max_skip: config.longshot.max_skip.max(1),
            target_overlap: config.longshot.target_overlap,
            debug,
        })
    }

    pub fn push_frame(&mut self, rgb: Vec<u8>) -> Result<StitchUpdate> {
        anyhow::ensure!(
            rgb.len() == self.width * self.height * 3,
            "Decoded frame size changed"
        );
        let index = self.frame_index;
        self.frame_index += 1;
        if index < self.next_match {
            return Ok(StitchUpdate::NoProgress);
        }
        let gray = grayscale(&rgb);
        let active_h = self.height - self.top - self.bottom;
        let cols = column_sample(&gray, self.width, self.height, self.top, self.bottom);
        let first = self.previous.is_none();
        let mut offset = 0;
        if let Some(previous) = &self.previous {
            let (dy, sad) = match_columns(MatchInput {
                previous: FrameSample {
                    cols: &previous.cols,
                    y: &previous.gray,
                },
                current: FrameSample {
                    cols: &cols,
                    y: &gray,
                },
                w: self.width,
                active_h,
                top_border: self.top,
                predict: self.history.last().map(|entry| entry.1).unwrap_or(0),
                min_overlap: ((active_h as f32 * MIN_OVERLAP_FRAC).max(20.0) as usize)
                    .min(active_h.saturating_sub(1)),
                sad_threshold: self.sad_threshold,
                debug: self.debug,
            });
            if dy.abs() < 3 && sad < self.sad_threshold * 0.3 {
                return Ok(StitchUpdate::NoProgress);
            }
            if !sad.is_finite() || sad >= self.sad_threshold * RELIABLE_MULTIPLIER {
                self.rejected += 1;
                return Ok(StitchUpdate::NoMatch);
            }
            offset = dy;
        }
        let position = self
            .position
            .checked_add(i64::from(offset))
            .context("Stitch position overflow")?;
        let old_origin = self.canvas.origin();
        let old_end = self.canvas.end();
        let row_bytes = self.width * 3;
        let body = &rgb[self.top * row_bytes..(self.height - self.bottom) * row_bytes];
        let grew = self.canvas.place(body, position)?;
        if first || position < old_origin {
            self.header = rgb[..self.top * row_bytes].to_vec();
        }
        if first || position + active_h as i64 > old_end {
            self.footer = rgb[(self.height - self.bottom) * row_bytes..].to_vec();
        }
        self.previous = Some(ReferenceFrame { gray, cols });
        self.position = position;
        self.placed += 1;
        if !first {
            self.history.push((index, offset, 0.0));
            if self.history.len() > VELOCITY_HISTORY {
                self.history.remove(0);
            }
            let (skip, _) =
                calc_skip_and_predict(&self.history, active_h, self.max_skip, self.target_overlap);
            self.next_match = index.saturating_add(skip);
        }
        Ok(if first {
            StitchUpdate::FirstFrame
        } else if grew {
            StitchUpdate::Appended
        } else {
            StitchUpdate::NoProgress
        })
    }

    pub fn finish(mut self) -> Result<image::RgbImage> {
        anyhow::ensure!(self.previous.is_some(), "No frames decoded");
        anyhow::ensure!(
            self.placed > 1 || self.rejected == 0,
            "No trustworthy overlap found; refusing to save an incomplete long screenshot"
        );
        if self.top > 0 {
            self.canvas
                .place(&self.header, self.canvas.origin() - self.top as i64)?;
        }
        if self.bottom > 0 {
            self.canvas.place(&self.footer, self.canvas.end())?;
        }
        if self.debug {
            eprintln!(
                "Longshot: {} source frames, {} accepted, {} rejected; {}x{} pixels",
                self.frame_index,
                self.placed,
                self.rejected,
                self.width,
                self.canvas.height()
            );
        }
        self.canvas.into_image()
    }
}

fn grayscale(rgb: &[u8]) -> Vec<u8> {
    rgb.par_chunks_exact(3)
        .map(|p| ((77 * u32::from(p[0]) + 150 * u32::from(p[1]) + 29 * u32::from(p[2])) >> 8) as u8)
        .collect()
}

/// Keep at most five luminance frames spread through the source, never RGB history.
struct BorderSamples {
    frames: Vec<Vec<u8>>,
    seen: u64,
    random: u64,
}

impl BorderSamples {
    fn new() -> Self {
        Self {
            frames: Vec::new(),
            seen: 0,
            random: 0x485953484f54,
        }
    }

    fn push(&mut self, gray: Vec<u8>) {
        self.seen += 1;
        if self.frames.len() < 5 {
            self.frames.push(gray);
        } else {
            // Deterministic reservoir sampling keeps the first frame and four samples.
            self.random = self
                .random
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1);
            let index = self.random % (self.seen - 1);
            if index < 4 {
                self.frames[index as usize + 1] = gray;
            }
        }
    }
}

pub fn stitch_to_image(
    video: &Path,
    debug: bool,
    config: &crate::config::Config,
) -> Result<image::RgbImage> {
    let (width, height) = get_video_dimensions(video)?;
    let mut reader = FrameReader::open(video, width, height, config.longshot.fps)?;
    let mut samples = BorderSamples::new();
    while let Some(rgb) = reader.next_frame()? {
        samples.push(grayscale(&rgb));
    }
    let planes: Vec<&[u8]> = samples.frames.iter().map(Vec::as_slice).collect();
    let borders = detect_static_borders(&planes, width, height);
    drop(samples);
    drop(reader);

    let mut stitcher = Stitcher::new(width, height, borders, config, debug)?;
    let mut reader = FrameReader::open(video, width, height, config.longshot.fps)?;
    while let Some(rgb) = reader.next_frame()? {
        let outcome = stitcher.push_frame(rgb)?;
        if debug && outcome == StitchUpdate::NoMatch {
            eprintln!("Longshot: frame has no trusted match");
        }
    }
    stitcher.finish()
}

pub fn stitch_video(
    video: &Path,
    output: &Path,
    debug: bool,
    config: &crate::config::Config,
) -> Result<image::RgbImage> {
    let image = stitch_to_image(video, debug, config)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    image.write_to(&mut temp, image::ImageFormat::Png)?;
    temp.persist_noclobber(output)
        .context("Cannot publish long screenshot; output must not already exist")?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

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
    fn static_frame_history_stays_bounded() {
        let config = crate::config::Config::default();
        let mut stitcher = Stitcher::new(16, 60, (0, 0), &config, false).unwrap();
        let frame = vec![100; 16 * 60 * 3];
        for _ in 0..300 {
            stitcher.push_frame(frame.clone()).unwrap();
        }
        assert_eq!(stitcher.history.len(), 0);
        assert_eq!(stitcher.finish().unwrap().dimensions(), (16, 60));
    }

    #[test]
    fn reservoir_never_retains_all_frames() {
        let mut samples = BorderSamples::new();
        for value in 0..10_000 {
            samples.push(vec![(value % 255) as u8; 8]);
        }
        assert_eq!(samples.frames.len(), 5);
        assert_eq!(samples.frames[0], vec![0; 8]);
    }

    #[test]
    fn incremental_stitching_restores_header_and_footer_once() {
        let mut config = crate::config::Config::default();
        config.longshot.max_skip = 1;
        let mut stitcher = Stitcher::new(24, 80, (5, 5), &config, false).unwrap();
        for offset in [0, 10, 20, 10, 30] {
            let frame = synthetic_frame(24, 80, offset, 5);
            stitcher.push_frame(frame).unwrap();
        }
        let result = stitcher.finish().unwrap();
        assert_eq!(result.dimensions(), (24, 110));
        assert_eq!(result.get_pixel(0, 0).0, [255, 20, 30]);
        assert_eq!(result.get_pixel(0, 109).0, [10, 20, 255]);
    }

    fn synthetic_frame(width: usize, height: usize, offset: usize, border: usize) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(width * height * 3);
        for row in 0..height {
            for col in 0..width {
                let pixel = if row < border {
                    [255, 20, 30]
                } else if row >= height - border {
                    [10, 20, 255]
                } else {
                    let value =
                        ((row + offset) * 73 + col * 31 + (row + offset) * (row + offset) * 17)
                            as u8;
                    [value, value, value]
                };
                rgb.extend_from_slice(&pixel);
            }
        }
        rgb
    }

    #[test]
    #[ignore = "requires ffmpeg and ffprobe; run explicitly for integration verification"]
    fn synthetic_video_pipeline_preserves_dimensions_and_colors() {
        use std::io::Write;
        let directory = tempfile::tempdir().unwrap();
        let video = directory.path().join("scroll.mkv");
        let mut encoder = Command::new("ffmpeg")
            .args([
                "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", "24x80", "-r", "6",
                "-i", "-", "-c:v", "ffv1",
            ])
            .arg(&video)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        {
            let mut stdin = encoder.stdin.take().unwrap();
            for offset in [0, 10, 20, 30, 40, 50] {
                stdin
                    .write_all(&synthetic_frame(24, 80, offset, 5))
                    .unwrap();
            }
        }
        assert!(encoder.wait().unwrap().success());
        let mut config = crate::config::Config::default();
        config.longshot.fps = 6;
        config.longshot.max_skip = 1;
        let output = directory.path().join("result.png");
        stitch_video(&video, &output, false, &config).unwrap();
        let image = image::open(&output).unwrap().into_rgb8();
        assert_eq!(image.dimensions(), (24, 130));
        assert_eq!(image.get_pixel(0, 0).0, [255, 20, 30]);
        assert_eq!(image.get_pixel(0, 129).0, [10, 20, 255]);
        assert!(stitch_video(&video, &output, false, &config).is_err());
    }
}
