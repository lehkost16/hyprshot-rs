use anyhow::{Context, Result};
use notify_rust::Notification;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::Builder;

use crate::geometry::Geometry;

/// Hard ceiling for an external OCR/translation command.
const EXTERNAL_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub struct ExternalOptions {
    pub debug: bool,
    pub silent: bool,
    pub notification_timeout: u32,
}

/// Quotes a value for safe interpolation into a POSIX shell command.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
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
    // The template is a shell command: every substituted value is quoted so a
    // path or monitor name containing shell metacharacters stays one word.
    let temp_path_str = shell_quote(&temp_path.to_string_lossy());

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
        cmd_str = cmd_str.replace("{monitor}", &shell_quote(&monitor.name));
    }

    if debug {
        eprintln!("Running command: {}", cmd_str);
    }

    // OCR mode: capture stdout
    // An OCR/translation helper that hangs must not hang the capture command:
    // the whole point of the external workflow is a quick round trip.
    let output = crate::utils::output_with_timeout(
        {
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(&cmd_str);
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            cmd
        },
        EXTERNAL_COMMAND_TIMEOUT,
    )
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
                .appname("Hyshot")
                .show();
        }
    } else {
        if !silent {
            let _ = Notification::new()
                .summary("External tool completed")
                .body("未识别出文字")
                .timeout(notif_timeout as i32)
                .appname("Hyshot")
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

        let mut s = t;

        // Strip leading "[digits] " index prefix if present
        if s.starts_with('[') {
            if let Some(close_idx) = s.find(']') {
                let inside = &s[1..close_idx];
                if !inside.is_empty() && inside.chars().all(|c| c.is_ascii_digit()) {
                    s = s[close_idx + 1..].trim_start();
                }
            }
        }

        // Strip trailing " (digits%)" confidence suffix if present
        if s.ends_with("%)") {
            if let Some(open_idx) = s.rfind('(') {
                let inside = &s[open_idx + 1..s.len() - 2];
                if !inside.is_empty() && inside.chars().all(|c| c.is_ascii_digit()) {
                    s = s[..open_idx].trim_end();
                }
            }
        }

        if !s.is_empty() {
            lines.push(s.to_string());
        }
    }

    lines.join("\n").trim().to_string()
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
    fn clean_ocr_text_preserves_internal_brackets_and_newlines() {
        let raw = "\
CPU Group: [ 11 8 6 ], 416548
The device supports: i8sdot:0
[1] const list = [item1, item2]; (99%)
[2] println!(\"result (test): {}\", list[0]); (100%)
[3] 【通知】测试公告 (附录 A) (95%)
";
        let cleaned = clean_ocr_text(raw);
        assert_eq!(
            cleaned,
            "const list = [item1, item2];\nprintln!(\"result (test): {}\", list[0]);\n【通知】测试公告 (附录 A)"
        );
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
