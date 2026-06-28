use anyhow::{Context, Result};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use serde::{Deserialize, Serialize};
use notify_rust::Notification;
use chrono::Local;

use crate::cli::Args;
use crate::config;
use crate::selector;

pub mod overlay;
pub mod stitcher;

const STATE_FILE: &str = "/tmp/shot-longshot.json";

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

fn is_process_running(pid: u32) -> bool {
    Path::new(&format!("/proc/{}", pid)).exists()
}

pub fn handle_longshot(args: &Args, config: &config::Config) -> Result<()> {
    let debug = args.debug;
    let silent = args.silent;
    let notif_timeout = args.notif_timeout.unwrap_or(config.capture.notification_timeout);

    // Check if a longshot recording is already active
    if Path::new(STATE_FILE).exists() {
        // Read state
        let state_data = fs::read_to_string(STATE_FILE)
            .context("Failed to read longshot state file")?;
        let state: LongshotState = serde_json::from_str(&state_data)
            .context("Failed to parse longshot state JSON")?;

        if debug {
            eprintln!("Stopping longshot recording: {:?}", state);
        }

        // Stop wf-recorder (SIGINT / -2 to save the video)
        let _ = Command::new("kill")
            .arg("-2")
            .arg(state.pid.to_string())
            .status();

        // Stop overlay process (SIGTERM)
        let _ = Command::new("kill")
            .arg(state.overlay_pid.to_string())
            .status();

        // Wait for processes to exit
        let mut wait_count = 0;
        while is_process_running(state.pid) && wait_count < 25 {
            std::thread::sleep(Duration::from_millis(200));
            wait_count += 1;
        }

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
            state.w,
            state.h,
            state.scale,
            debug,
        );

        // Delete state file
        let _ = fs::remove_file(STATE_FILE);

        match stitch_res {
            Ok(()) => {
                // Copy to clipboard
                if let Ok(png_bytes) = fs::read(&state.output_path) {
                    let mut wl_copy_cmd = Command::new("wl-copy")
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
                
                if debug {
                    eprintln!("Longshot completed successfully. Stitched file: {}", state.output_path);
                }
            }
            Err(err) => {
                let _ = fs::remove_file(&state.video_path);
                if !silent {
                    let _ = Notification::new()
                        .summary("Longshot error")
                        .body(&format!("拼接失败: {}", err))
                        .timeout(notif_timeout as i32)
                        .appname("Shot")
                        .show();
                }
                return Err(err);
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

        // Query active monitor name & scale factor
        let (monitor, scale_f) = super::external::get_active_monitor_info(debug)
            .unwrap_or(("eDP-1".to_string(), 1.0));
        let scale = scale_f;

        let video_path = format!("/tmp/shot_longshot_{}.mp4", std::process::id());
        let filename = format!("longshot_{}.png", Local::now().format("%Y-%m-%d-%H%M%S"));
        let output_path = save_dir.join(filename);

        if debug {
            eprintln!(
                "Starting longshot recording. Video: {}, Output: {}, Region: {:?}, Scale: {}",
                video_path, output_path.display(), geometry, scale
            );
        }

        // Spawn wf-recorder
        let rec_child = Command::new("wf-recorder")
            .arg("-g")
            .arg(format!("{},{} {}x{}", geometry.x, geometry.y, geometry.width, geometry.height))
            .arg("-f")
            .arg(&video_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn wf-recorder. Please ensure it is installed.")?;
        let rec_pid = rec_child.id();

        // Spawn overlay
        let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
        let overlay_child = Command::new(exe_path)
            .arg("overlay")
            .arg("--x").arg(geometry.x.to_string())
            .arg("--y").arg(geometry.y.to_string())
            .arg("--w").arg(geometry.width.to_string())
            .arg("--h").arg(geometry.height.to_string())
            .arg("--scale").arg(scale.to_string())
            .arg("--monitor").arg(&monitor)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn overlay process")?;
        let overlay_pid = overlay_child.id();

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
        let state_json = serde_json::to_string_pretty(&state)
            .context("Failed to serialize state to JSON")?;
        fs::write(STATE_FILE, state_json)
            .context("Failed to write longshot state file")?;

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
