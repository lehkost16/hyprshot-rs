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

const STATE_FILE: &str = "/tmp/shot-record.json";

#[derive(Serialize, Deserialize, Debug)]
struct RecordState {
    pid: u32,
    overlay_pid: u32,
    video_path: String,
}

fn is_process_running(pid: u32) -> bool {
    Path::new(&format!("/proc/{}", pid)).exists()
}

pub fn handle_record(args: &Args, config: &config::Config) -> Result<()> {
    let debug = args.debug;
    let silent = args.silent;
    let notif_timeout = args.notif_timeout.unwrap_or(config.capture.notification_timeout);

    // Check if recording is already active
    if Path::new(STATE_FILE).exists() {
        // Read state
        let state_data = fs::read_to_string(STATE_FILE)
            .context("Failed to read record state file")?;
        let state: RecordState = serde_json::from_str(&state_data)
            .context("Failed to parse record state JSON")?;

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
        let _ = fs::remove_file(STATE_FILE);

        // Copy the output video path to clipboard
        let mut wl_copy_cmd = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn();
        if let Ok(mut child) = wl_copy_cmd {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(state.video_path.as_bytes());
            }
            let _ = child.wait();
        }

        // Send success notification
        if !silent {
            let _ = Notification::new()
                .summary("🎥 录屏已完成")
                .body(&format!("视频已保存至: {}\n路径已复制到剪贴板", state.video_path))
                .timeout(notif_timeout as i32)
                .appname("Shot")
                .show();
        }

        if debug {
            eprintln!("Recording stopped. File: {}", state.video_path);
        }

        Ok(())
    } else {
        // Start recording
        let home_dir = dirs::home_dir().context("Failed to get home directory")?;
        let save_dir = home_dir.join("Videos").join("record");
        
        // Ensure directory exists
        fs::create_dir_all(&save_dir).context("Failed to create Videos/record directory")?;

        // Select region
        let geometry = selector::select_region(debug)?;

        // Query active monitor name & scale factor
        let (monitor, scale_f, ox, oy) = crate::external::get_active_monitor_info(debug)
            .unwrap_or(("eDP-1".to_string(), 1.0, 0, 0));
        let scale = scale_f;

        let filename = format!("record_{}.webm", Local::now().format("%Y-%m-%d-%H%M%S"));
        let video_path = save_dir.join(filename);
        let video_path_str = video_path.to_string_lossy().to_string();

        if debug {
            eprintln!(
                "Starting screen recording. Video: {}, Region: {:?}, Scale: {}",
                video_path_str, geometry, scale
            );
        }

        // Spawn wf-recorder with standard recording parameters for WebM VP9
        let fps_arg = format!("fps={}", config.record.fps);
        let crf_arg = config.record.crf.to_string();
        
        let rec_child = Command::new("wf-recorder")
            .arg("-g")
            .arg(format!("{},{} {}x{}", geometry.x, geometry.y, geometry.width, geometry.height))
            .arg("-f")
            .arg(&video_path_str)
            .arg("-c").arg("libvpx-vp9")
            .arg("-p").arg(format!("crf={}", crf_arg))
            .arg("-F")
            .arg(&fps_arg)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn wf-recorder. Please ensure it is installed.")?;
        let rec_pid = rec_child.id();

        // Spawn overlay
        let log_file = std::fs::File::create("/tmp/shot_overlay.log").ok();
        let stderr_cfg = log_file.map(Stdio::from).unwrap_or_else(|| Stdio::null());

        let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
        let overlay_child = Command::new(exe_path)
            .arg("overlay")
            .arg("--x").arg(geometry.x.to_string())
            .arg("--y").arg(geometry.y.to_string())
            .arg("--w").arg(geometry.width.to_string())
            .arg("--h").arg(geometry.height.to_string())
            .arg("--scale").arg(scale.to_string())
            .arg("--monitor").arg(&monitor)
            .arg("--ox").arg(ox.to_string())
            .arg("--oy").arg(oy.to_string())
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
        };
        let state_json = serde_json::to_string_pretty(&state)
            .context("Failed to serialize state to JSON")?;
        fs::write(STATE_FILE, state_json)
            .context("Failed to write record state file")?;

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
