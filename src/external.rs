use anyhow::{Context, Result};
use notify_rust::Notification;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::Builder;

use crate::geometry::Geometry;

pub struct ExternalOptions {
    pub debug: bool,
    pub silent: bool,
    pub notification_timeout: u32,
}

/// The process adapter owns temporary files, never selection or screen capture.
pub fn run(
    template: &str,
    image_bytes: &[u8],
    geometry: Geometry,
    options: ExternalOptions,
) -> Result<()> {
    let debug = options.debug;
    let silent = options.silent;
    let notif_timeout = options.notification_timeout;
    // Keep the temporary image alive until OCR finishes.
    let mut temp_file = Builder::new()
        .prefix("shot_temp_")
        .suffix(".png")
        .tempfile()
        .context("Failed to create temporary file for screenshot")?;

    temp_file
        .write_all(image_bytes)
        .context("Failed to write screenshot bytes to temporary file")?;
    let temp_path = temp_file.path().to_path_buf();
    let temp_path_str = temp_path.to_string_lossy().to_string();

    // Build the configured OCR command.
    let mut cmd_str = template.to_owned();
    if cmd_str.contains("{}") {
        cmd_str = cmd_str.replace("{}", &temp_path_str);
    } else if cmd_str.contains("{path}") {
        cmd_str = cmd_str.replace("{path}", &temp_path_str);
    } else {
        // Append path at the end if no placeholder is found
        cmd_str = format!("{} {}", cmd_str, temp_path_str);
    }

    // Query the compositor only for commands that actually need output metadata.
    let monitor_info = if cmd_str.contains("{scale}") || cmd_str.contains("{monitor}") {
        Some(crate::compositor::get_monitor_info_for_geometry(
            &geometry, debug,
        )?)
    } else {
        None
    };

    // Replace other placeholders
    cmd_str = cmd_str.replace("{x}", &geometry.x.to_string());
    cmd_str = cmd_str.replace("{y}", &geometry.y.to_string());
    cmd_str = cmd_str.replace("{w}", &geometry.width.to_string());
    cmd_str = cmd_str.replace("{h}", &geometry.height.to_string());
    if let Some(monitor) = monitor_info {
        cmd_str = cmd_str.replace("{scale}", &monitor.scale.to_string());
        cmd_str = cmd_str.replace("{monitor}", &monitor.name);
    }

    if debug {
        eprintln!("Running command: {}", cmd_str);
    }

    // OCR mode: capture stdout
    let output = Command::new("sh")
        .arg("-c")
        .arg(&cmd_str)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to run OCR command")?;

    let ocr_stdout = String::from_utf8_lossy(&output.stdout);
    let ocr_stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::ensure!(
        output.status.success(),
        "External command failed ({}): {}",
        output.status,
        ocr_stderr.trim()
    );
    if debug {
        eprintln!("OCR stdout: {}", ocr_stdout);
        eprintln!("OCR stderr: {}", ocr_stderr);
    }

    // Parse and clean OCR text
    let cleaned_txt = clean_ocr_text(&ocr_stdout);

    if !cleaned_txt.is_empty() {
        // Keep terminal invocations useful while the notification and clipboard
        // remain the feedback path for Hyprland key bindings.
        println!("{cleaned_txt}");

        // Copy to clipboard
        let mut wl_copy = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .context("Failed to start wl-copy")?;
        let written = wl_copy
            .stdin
            .take()
            .context("Missing wl-copy stdin")?
            .write_all(cleaned_txt.as_bytes());
        let status = wl_copy.wait().context("Failed waiting for wl-copy")?;
        written.context("Failed to write to wl-copy")?;
        anyhow::ensure!(status.success(), "wl-copy exited with {status}");

        // Send notification
        if !silent {
            let _ = Notification::new()
                .summary("External tool completed")
                .body(&cleaned_txt)
                .timeout(notif_timeout as i32)
                .appname("Shot")
                .show();
        }
    } else {
        if !silent {
            let _ = Notification::new()
                .summary("External tool completed")
                .body("未识别出文字")
                .timeout(notif_timeout as i32)
                .appname("Shot")
                .show();
        }
    }
    Ok(())
}

fn clean_ocr_text(input: &str) -> String {
    let mut lines = Vec::new();
    for line in input.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        // Filter out status/model loading messages
        if t.starts_with('ℹ')
            || t.starts_with('✓')
            || t.starts_with("CPU")
            || t.starts_with("The device")
            || t.starts_with('↓')
            || t.starts_with("No text")
        {
            continue;
        }

        // Remove [number] bracket info and percentages
        let mut cleaned = String::new();
        let mut chars = t.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '[' {
                while let Some(&next) = chars.peek() {
                    if next == ']' {
                        chars.next();
                        break;
                    }
                    chars.next();
                }
            } else if ch == '(' {
                let mut temp = String::new();
                let mut is_pct = false;
                while let Some(&next) = chars.peek() {
                    if next == ')' {
                        chars.next();
                        if temp.ends_with('%') {
                            is_pct = true;
                        }
                        break;
                    }
                    temp.push(chars.next().unwrap());
                }
                if !is_pct {
                    cleaned.push('(');
                    cleaned.push_str(&temp);
                    cleaned.push(')');
                }
            } else {
                cleaned.push(ch);
            }
        }

        lines.push(cleaned.trim().to_string());
    }

    let joined = lines.join(" ");
    joined.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> ExternalOptions {
        ExternalOptions {
            debug: false,
            silent: true,
            notification_timeout: 1000,
        }
    }

    #[test]
    fn failed_external_command_does_not_treat_stdout_as_success() {
        let geometry = Geometry::new(0, 0, 10, 10).unwrap();
        let error = run(
            "printf 'recognized text'; exit 9 # {path}",
            b"png",
            geometry,
            options(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("External command failed"));
    }

    #[test]
    fn external_command_receives_live_temporary_file() {
        let geometry = Geometry::new(0, 0, 10, 10).unwrap();
        run("test -s {path}", b"png", geometry, options()).unwrap();
    }
}
