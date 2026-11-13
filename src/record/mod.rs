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

use crate::capture_session::{self, Phase, Session, SessionLock};
use crate::cli::Args;
use crate::config;
use crate::selector;

fn state_file_path() -> Result<PathBuf> {
    capture_session::state_path("record.json")
}

#[derive(Serialize, Deserialize, Debug)]
struct RecordState {
    session: Session,
    video_path: String,
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

    let state_file = state_file_path()?;
    let _lock = SessionLock::acquire(&state_file)?;
    // Anything still journaled here belongs to a session whose state file is
    // gone; it can no longer be stopped by pressing the hotkey.
    crate::capture_session::reap_orphans(&state_file, debug);

    // Check if recording is already active
    if state_file.exists() {
        // Read state
        let state_data =
            fs::read_to_string(&state_file).context("Failed to read record state file")?;
        // A truncated or stale state file must not wedge the hotkey forever:
        // without this the same unparseable file is re-read on every press and
        // the command dies before it can ever start a new recording.
        let mut state: RecordState = match serde_json::from_str(&state_data) {
            Ok(state) => state,
            Err(error) => {
                let _ = fs::remove_file(&state_file);
                return Err(error).context(
                    "Record state was unreadable and has been cleared; run record again",
                );
            }
        };

        if debug {
            eprintln!("Stopping recording: {:?}", state);
        }

        // A stop failure must not abort the flow: the processes were already
        // signalled best-effort inside stop(), and returning here would leave
        // the state file as the only record of a session nobody can finish.
        if let Err(error) = state.session.stop() {
            eprintln!("Stopping the recording reported: {error:#}");
        }
        state.session.phase = Phase::Finalizing;
        capture_session::write_state(&state_file, &state)?;
        if let Err(error) = finalize_recording(&state) {
            // Failure ends the session, same as longshot: finalizing the same
            // file again would fail identically. The raw recording is kept so
            // it can still be recovered by hand.
            let _ = fs::remove_file(&state_file);
            let _ = crate::capture_session::clear_journal(&state_file);
            if !silent {
                let _ = Notification::new()
                    .summary("录屏保存失败")
                    .body(&format!("{}\n原始录像保留于: {}", error, state.recording_path))
                    .timeout(notif_timeout as i32)
                    .appname("Hyshot")
                    .show();
            }
            return Err(error).context(format!(
                "Raw recording retained at {}",
                state.recording_path
            ));
        }
        fs::remove_file(&state_file).context("Cannot remove completed recording state")?;
        // Session is fully over; nothing is left to reap.
        let _ = crate::capture_session::clear_journal(&state_file);
        let final_path = &state.video_path;
        let mut copied = None;
        if config.record.after_save.eq_ignore_ascii_case("copy") {
            copied = Some(copy_path(final_path));
        } else if config.record.after_save.eq_ignore_ascii_case("open") {
            crate::save::open_path(Path::new(final_path));
        }
        if !silent {
            let copy_suffix = match copied {
                Some(true) => "\n路径已复制到剪贴板",
                Some(false) => "\n剪贴板复制失败",
                None => "",
            };
            let body = format!("视频已保存至: {}{}", final_path, copy_suffix);
            crate::save::spawn_notification_with_actions(
                if state.temp_video_path.is_some() {
                    "GIF已生成".to_string()
                } else {
                    "录屏已完成".to_string()
                },
                body,
                String::new(),
                notif_timeout,
                PathBuf::from(final_path),
            );
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
        let monitor_info = crate::compositor::get_monitor_info_for_geometry(&geometry, debug)?;
        let scale = monitor_info.scale;

        let filename = format!(
            "record_{}.{}",
            Local::now().format("%Y-%m-%d-%H%M%S-%3f"),
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

        let capture = crate::capture_session::StartedCapture::start(
            cmd,
            &geometry,
            &monitor_info,
            crate::capture_session::CaptureKind::Record,
            debug,
        )?;
        let session = capture.session()?;

        // Write state file
        let state = RecordState {
            session,
            video_path: video_path_str,
            recording_path: record_path_str.clone(),
            temp_video_path: if is_gif { Some(record_path_str) } else { None },
        };
        // Journal the identities before committing: once the state file is the
        // only owner, losing it would orphan two processes with no way back.
        crate::capture_session::journal_processes(&state_file, &state.session)?;
        capture.commit(&state_file, &state)?;

        // Send starting notification
        if !silent {
            let _ = Notification::new()
                .summary("🎥 录屏中")
                .body("再次运行命令停止录像")
                .timeout(notif_timeout as i32)
                .appname("Hyshot")
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

fn copy_path(path: &str) -> bool {
    let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() else {
        return false;
    };
    let written = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(path.as_bytes()).is_ok());
    child.wait().is_ok_and(|status| status.success()) && written
}

fn finalize_recording(state: &RecordState) -> Result<()> {
    anyhow::ensure!(
        fs::metadata(&state.recording_path)?.len() > 0,
        "Recording is empty"
    );
    if let Some(source) = &state.temp_video_path {
        let output = Path::new(&state.video_path);
        let temporary = tempfile::Builder::new().suffix(".gif").tempfile_in(
            output
                .parent()
                .context("Recording has no output directory")?,
        )?;
        let status = Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-y", "-i"])
            .arg(source)
            .args([
                "-vf",
                "fps=15,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
            ])
            .arg(temporary.path())
            .stdout(Stdio::null())
            .status()
            .context("Cannot start GIF conversion")?;
        anyhow::ensure!(status.success(), "GIF conversion failed");
        anyhow::ensure!(
            temporary.as_file().metadata()?.len() > 0,
            "GIF output is empty"
        );
        temporary
            .persist_noclobber(output)
            .context("Cannot publish GIF; existing files are never overwritten")?;
        // Publishing succeeded; cleanup must not turn a finished output into a retry.
        if let Err(error) = fs::remove_file(source) {
            eprintln!("Cannot remove intermediate recording: {error}");
        }
    } else {
        move_finished_recording(&state.recording_path, &state.video_path)?;
    }
    Ok(())
}

fn move_finished_recording(from: &str, to: &str) -> Result<()> {
    fs::hard_link(from, to).with_context(|| {
        format!(
            "Cannot publish recording '{}' as '{}' without overwriting",
            from, to
        )
    })?;
    if let Err(error) = fs::remove_file(from) {
        eprintln!("Cannot remove intermediate recording: {error}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishing_never_overwrites_and_retains_source_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        let target = dir.path().join("target.mp4");
        fs::write(&source, b"new recording").unwrap();
        fs::write(&target, b"existing recording").unwrap();
        assert!(
            move_finished_recording(source.to_str().unwrap(), target.to_str().unwrap()).is_err()
        );
        assert_eq!(fs::read(&source).unwrap(), b"new recording");
        assert_eq!(fs::read(&target).unwrap(), b"existing recording");
        fs::remove_file(&target).unwrap();
        move_finished_recording(source.to_str().unwrap(), target.to_str().unwrap()).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(&target).unwrap(), b"new recording");
    }

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
