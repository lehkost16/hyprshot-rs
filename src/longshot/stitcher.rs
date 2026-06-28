use anyhow::{Context, Result};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
    process::{Command, Stdio},
};
use image::{ImageBuffer, Rgb};

const PADDING: i32 = 10;
const BOTTOM_RATIO: f32 = 0.15;
const MID_START_RATIO: f32 = 0.40;
const MID_END_RATIO: f32 = 0.60;
const MATCH_THRESHOLD: f32 = 0.80;
const MIN_MOVEMENT: i32 = 5;
const STATIC_THRESH: f32 = 1.0;

fn to_grayscale(rgb: &[u8], gray: &mut [u8]) {
    let num_pixels = rgb.len() / 3;
    for i in 0..num_pixels {
        let offset = i * 3;
        let r = rgb[offset];
        let g = rgb[offset + 1];
        let b = rgb[offset + 2];
        // Standard formula: Y = 0.299R + 0.587G + 0.114B
        gray[i] = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) as u8;
    }
}

fn l1_diff(a: &[u8], b: &[u8]) -> f32 {
    let mut sum = 0u64;
    for (x, y) in a.iter().zip(b.iter()) {
        sum += x.abs_diff(*y) as u64;
    }
    sum as f32 / a.len() as f32
}

fn extract_stripe(gray: &[u8], w: usize, h: usize, stripe_w: usize) -> (Vec<u8>, usize) {
    let sw = stripe_w.min(w);
    let x_start = (w - sw) / 2;
    let mut stripe = vec![0u8; sw * h];
    for y in 0..h {
        let src_offset = y * w + x_start;
        let dst_offset = y * sw;
        stripe[dst_offset..dst_offset + sw].copy_from_slice(&gray[src_offset..src_offset + sw]);
    }
    (stripe, sw)
}

fn match_template_vertical(
    curr: &[u8],
    temp: &[u8],
    w: usize,
    h: usize,
    h_temp: usize,
) -> (f32, usize) {
    let n = w * h_temp;
    
    // Convert template to f32 once
    let temp_f32: Vec<f32> = temp.iter().map(|&x| x as f32).collect();
    
    // Mean of template
    let sum_t: f32 = temp_f32.iter().sum();
    let mean_t = sum_t / n as f32;
    let mut sq_sum_t: f32 = 0.0;
    for &x in &temp_f32 {
        let diff = x - mean_t;
        sq_sum_t += diff * diff;
    }
    let var_t = sq_sum_t;

    if var_t < 1e-5f32 {
        return (0.0, 0);
    }

    let mut best_score = -1.0f32;
    let mut best_y = 0;

    // Search vertically from top to bottom
    for y in 0..=(h - h_temp) {
        let offset = y * w;
        let patch = &curr[offset..offset + n];

        let sum_p: f32 = patch.iter().map(|&x| x as f32).sum();
        let mean_p = sum_p / n as f32;

        let mut sum_pt: f32 = 0.0;
        let mut sum_p2: f32 = 0.0;
        for i in 0..n {
            let p = patch[i] as f32;
            let t = temp_f32[i];
            sum_pt += p * t;
            sum_p2 += p * p;
        }

        let cov = sum_pt - n as f32 * mean_p * mean_t;
        let sq_sum_p = sum_p2 - n as f32 * mean_p * mean_p;

        if sq_sum_p > 1e-5f32 {
            let score = (cov / (sq_sum_p * var_t).sqrt()) as f32;
            if score > best_score {
                best_score = score;
                best_y = y;
            }
        }
    }

    (best_score, best_y)
}

pub fn stitch_video(
    video_path: &Path,
    output_path: &Path,
    w_logical: i32,
    h_logical: i32,
    scale: f64,
    debug: bool,
) -> Result<()> {
    // 1. Calculate dimensions using float scale factor
    let crop = 0usize;
    let w_phys = (w_logical as f64 * scale).round() as usize;
    let h_phys = (h_logical as f64 * scale).round() as usize;

    if w_phys <= crop * 2 || h_phys <= crop * 2 {
        anyhow::bail!("Video physical size is too small for cropping");
    }

    let w_cropped = w_phys - crop * 2;
    let h_cropped = h_phys - crop * 2;

    let bottom_th = (h_cropped as f32 * BOTTOM_RATIO) as usize;
    let mid_start = (h_cropped as f32 * MID_START_RATIO) as usize;
    let mid_end = (h_cropped as f32 * MID_END_RATIO) as usize;

    if debug {
        eprintln!(
            "Stitcher: physical size={}x{}, cropped={}x{}, crop={}, scale={}",
            w_phys, h_phys, w_cropped, h_cropped, crop, scale
        );
    }

    // 2. Spawn ffmpeg process to dump frames
    let mut ffmpeg = Command::new("ffmpeg")
        .arg("-i")
        .arg(video_path)
        .arg("-vf")
        .arg("fps=10")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgb24")
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn ffmpeg command")?;

    let mut stdout = ffmpeg.stdout.take().context("Failed to open ffmpeg stdout")?;

    let raw_frame_size = w_phys * h_phys * 3;
    let mut raw_buf = vec![0u8; raw_frame_size];

    let mut result_rgb: Vec<u8> = Vec::new();
    let mut prev_gray: Vec<u8> = Vec::new();
    let mut prev_stripe: Vec<u8> = Vec::new();
    let mut sw = 0usize;

    let mut frame_count = 0;
    let mut stitch_count = 0;

    loop {
        // Read next physical frame
        match stdout.read_exact(&mut raw_buf) {
            Ok(()) => {}
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(err) => return Err(err).context("Error reading ffmpeg output stream"),
        }

        // Perform crop
        let mut curr_rgb = vec![0u8; w_cropped * h_cropped * 3];
        for y in 0..h_cropped {
            let src_y = y + crop;
            let src_start = (src_y * w_phys + crop) * 3;
            let src_end = src_start + w_cropped * 3;
            let dst_start = y * w_cropped * 3;
            curr_rgb[dst_start..dst_start + w_cropped * 3].copy_from_slice(&raw_buf[src_start..src_end]);
        }

        let mut curr_gray = vec![0u8; w_cropped * h_cropped];
        to_grayscale(&curr_rgb, &mut curr_gray);

        frame_count += 1;

        if frame_count == 1 {
            // First frame: append entirely
            result_rgb.extend_from_slice(&curr_rgb);
            let (stripe, stripe_w) = extract_stripe(&curr_gray, w_cropped, h_cropped, 300);
            prev_stripe = stripe;
            sw = stripe_w;
            prev_gray = curr_gray;
            continue;
        }

        // Check if frame is static compared to previous
        let diff = l1_diff(&curr_gray, &prev_gray);
        if diff < STATIC_THRESH {
            continue;
        }

        let (curr_stripe, _) = extract_stripe(&curr_gray, w_cropped, h_cropped, 300);

        // Try Bottom-to-Bottom matching
        let temp_bottom = &prev_stripe[(h_cropped - bottom_th) * sw..h_cropped * sw];
        let (score_b, y_b) = match_template_vertical(
            &curr_stripe,
            temp_bottom,
            sw,
            h_cropped,
            bottom_th,
        );

        let bottom_movement = (h_cropped - bottom_th) - y_b;
        if score_b > MATCH_THRESHOLD && bottom_movement as i32 > MIN_MOVEMENT {
            let split_row = y_b + bottom_th;
            if split_row < h_cropped {
                let start_idx = split_row * w_cropped * 3;
                result_rgb.extend_from_slice(&curr_rgb[start_idx..]);
                stitch_count += 1;
            }
            prev_stripe = curr_stripe;
            prev_gray = curr_gray;
            continue;
        }

        // Try Middle matching
        let temp_mid = &prev_stripe[mid_start * sw..mid_end * sw];
        let mid_h = mid_end - mid_start;
        let (score_m, y_m) = match_template_vertical(
            &curr_stripe,
            temp_mid,
            sw,
            h_cropped,
            mid_h,
        );

        let mid_movement = mid_start as i32 - y_m as i32;
        if score_m > MATCH_THRESHOLD && mid_movement > MIN_MOVEMENT {
            let dy = mid_movement as usize;
            if dy < h_cropped {
                let start_idx = (h_cropped - dy) * w_cropped * 3;
                result_rgb.extend_from_slice(&curr_rgb[start_idx..]);
                stitch_count += 1;
            }
            prev_stripe = curr_stripe;
            prev_gray = curr_gray;
        }
    }

    let _ = ffmpeg.wait();

    if result_rgb.is_empty() {
        anyhow::bail!("No frames were processed");
    }

    let total_h = result_rgb.len() / (w_cropped * 3);
    if debug {
        eprintln!(
            "Stitcher finished: total stitched height={}px ({} segments), total read frames={}",
            total_h, stitch_count, frame_count
        );
    }

    // 3. Save resulting image to file
    let img_buffer: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(w_cropped as u32, total_h as u32, result_rgb)
            .context("Failed to construct final image buffer")?;

    img_buffer
        .save(output_path)
        .context(format!("Failed to save stitched image to {}", output_path.display()))?;

    Ok(())
}
