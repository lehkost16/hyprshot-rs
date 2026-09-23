use anyhow::{Context, Result};
use notify_rust::Notification;
use std::fs::{create_dir_all, write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::geometry::Geometry;

pub struct SaveOptions {
    pub capture: crate::config::CaptureConfig,
    pub clipboard_only: bool,
    pub raw: bool,
    pub command: Option<Vec<String>>,
    pub silent: bool,
    pub notif_timeout: u32,
    pub debug: bool,
    pub image_bytes: Option<Vec<u8>>,
    pub upload: bool,
    pub upload_command: String,
}

#[cfg(feature = "grim")]
#[allow(clippy::too_many_arguments)]
pub fn save_geometry_with_grim(
    geometry: &Geometry,
    save_fullpath: &PathBuf,
    options: SaveOptions,
) -> Result<()> {
    use std::io::Write;

    if options.debug {
        eprintln!("Saving geometry with grim CLI: {}", geometry);
    }

    let image_bytes = match options.image_bytes {
        Some(bytes) => bytes,
        None => crate::utils::capture_region_with_grim_cli(geometry, &options.capture)?,
    };

    if options.raw {
        std::io::stdout().write_all(&image_bytes)?;
        return Ok(());
    }

    if !options.clipboard_only {
        create_dir_all(save_fullpath.parent().unwrap())
            .context("Failed to create screenshot directory")?;

        write(save_fullpath, &image_bytes).context(format!(
            "Failed to save screenshot to '{}'",
            save_fullpath.display()
        ))?;

        let wl_copy_result = (|| -> Result<()> {
            let mut wl_copy = Command::new("wl-copy")
                .arg("--type")
                .arg("image/png")
                .stdin(Stdio::piped())
                .spawn()
                .context("Failed to start wl-copy")?;
            wl_copy
                .stdin
                .as_mut()
                .unwrap()
                .write_all(&image_bytes)
                .context("Failed to write to wl-copy stdin")?;
            // Best-effort in normal mode: don't block on wl-copy completion.
            std::mem::drop(wl_copy);
            Ok(())
        })();
        if let Err(err) = wl_copy_result {
            eprintln!("Warning: failed to copy screenshot to clipboard: {}", err);
        }

        if let Some(cmd) = options.command {
            let cmd_status = Command::new(&cmd[0])
                .args(&cmd[1..])
                .arg(save_fullpath)
                .status()
                .context(format!("Failed to run command '{}'", cmd[0]))?;
            if !cmd_status.success() {
                return Err(anyhow::anyhow!("Command '{}' failed", cmd[0]));
            }
        }
    } else {
        let mut wl_copy = Command::new("wl-copy")
            .arg("--type")
            .arg("image/png")
            .stdin(Stdio::piped())
            .spawn()
            .context("Failed to start wl-copy")?;
        wl_copy
            .stdin
            .as_mut()
            .unwrap()
            .write_all(&image_bytes)
            .context("Failed to write to wl-copy stdin")?;
        std::mem::drop(wl_copy);
    }

    let mut uploaded_url: Option<String> = None;
    if options.upload && !options.clipboard_only && !options.raw {
        if options.upload_command.is_empty() {
            eprintln!("Warning: Upload command is not configured in hyshot config file.");
            if !options.silent {
                let _ = Notification::new()
                    .summary("上传未配置")
                    .body("请在配置文件中设置 capture.upload_command")
                    .timeout(options.notif_timeout as i32)
                    .appname("Hyshot")
                    .show();
            }
        } else {
            let cmd_str = options
                .upload_command
                .replace("{path}", &save_fullpath.to_string_lossy());
            if options.debug {
                eprintln!("Running upload command: {}", cmd_str);
            }
            let upload_res = Command::new("sh").arg("-c").arg(&cmd_str).output();
            match upload_res {
                Ok(output) if output.status.success() => {
                    let stdout_str = String::from_utf8_lossy(&output.stdout);
                    if options.debug {
                        eprintln!("Upload output: {}", stdout_str);
                    }
                    if let Some(url) = extract_url(&stdout_str) {
                        // Copy URL to clipboard
                        let wl_copy_res = Command::new("wl-copy").stdin(Stdio::piped()).spawn();
                        if let Ok(mut child) = wl_copy_res {
                            if let Some(mut stdin) = child.stdin.take() {
                                let _ = stdin.write_all(url.as_bytes());
                            }
                            let _ = child.wait();
                        }
                        uploaded_url = Some(url);
                    } else {
                        eprintln!("Warning: Could not extract URL from upload command output.");
                    }
                }
                Ok(output) => {
                    eprintln!(
                        "Warning: Upload command failed with exit status: {:?}",
                        output.status
                    );
                }
                Err(e) => {
                    eprintln!("Warning: Failed to execute upload command: {}", e);
                }
            }
        }
    }

    if !options.silent {
        let (summary, message) = if let Some(ref url) = uploaded_url {
            (
                "上传完成".to_string(),
                format!("图片链接已复制到剪贴板:\n{}", url),
            )
        } else if options.clipboard_only {
            (
                "Screenshot saved".to_string(),
                "Image copied to the clipboard".to_string(),
            )
        } else {
            (
                "Screenshot saved".to_string(),
                format!(
                    "Image saved in <i>{}</i> and copied to the clipboard.",
                    save_fullpath.display()
                ),
            )
        };
        let icon_name = if options.clipboard_only {
            "edit-paste".to_string()
        } else {
            save_fullpath.to_str().unwrap_or("screenshot").to_string()
        };
        if let Err(err) = Notification::new()
            .summary(&summary)
            .body(&message)
            .icon(&icon_name)
            .timeout(options.notif_timeout as i32)
            .appname("Hyshot")
            .show()
        {
            eprintln!("Warning: failed to show notification: {}", err);
        }
    }

    Ok(())
}

fn extract_url(text: &str) -> Option<String> {
    let start_idx = text.find("http://").or_else(|| text.find("https://"))?;
    let rest = &text[start_idx..];
    let end_idx = rest.find(|c: char| {
        c.is_whitespace() || c == '"' || c == '\'' || c == '<' || c == '>' || c == '\\'
    });
    let url = match end_idx {
        Some(i) => &rest[..i],
        None => rest,
    };
    Some(url.trim().to_string())
}

#[allow(clippy::too_many_arguments)]
pub fn save_geometry(
    geometry: &Geometry,
    save_fullpath: &PathBuf,
    options: SaveOptions,
) -> Result<()> {
    #[cfg(feature = "grim")]
    return save_geometry_with_grim(geometry, save_fullpath, options);
    #[cfg(not(feature = "grim"))]
    compile_error!("Feature 'grim' must be enabled to save screenshots");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_url() {
        assert_eq!(
            extract_url("https://tmp.link/f/123456"),
            Some("https://tmp.link/f/123456".to_string())
        );
        assert_eq!(
            extract_url("{\"url\": \"https://tmp.link/f/123456\"}"),
            Some("https://tmp.link/f/123456".to_string())
        );
        assert_eq!(
            extract_url("Upload finished. Link: http://example.com/image.png\nThank you!"),
            Some("http://example.com/image.png".to_string())
        );
        assert_eq!(extract_url("some text without url"), None);
    }
}
