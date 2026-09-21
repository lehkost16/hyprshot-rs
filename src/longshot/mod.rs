use anyhow::{Context, Result};
use chrono::Local;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use crate::cli::Args;
use crate::config;
use crate::selector;
use crate::utils;

mod canvas;
mod decoder;
pub mod overlay;
pub mod stitcher;

fn state_file_path() -> std::path::PathBuf {
    utils::runtime_state_path("longshot.json")
}

#[derive(Serialize, Deserialize, Debug)]
struct LongshotState {
    pid: u32,
    overlay_pid: u32,
    video_path: String,
    output_path: String,
    w: i32,
    h: i32,
    scale: f64,
}

pub struct StitchRequest {
    pub input: std::path::PathBuf,
    pub output: Option<std::path::PathBuf>,
    pub debug: bool,
    pub silent: bool,
    pub notif_timeout: u32,
}

pub fn handle_longshot(args: &Args, config: &config::Config) -> Result<()> {
    let debug = args.debug;
    let silent = args.silent || !config.capture.notification;
    let notif_timeout = args
        .notif_timeout
        .unwrap_or(config.capture.notification_timeout);

    let state_file = state_file_path();

    // Check if a longshot recording is already active
    if state_file.exists() {
        // Read state
        let state_data =
            fs::read_to_string(&state_file).context("Failed to read longshot state file")?;
        let state: LongshotState =
            serde_json::from_str(&state_data).context("Failed to parse longshot state JSON")?;

        if debug {
            eprintln!("Stopping longshot recording: {:?}", state);
        }

        crate::capture_session::stop(state.pid, state.overlay_pid)?;

        // Send stitching notification
        if !silent {
            let _ = Notification::new()
                .summary("Stitching...")
                .body("正在进行长截图拼接，请稍候...")
                .timeout(3000)
                .appname("Shot")
                .show();
        }

        // Stitch video frames
        let stitch_res = stitcher::stitch_video(
            Path::new(&state.video_path),
            Path::new(&state.output_path),
            debug,
            config,
        );

        match stitch_res {
            Ok(()) => {
                // Copy to clipboard
                if let Ok(png_bytes) = fs::read(&state.output_path) {
                    let wl_copy_cmd = Command::new("wl-copy")
                        .arg("--type")
                        .arg("image/png")
                        .stdin(Stdio::piped())
                        .spawn();
                    if let Ok(mut child) = wl_copy_cmd {
                        if let Some(mut stdin) = child.stdin.take() {
                            let _ = stdin.write_all(&png_bytes);
                        }
                        let _ = child.wait();
                    }
                }

                // Send success notification
                if !silent {
                    let _ = Notification::new()
                        .summary("✅ Longshot 已生成")
                        .body(&format!("图片已保存至: {}", state.output_path))
                        .icon(&state.output_path)
                        .timeout(notif_timeout as i32)
                        .appname("Shot")
                        .show();
                }

                // Clean up temp video file
                let _ = fs::remove_file(&state.video_path);
                let _ = fs::remove_file(&state_file);

                if debug {
                    eprintln!(
                        "Longshot completed successfully. Stitched file: {}",
                        state.output_path
                    );
                }
            }
            Err(err) => {
                if !silent {
                    let _ = Notification::new()
                        .summary("Longshot error")
                        .body(&format!(
                            "拼接失败: {}\n源视频保留于: {}",
                            err, state.video_path
                        ))
                        .timeout(notif_timeout as i32)
                        .appname("Shot")
                        .show();
                }
                return Err(err)
                    .context(format!("Source recording retained at {}", state.video_path));
            }
        }

        Ok(())
    } else {
        // Start longshot recording
        let save_dir = config::get_screenshots_dir(args.output_folder.clone(), config, debug)?;
        let save_dir = if !args.clipboard_only && !args.raw {
            config::ensure_directory(&save_dir.to_string_lossy())?
        } else {
            save_dir
        };

        // Select region
        let geometry = selector::select_region(debug)?;

        // Query the monitor containing the selected region, so the overlay border
        // is drawn in the same output coordinate space as the recording.
        let monitor_info = crate::compositor::get_monitor_info_for_geometry(&geometry, debug)?;
        let scale = monitor_info.scale;

        let video_path = std::env::temp_dir()
            .join(format!("shot_longshot_{}.mp4", std::process::id()))
            .to_string_lossy()
            .to_string();
        let filename = format!("longshot_{}.png", Local::now().format("%Y-%m-%d-%H%M%S"));
        let output_path = save_dir.join(filename);

        if debug {
            eprintln!(
                "Starting longshot recording. Video: {}, Output: {}, Region: {:?}, Scale: {}",
                video_path,
                output_path.display(),
                geometry,
                scale
            );
        }

        // Spawn wl-screenrec with --no-cursor
        let geom_str = format!(
            "{},{} {}x{}",
            geometry.x, geometry.y, geometry.width, geometry.height
        );
        let max_fps_arg = config.longshot.fps.to_string();

        let mut rec_cmd = Command::new("wl-screenrec");
        rec_cmd
            .arg("-g")
            .arg(&geom_str)
            .arg("-f")
            .arg(&video_path)
            .arg("--no-cursor")
            .arg("--max-fps")
            .arg(&max_fps_arg);

        if config.record.hwaccel == "none" {
            rec_cmd.arg("--no-hw");
        }

        let capture = crate::capture_session::StartedCapture::start(
            rec_cmd,
            &geometry,
            &monitor_info,
            debug,
        )?;
        let rec_pid = capture.recorder_pid();
        let overlay_pid = capture.overlay_pid();

        // Write state file
        let state = LongshotState {
            pid: rec_pid,
            overlay_pid,
            video_path,
            output_path: output_path.to_string_lossy().to_string(),
            w: geometry.width,
            h: geometry.height,
            scale,
        };
        capture.commit(&state_file, &state)?;

        // Send starting notification
        if !silent {
            let _ = Notification::new()
                .summary("🔴 Longshot 录制中")
                .body("滚动页面完成后，再次运行命令停止录制并拼接")
                .timeout(notif_timeout as i32)
                .appname("Shot")
                .show();
        }

        Ok(())
    }
}

/// Directly stitch an existing video file into a long screenshot.
pub fn handle_stitch(request: StitchRequest, config: &config::Config) -> Result<()> {
    let StitchRequest {
        input,
        output,
        debug,
        silent,
        notif_timeout,
    } = request;

    if !input.exists() {
        anyhow::bail!("Video file not found: {}", input.display());
    }

    let output_path = match output {
        Some(p) => p,
        None => {
            let mut p = input.clone();
            p.set_extension("png");
            p
        }
    };

    if !silent {
        let _ = Notification::new()
            .summary("Stitching video...")
            .body(&format!("{} → {}", input.display(), output_path.display()))
            .timeout(3000)
            .appname("Shot")
            .show();
    }

    stitcher::stitch_video(&input, &output_path, debug, config)?;

    if let Ok(png_bytes) = std::fs::read(&output_path) {
        let wl_copy_cmd = std::process::Command::new("wl-copy")
            .arg("--type")
            .arg("image/png")
            .stdin(std::process::Stdio::piped())
            .spawn();
        if let Ok(mut child) = wl_copy_cmd {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(&png_bytes);
            }
            let _ = child.wait();
        }
    }

    if !silent {
        let _ = Notification::new()
            .summary("✅ 拼接完成")
            .body(&format!("长图已保存至: {}", output_path.display()))
            .icon(output_path.to_string_lossy().as_ref())
            .timeout(notif_timeout as i32)
            .appname("Shot")
            .show();
    }

    Ok(())
}
