use anyhow::{Context, Result};
use chrono::Local;
use notify_rust::Notification;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::cli::Args;
use crate::config;
use crate::selector;
use crate::utils;

fn state_file_path() -> std::path::PathBuf {
    utils::runtime_state_path("record.json")
}

#[derive(Serialize, Deserialize, Debug)]
struct RecordState {
    pid: u32,
    overlay_pid: u32,
    video_path: String,
    #[serde(default)]
    recording_path: String,
    #[serde(default)]
    temp_video_path: Option<String>,
}

pub fn handle_record(args: &Args, config: &config::Config) -> Result<()> {
    let debug = args.debug;
    let silent = args.silent || !config.capture.notification;
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

        crate::capture_session::stop(state.pid, state.overlay_pid)?;

        // Delete state file
        let _ = fs::remove_file(&state_file);

        let mut final_path = state.video_path.clone();
        let mut conversion_succeeded = false;

        if let Some(ref temp_path) = state.temp_video_path {
            if debug {
                eprintln!(
                    "Converting temporary video {} to GIF {}",
                    temp_path, state.video_path
                );
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
                .arg("-i")
                .arg(temp_path)
                .arg("-vf")
                .arg("fps=15,scale=flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse")
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
                    eprintln!(
                        "Warning: ffmpeg conversion failed or returned error: {:?}",
                        other
                    );
                    // Fallback to original webm
                    final_path = temp_path.clone();
                }
            }
        } else if !state.recording_path.is_empty() && state.recording_path != state.video_path {
            move_finished_recording(&state.recording_path, &state.video_path)?;
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
                    (
                        "🎥 GIF已生成".to_string(),
                        format!("GIF已保存至: {}\n路径已复制到剪贴板", final_path),
                    )
                } else {
                    (
                        "🎥 GIF转换失败".to_string(),
                        format!(
                            "转换失败，原视频已保存至: {}\n路径已复制到剪贴板",
                            final_path
                        ),
                    )
                }
            } else {
                (
                    "🎥 录屏已完成".to_string(),
                    format!("视频已保存至: {}\n路径已复制到剪贴板", final_path),
                )
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
        let monitor_info = crate::compositor::get_monitor_info_for_geometry(&geometry, debug)
            .unwrap_or_else(|_| crate::compositor::MonitorInfo {
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
        let codec = config.record.resolved_codec();
        let bitrate = config.record.resolved_bitrate();
        let intermediate_ext = if matches!(codec, "vp9" | "vp8" | "av1") {
            "webm"
        } else {
            "mp4"
        };
        let recording_path = runtime_recording_path(&video_path, intermediate_ext)?;
        let record_path_str = recording_path.to_string_lossy().to_string();

        if debug {
            eprintln!(
                "Starting screen recording. Target File: {}, Recording File: {}, Region: {:?}, Scale: {}, Codec: {}, Bitrate: {}",
                video_path_str, record_path_str, geometry, scale, codec, bitrate
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
            .arg(&fps_arg)
            .arg("--bitrate")
            .arg(bitrate);

        if config.record.hide_cursor {
            cmd.arg("--no-cursor");
        }

        if config.record.hwaccel == "none" {
            cmd.arg("--no-hw");
        }

        cmd.arg("--codec").arg(codec);

        if config.record.audio {
            cmd.arg("--audio");
        }

        cmd.args(&config.record.command_args);
        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        let capture =
            crate::capture_session::StartedCapture::start(cmd, &geometry, &monitor_info, debug)?;
        let rec_pid = capture.recorder_pid();
        let overlay_pid = capture.overlay_pid();

        // Write state file
        let state = RecordState {
            pid: rec_pid,
            overlay_pid,
            video_path: video_path_str,
            recording_path: record_path_str.clone(),
            temp_video_path: if is_gif { Some(record_path_str) } else { None },
        };
        capture.commit(&state_file, &state)?;

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

fn runtime_recording_path(final_path: &Path, ext: &str) -> Result<PathBuf> {
    let stem = final_path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Failed to build temporary recording file name")?;
    let filename = format!(".{}.recording.{}", stem, ext);
    let parent = final_path
        .parent()
        .context("Failed to determine recording output directory")?;
    Ok(parent.join(filename))
}

fn move_finished_recording(from: &str, to: &str) -> Result<()> {
    fs::rename(from, to)
        .with_context(|| format!("Failed to rename recording from '{}' to '{}'", from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_recording_path_uses_hidden_file_next_to_final_output() {
        let final_path = Path::new("/tmp/videos/record_2026-08-17-120000.mp4");
        let temp_path = runtime_recording_path(final_path, "mp4").unwrap();
        let name = temp_path.file_name().and_then(|s| s.to_str()).unwrap();

        assert_eq!(name, ".record_2026-08-17-120000.recording.mp4");
        assert_eq!(
            temp_path,
            Path::new("/tmp/videos/.record_2026-08-17-120000.recording.mp4")
        );
    }
}
