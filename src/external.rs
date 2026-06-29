use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use std::io::Write;
use tempfile::Builder;
use notify_rust::Notification;
use serde_json::Value;
use std::time::Duration;

use crate::cli::Args;
use crate::config::Config;
use crate::geometry::Geometry;
use crate::selector;
use crate::freeze;
use crate::utils::output_with_timeout;

pub fn get_active_monitor_info(_debug: bool) -> Result<(String, f64, i32, i32)> {
    const IPC_TIMEOUT: Duration = Duration::from_secs(3);
    // Try Hyprland first
    if let Ok(output) = output_with_timeout(
        {
            let mut cmd = Command::new("hyprctl");
            cmd.arg("activeworkspace").arg("-j");
            cmd
        },
        IPC_TIMEOUT,
    ) {
        if let Ok(active_workspace) = serde_json::from_slice::<Value>(&output.stdout) {
            if let Ok(output_mon) = output_with_timeout(
                {
                    let mut cmd = Command::new("hyprctl");
                    cmd.arg("monitors").arg("-j");
                    cmd
                },
                IPC_TIMEOUT,
            ) {
                if let Ok(monitors) = serde_json::from_slice::<Value>(&output_mon.stdout) {
                    if let Some(arr) = monitors.as_array() {
                        if let Some(m) = arr.iter().find(|m| m["activeWorkspace"]["id"] == active_workspace["id"]) {
                            let name = m["name"].as_str().unwrap_or("").to_string();
                            let scale = m["scale"].as_f64().unwrap_or(1.0);
                            let x = m["x"].as_i64().unwrap_or(0) as i32;
                            let y = m["y"].as_i64().unwrap_or(0) as i32;
                            return Ok((name, scale, x, y));
                        }
                    }
                }
            }
        }
    }
    
    // Try Sway
    if let Ok(output) = output_with_timeout(
        {
            let mut cmd = Command::new("swaymsg");
            cmd.arg("-t").arg("get_workspaces");
            cmd
        },
        IPC_TIMEOUT,
    ) {
        if let Ok(workspaces) = serde_json::from_slice::<Value>(&output.stdout) {
            if let Some(arr) = workspaces.as_array() {
                if let Some(w) = arr.iter().find(|w| w["focused"].as_bool() == Some(true)) {
                    if let Some(focused_output) = w["output"].as_str() {
                        if let Ok(output_mon) = output_with_timeout(
                            {
                                let mut cmd = Command::new("swaymsg");
                                cmd.arg("-t").arg("get_outputs");
                                cmd
                            },
                            IPC_TIMEOUT,
                        ) {
                            if let Ok(outputs) = serde_json::from_slice::<Value>(&output_mon.stdout) {
                                if let Some(arr_mon) = outputs.as_array() {
                                    if let Some(o) = arr_mon.iter().find(|o| o["name"].as_str() == Some(focused_output)) {
                                        let name = o["name"].as_str().unwrap_or("").to_string();
                                        let scale = o["scale"].as_f64().unwrap_or(1.0);
                                        let rect = &o["rect"];
                                        let x = rect["x"].as_i64().unwrap_or(0) as i32;
                                        let y = rect["y"].as_i64().unwrap_or(0) as i32;
                                        return Ok((name, scale, x, y));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(("eDP-1".to_string(), 1.0, 0, 0))
}

pub fn run_external_screenshot_tool(
    args: &Args,
    config: &Config,
    is_ocr: bool,
) -> Result<()> {
    let debug = args.debug;
    let silent = args.silent;
    let notif_timeout = args.notif_timeout.unwrap_or(config.capture.notification_timeout);

    // 1. Select region
    let geometry = selector::select_region(debug)?;

    // 2. Start freeze overlay if requested
    let freeze = if args.freeze {
        true
    } else {
        config.advanced.freeze_on_region
    };
    
    let (monitor_name, scale, _, _) = get_active_monitor_info(debug).unwrap_or(("".to_string(), 1.0, 0, 0));
    
    let freeze_guard = if freeze {
        let guard = freeze::start_freeze(None, debug)?;
        Some(guard)
    } else {
        None
    };

    // Stop freeze overlay immediately
    if let Some(guard) = freeze_guard {
        guard.stop()?;
        std::thread::sleep(std::time::Duration::from_millis(150));
    } else {
        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    // 3. Capture region using grim CLI to PNG bytes
    let png_bytes = crate::utils::capture_region_with_grim_cli(&geometry)?;

    // 4. Save PNG to a unique temp file in /tmp/
    let mut temp_file = Builder::new()
        .prefix("shot_temp_")
        .suffix(".png")
        .tempfile()
        .context("Failed to create temporary file for screenshot")?;
        
    temp_file.write_all(&png_bytes).context("Failed to write screenshot bytes to temporary file")?;
    let temp_path = temp_file.path().to_path_buf();
    let temp_path_str = temp_path.to_string_lossy().to_string();

    // 5. Build external command by replacing placeholders
    let cmd_template = if is_ocr {
        &config.ocr.command
    } else {
        &config.satty.command
    };

    let mut cmd_str = cmd_template.clone();
    if cmd_str.contains("{}") {
        cmd_str = cmd_str.replace("{}", &temp_path_str);
    } else if cmd_str.contains("{path}") {
        cmd_str = cmd_str.replace("{path}", &temp_path_str);
    } else {
        // Append path at the end if no placeholder is found
        cmd_str = format!("{} {}", cmd_str, temp_path_str);
    }

    // Replace other placeholders
    cmd_str = cmd_str.replace("{x}", &geometry.x.to_string());
    cmd_str = cmd_str.replace("{y}", &geometry.y.to_string());
    cmd_str = cmd_str.replace("{w}", &geometry.width.to_string());
    cmd_str = cmd_str.replace("{h}", &geometry.height.to_string());
    cmd_str = cmd_str.replace("{scale}", &scale.to_string());
    cmd_str = cmd_str.replace("{monitor}", &monitor_name);

    if debug {
        eprintln!("Running command: {}", cmd_str);
    }

    // Parse the command string to executable and args
    let status_res = if is_ocr {
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
        if debug {
            eprintln!("OCR stdout: {}", ocr_stdout);
            eprintln!("OCR stderr: {}", ocr_stderr);
        }

        // Parse and clean OCR text
        let cleaned_txt = clean_ocr_text(&ocr_stdout);

        if !cleaned_txt.is_empty() {
            // Copy to clipboard
            let mut wl_copy = Command::new("wl-copy")
                .stdin(Stdio::piped())
                .spawn()
                .context("Failed to start wl-copy")?;
            wl_copy.stdin.as_mut().unwrap().write_all(cleaned_txt.as_bytes()).context("Failed to write to wl-copy")?;
            let _ = wl_copy.wait();

            // Send notification
            if !silent {
                let _ = Notification::new()
                    .summary("OCR完成")
                    .body(&cleaned_txt)
                    .timeout(notif_timeout as i32)
                    .appname("Shot")
                    .show();
            }
        } else {
            if !silent {
                let _ = Notification::new()
                    .summary("OCR完成")
                    .body("未识别出文字")
                    .timeout(notif_timeout as i32)
                    .appname("Shot")
                    .show();
            }
        }
        Ok(())
    } else {
        // Edit mode: inherit stdio so UI works
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&cmd_str)
            .spawn()
            .context("Failed to run Edit command")?;

        let status = child.wait().context("Failed waiting for Edit process")?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Edit process exited with error status"))
        }
    };

    // Cleanup temp file
    drop(temp_file);

    status_res
}

fn clean_ocr_text(input: &str) -> String {
    let mut lines = Vec::new();
    for line in input.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        // Filter out status/model loading messages
        if t.starts_with('ℹ') || t.starts_with('✓') || t.starts_with("CPU") || t.starts_with("The device") || t.starts_with('↓') || t.starts_with("No text") {
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
