use anyhow::{Context, Result};
use std::{
    fs,
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

        // Stop overlay process
        if state.overlay_pid != 0 {
            let _ = Command::new("kill")
                .arg(state.overlay_pid.to_string())
                .status();
        }
        let _ = Command::new("pkill")
            .arg("-f")
            .arg("shot-overlay")
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
            config,
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
        let (_monitor, scale_f, _ox, _oy) = super::external::get_active_monitor_info(debug)
            .unwrap_or(("eDP-1".to_string(), 1.0, 0, 0));
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
        let fps_arg = format!("fps={}", config.longshot.fps);
        let rec_child = Command::new("wf-recorder")
            .arg("-g")
            .arg(format!("{},{} {}x{}", geometry.x, geometry.y, geometry.width, geometry.height))
            .arg("-f")
            .arg(&video_path)
            .arg("-F")
            .arg(&fps_arg)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn wf-recorder. Please ensure it is installed.")?;
        let rec_pid = rec_child.id();

        // Spawn C-based overlay using hyprctl dispatch exec
        let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
        let overlay_path = exe_path.parent().unwrap().join("shot-overlay");
        let overlay_path = if overlay_path.exists() {
            overlay_path
        } else {
            PathBuf::from("shot-overlay")
        };

        let padding = 10;
        let ox_sel = if geometry.x > padding { geometry.x - padding } else { 0 };
        let oy_sel = if geometry.y > padding { geometry.y - padding } else { 0 };
        let ow_sel = geometry.width + padding * 2;
        let oh_sel = geometry.height + padding * 2;

        let mut spawned = false;
        if Command::new("hyprctl").arg("version").output().is_ok() {
            let is_lua = is_hyprland_lua();
            let overlay_cmd = if is_lua {
                format!(
                    "hl.dsp.exec_cmd(\"{} {} {}\", {{ float = true, move = {{ {}, {} }}, size = {{ {}, {} }} }})",
                    overlay_path.to_string_lossy(),
                    ow_sel, oh_sel,
                    ox_sel, oy_sel,
                    ow_sel, oh_sel
                )
            } else {
                format!(
                    "exec [move {} {}; size {} {}] env XDG_SESSION_TYPE=wayland WAYLAND_DISPLAY={} {} {} {}",
                    ox_sel, oy_sel, ow_sel, oh_sel,
                    std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_string()),
                    overlay_path.to_string_lossy(),
                    ow_sel, oh_sel
                )
            };

            if debug {
                eprintln!("Spawning C overlay (is_lua={}): {}", is_lua, overlay_cmd);
            }

            let status = Command::new("hyprctl")
                .arg("dispatch")
                .arg(&overlay_cmd)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if let Ok(s) = status {
                if s.success() {
                    spawned = true;
                }
            }
        }

        if !spawned {
            if debug {
                eprintln!("hyprctl dispatch failed, falling back to direct overlay spawn");
            }
            let _ = Command::new(&overlay_path)
                .arg(ow_sel.to_string())
                .arg(oh_sel.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }

        let overlay_pid = 0; // We will clean it up via pkill shot-overlay

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

fn is_hyprland_lua() -> bool {
    if let Ok(output) = Command::new("hyprctl").arg("version").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(idx) = stdout.find("Hyprland ") {
            let version_part = &stdout[idx + 9..];
            let mut parts = version_part.split('.');
            if let Some(major) = parts.next() {
                if let Some(minor_str) = parts.next() {
                    if let Ok(minor) = minor_str.parse::<u32>() {
                        if major == "0" && minor >= 55 {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}
