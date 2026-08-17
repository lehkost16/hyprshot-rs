use anyhow::{Context, Result};
use chrono::Local;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use crate::cli::Args;
use crate::config;
use crate::selector;

fn state_file_path() -> std::path::PathBuf {
    std::env::temp_dir().join("shot-record.json")
}

#[derive(Serialize, Deserialize, Debug)]
struct RecordState {
    pid: u32,
    overlay_pid: u32,
    video_path: String,
    #[serde(default)]
    temp_video_path: Option<String>,
}

fn is_process_running(pid: u32) -> bool {
    Path::new(&format!("/proc/{}", pid)).exists()
}

pub fn handle_record(args: &Args, config: &config::Config) -> Result<()> {
    let debug = args.debug;
    let silent = args.silent;
    let notif_timeout = args
        .notif_timeout
        .unwrap_or(config.capture.notification_timeout);

    let state_file = state_file_path();

    // Check if recording is already active
    if state_file.exists() {
        // Read state
        let state_data =
            fs::read_to_string(&state_file).context("Failed to read record state file")?;
        let state: RecordState =
            serde_json::from_str(&state_data).context("Failed to parse record state JSON")?;

        if debug {
            eprintln!("Stopping recording: {:?}", state);
        }

        // Stop wf-recorder (SIGINT / -2 to save the video cleanly with MP4 headers)
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

        // Delete state file
        let _ = fs::remove_file(&state_file);

        let mut final_path = state.video_path.clone();
        let mut conversion_succeeded = false;

        if let Some(ref temp_path) = state.temp_video_path {
            if debug {
                eprintln!("Converting temporary video {} to GIF {}", temp_path, state.video_path);
            }
            if !silent {
                let _ = Notification::new()
                    .summary("🎥 正在转换GIF...")
                    .body("录像已结束，正在生成高质量GIF，请稍候...")
                    .timeout(notif_timeout as i32)
                    .appname("Shot")
                    .show();
            }

            let ffmpeg_status = Command::new("ffmpeg")
                .arg("-y")
                .arg("-i").arg(temp_path)
                .arg("-vf").arg("fps=15,scale=flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse")
                .arg(&state.video_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();

            match ffmpeg_status {
                Ok(status) if status.success() => {
                    conversion_succeeded = true;
                    // Delete intermediate file
                    let _ = fs::remove_file(temp_path);
                    if debug {
                        eprintln!("GIF conversion succeeded, deleted temp file.");
                    }
                }
                other => {
                    eprintln!("Warning: ffmpeg conversion failed or returned error: {:?}", other);
                    // Fallback to original webm
                    final_path = temp_path.clone();
                }
            }
        }

        // Copy the output path to clipboard
        let wl_copy_cmd = Command::new("wl-copy").stdin(Stdio::piped()).spawn();
        if let Ok(mut child) = wl_copy_cmd {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(final_path.as_bytes());
            }
            let _ = child.wait();
        }

        // Send success notification
        if !silent {
            let (summary, body) = if state.temp_video_path.is_some() {
                if conversion_succeeded {
                    ("🎥 GIF已生成".to_string(), format!("GIF已保存至: {}\n路径已复制到剪贴板", final_path))
                } else {
                    ("🎥 GIF转换失败".to_string(), format!("转换失败，原视频已保存至: {}\n路径已复制到剪贴板", final_path))
                }
            } else {
                ("🎥 录屏已完成".to_string(), format!("视频已保存至: {}\n路径已复制到剪贴板", final_path))
            };

            let _ = Notification::new()
                .summary(&summary)
                .body(&body)
                .timeout(notif_timeout as i32)
                .appname("Shot")
                .show();
        }

        if debug {
            eprintln!("Recording stopped. File: {}", final_path);
        }

        Ok(())
    } else {
        // Start recording
        let save_dir = config::get_recordings_dir(args.output_folder.clone(), config, debug)?;
        let save_dir = config::ensure_directory(&save_dir.to_string_lossy())?;

        // Select region
        let geometry = selector::select_region(debug)?;

        // Query the monitor containing the selected region, so the overlay border
        // is drawn in the same output coordinate space as the recording.
        let monitor_info = crate::external::get_monitor_info_for_geometry(&geometry, debug)
            .unwrap_or_else(|_| crate::external::MonitorInfo {
                name: "eDP-1".to_string(),
                scale: 1.0,
                x: 0,
                y: 0,
                width: i32::MAX,
                height: i32::MAX,
            });
        let scale = monitor_info.scale;

        let filename = format!(
            "record_{}.{}",
            Local::now().format("%Y-%m-%d-%H%M%S"),
            config.record.format
        );
        let video_path = save_dir.join(filename);
        let video_path_str = video_path.to_string_lossy().to_string();

        let is_gif = config.record.format == "gif";
        let intermediate_ext = if config.record.codec.contains("vp9") || config.record.codec.contains("vp8") {
            "webm"
        } else {
            "mp4"
        };
        let record_path_str = if is_gif {
            video_path.with_extension(intermediate_ext).to_string_lossy().to_string()
        } else {
            video_path_str.clone()
        };

        if debug {
            eprintln!(
                "Starting screen recording. Target File: {}, Recording File: {}, Region: {:?}, Scale: {}",
                video_path_str, record_path_str, geometry, scale
            );
        }

        // Spawn wl-screenrec with configured parameters
        let fps_arg = config.record.fps.to_string();
        let geom_str = format!(
            "{},{} {}x{}",
            geometry.x, geometry.y, geometry.width, geometry.height
        );

        let mut cmd = Command::new("wl-screenrec");
        cmd.arg("-g")
            .arg(&geom_str)
            .arg("-f")
            .arg(&record_path_str)
            .arg("--max-fps")
            .arg(&fps_arg);

        if config.record.hide_cursor {
            cmd.arg("--no-cursor");
        }

        if config.record.hwaccel == "none" {
            cmd.arg("--no-hw");
        }

        if !config.record.codec.is_empty() && config.record.codec != "auto" {
            cmd.arg("--codec").arg(&config.record.codec);
        }

        cmd.args(&config.record.command_args);
        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        let rec_child = cmd
            .spawn()
            .context("Failed to spawn wl-screenrec. Please ensure it is installed: sudo pacman -S wl-screenrec")?;
        let rec_pid = rec_child.id();

        // Spawn overlay
        let log_file = std::fs::File::create(std::env::temp_dir().join("shot_overlay.log")).ok();
        let stderr_cfg = log_file.map(Stdio::from).unwrap_or_else(|| Stdio::null());

        let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
        let overlay_child = Command::new(exe_path)
            .arg("overlay")
            .arg("--x")
            .arg(geometry.x.to_string())
            .arg("--y")
            .arg(geometry.y.to_string())
            .arg("--w")
            .arg(geometry.width.to_string())
            .arg("--h")
            .arg(geometry.height.to_string())
            .arg("--scale")
            .arg(scale.to_string())
            .arg("--monitor")
            .arg(&monitor_info.name)
            .arg("--ox")
            .arg(monitor_info.x.to_string())
            .arg("--oy")
            .arg(monitor_info.y.to_string())
            .stdout(Stdio::null())
            .stderr(stderr_cfg)
            .spawn()
            .context("Failed to spawn overlay process")?;
        let overlay_pid = overlay_child.id();

        // Write state file
        let state = RecordState {
            pid: rec_pid,
            overlay_pid,
            video_path: video_path_str,
            temp_video_path: if is_gif { Some(record_path_str) } else { None },
        };
        let state_json =
            serde_json::to_string_pretty(&state).context("Failed to serialize state to JSON")?;
        fs::write(&state_file, state_json).context("Failed to write record state file")?;

        // Send starting notification
        if !silent {
            let _ = Notification::new()
                .summary("🎥 录屏中")
                .body("再次运行命令停止录像")
                .timeout(notif_timeout as i32)
                .appname("Shot")
                .show();
        }

        Ok(())
    }
}
