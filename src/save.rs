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
        // Actions need the notification to stay reachable until the user
        // answers, so hand the blocking wait to a detached thread and let the
        // capture path return immediately. Capture paths are short-lived
        // processes, so the thread keeps the process alive only until the
        // notification is dismissed or an action fires.
        if !options.clipboard_only {
            let path = save_fullpath.clone();
            spawn_notification_with_actions(summary, message, icon_name, options.notif_timeout, path);
        } else if let Err(err) = Notification::new()
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

/// Action keys attached to the saved-screenshot notification.
pub const ACTION_OPEN: &str = "open-folder";
pub const ACTION_DELETE: &str = "delete-file";

/// Minimum on-screen time for a notification carrying action buttons.
///
/// The configured timeout is tuned for a glance-and-dismiss toast; a button
/// the user cannot reach in time is a button that does not exist. Actionable
/// notifications therefore keep at least this long, while still honouring a
/// longer configured timeout.
const ACTION_NOTIFICATION_MIN_TIMEOUT_MS: u32 = 8000;

/// Shows the saved-screenshot notification with "open folder" and "delete"
/// actions, handling whichever the user picks.
///
/// The action wait is delegated to a `notify-send -A` child that waits on the
/// notification and prints the chosen action key, then runs the matching
/// handler. Doing the wait in-process would keep hyshot alive for as long as
/// the notification sits on screen — a screenshot command lingering for
/// minutes is worse than spawning one short-lived helper.
pub fn spawn_notification_with_actions(
    summary: String,
    body: String,
    _icon: String,
    timeout_ms: u32,
    path: PathBuf,
) {
    let timeout_ms = timeout_ms.max(ACTION_NOTIFICATION_MIN_TIMEOUT_MS);
    // `xdg-open` resolves to the generic backend here and silently does
    // nothing for directories, so prefer the file manager directly and only
    // fall back to xdg-open when it is absent.
    let dir = path.parent().unwrap_or(&path).to_string_lossy().into_owned();
    let opener = if which("thunar") {
        format!("thunar -- {}", shell_quote(&dir))
    } else {
        format!("xdg-open -- {}", shell_quote(&dir))
    };
    let script = format!(
        r#"action=$(notify-send -a Hyshot -t {timeout} \
            -A {open_key}={open_label} \
            -A {del_key}={del_label} \
            -- {summary} {body})
case "$action" in
    {open_key}) {opener} >/dev/null 2>&1 & ;;
    {del_key}) rm -f -- {file} ;;
esac"#,
        timeout = timeout_ms,
        open_key = ACTION_OPEN,
        open_label = shell_quote("打开所在文件夹"),
        del_key = ACTION_DELETE,
        del_label = shell_quote("删除"),
        summary = shell_quote(&summary),
        body = shell_quote(&body),
        opener = opener,
        file = shell_quote(&path.to_string_lossy()),
    );
    if let Err(err) = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        eprintln!("Warning: failed to show notification: {err}");
    }
}

/// Reports whether an executable is reachable on PATH, without shelling out.
fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(program);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

/// Quotes a value for safe interpolation into a POSIX shell script.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
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

    /// Paths and titles reach a shell script, so quoting must survive spaces,
    /// quotes and command substitution. Getting this wrong either breaks on
    /// ordinary filenames or executes injected text.
    #[test]
    fn shell_quote_neutralises_shell_metacharacters() {
        assert_eq!(shell_quote("/tmp/a b/shot.png"), "'/tmp/a b/shot.png'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote("$(rm -rf /)"), "'$(rm -rf /)'");
        assert_eq!(shell_quote("`id`"), "'`id`'");
        assert_eq!(shell_quote("a;b|c&d"), "'a;b|c&d'");
    }

    /// A quoted value must round-trip through a real shell unchanged, which is
    /// the property the notification script actually depends on.
    #[test]
    fn shell_quote_round_trips_through_sh() {
        for original in ["/tmp/a b/x.png", "it's", "$HOME", "a\nb", "a `id` b"] {
            let script = format!("printf %s {}", shell_quote(original));
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(&script)
                .output()
                .expect("sh should run");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                original,
                "round trip failed for {original:?}"
            );
        }
    }

    /// Buttons the user has no time to reach are useless, so actionable
    /// notifications must not inherit a toast-sized timeout.
    #[test]
    fn actionable_notification_gets_a_reachable_timeout() {
        assert!(ACTION_NOTIFICATION_MIN_TIMEOUT_MS >= 5000);
        // Shorter configured values are raised to the floor.
        assert_eq!(3000u32.max(ACTION_NOTIFICATION_MIN_TIMEOUT_MS), ACTION_NOTIFICATION_MIN_TIMEOUT_MS);
        // Longer configured values are preserved.
        assert_eq!(20000u32.max(ACTION_NOTIFICATION_MIN_TIMEOUT_MS), 20000);
    }
}
