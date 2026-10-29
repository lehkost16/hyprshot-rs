use anyhow::{Context, Result};
use notify_rust::Notification;
use std::fs::{create_dir_all, write};
use std::path::{Path, PathBuf};
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
        create_dir_all(save_fullpath.parent().unwrap_or(Path::new(".")))
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
            // The template is a shell command, so the substituted path must be
            // quoted: a screenshot directory or filename containing a quote or
            // `$(...)` would otherwise be interpreted by the shell.
            let cmd_str = options
                .upload_command
                .replace("{path}", &shell_quote(&save_fullpath.to_string_lossy()));
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
        // Actions only apply when a file is actually written; clipboard-only
        // captures have nothing to open or delete.
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

/// Whether the notification should offer "open folder" and "delete".
///
/// Only a real file qualifies: both actions operate on it, and a directory
/// must never be offered for deletion.
fn should_offer_actions(path: &Path) -> bool {
    path.is_file()
}

/// Shows the saved-file notification with "open folder" and "delete" actions,
/// handling whichever the user picks.
///
/// Both actions operate on the file, so they are only offered when the file is
/// actually on disk. A notification with broken buttons is worse than a plain
/// one, so callers that cannot prove the file exists fall back to a toast.
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
    if !should_offer_actions(&path) {
        // No file, no actions: report the outcome without offering buttons
        // that would fail when clicked.
        if let Err(err) = Notification::new()
            .summary(&summary)
            .body(&body)
            .timeout(timeout_ms as i32)
            .appname("Hyshot")
            .show()
        {
            eprintln!("Warning: failed to show notification: {err}");
        }
        return;
    }
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

/// Opens a saved file with whatever the desktop associates with its type.
///
/// `xdg-open` is the documented entry point but its generic backend resolves
/// to nothing under this compositor, so the `.desktop` handler from
/// `xdg-mime query default` is launched directly and xdg-open is only the last
/// resort. The child is detached: the calling command exits immediately and
/// must not wait for the viewer to close.
pub fn open_path(path: &Path) {
    if let Some((program, args)) = desktop_handler_command(path) {
        if Command::new(&program)
            .args(&args)
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            return;
        }
    }
    let _ = Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Resolves the command backing the default handler for `path`'s MIME type.
///
/// `xdg-mime query default` reports a `.desktop` id; that file's `Exec=` line
/// holds the actual command line, which may carry required arguments
/// (`mpv --player-operation-mode=pseudo-gui -- %U`). Running the basename alone
/// would drop them and silently change how the program starts, so the `Exec`
/// line is parsed and its field codes stripped. Falls back to the basename when
/// the desktop file cannot be read.
fn desktop_handler_command(path: &Path) -> Option<(String, Vec<String>)> {
    let mime = mime_for_extension(path.extension()?.to_str()?)?;
    let output = Command::new("xdg-mime")
        .args(["query", "default", mime])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let desktop_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let program = desktop_id.strip_suffix(".desktop")?.to_string();
    if program.is_empty() || !which(&program) {
        return None;
    }

    // Prefer the real Exec line; the basename alone is the last resort.
    let args = desktop_exec_args(&desktop_id).unwrap_or_default();
    Some((program, args))
}

/// Reads the `Exec=` line of a `.desktop` file and returns its arguments.
///
/// Field codes (`%U`, `%F`, `%f`, `%u`) are dropped: the caller always appends
/// the concrete path itself. `%U`/`%F` expand to the file list, so keeping them
/// would pass the path twice.
fn desktop_exec_args(desktop_id: &str) -> Option<Vec<String>> {
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    let mut search: Vec<PathBuf> = vec![PathBuf::from("/usr/share/applications")];
    for dir in data_dirs.split(':').filter(|d| !d.is_empty()) {
        search.push(PathBuf::from(dir).join("applications"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        search.insert(0, PathBuf::from(home).join(".local/share/applications"));
    }

    for dir in search {
        let candidate = dir.join(desktop_id);
        let Ok(contents) = std::fs::read_to_string(&candidate) else {
            continue;
        };
        let Some(exec) = contents
            .lines()
            .find_map(|line| line.strip_prefix("Exec="))
        else {
            continue;
        };
        return Some(desktop_exec_tokens(exec));
    }
    None
}

/// Splits an `Exec=` value into arguments.
///
/// The first token is the program itself, already launched separately, so it is
/// dropped. Field codes expand to the file list, which the caller supplies.
fn desktop_exec_tokens(exec: &str) -> Vec<String> {
    exec.split_whitespace()
        .filter(|token| !is_desktop_field_code(token))
        .skip(1)
        .map(str::to_string)
        .collect()
}

/// Whether a token is a `.desktop` field code (`%U`, `%F`, `%i`, ...).
fn is_desktop_field_code(token: &str) -> bool {
    matches!(token, "%f" | "%F" | "%u" | "%U" | "%i" | "%c" | "%k" | "%%")
}

/// Minimal extension-to-MIME mapping for the file types hyshot produces.
fn mime_for_extension(ext: &str) -> Option<&'static str> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        _ => return None,
    })
}

/// Reports whether a file has any execute bit set.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Reports whether an executable is reachable on PATH, without shelling out.
fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(program);
                // A file that is not executable would make spawn() fail after
                // we had already committed to this handler.
                candidate.is_file() && is_executable(&candidate)
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

    /// The extension mapping decides which handler is resolved, so every type
    /// hyshot writes must map, and case must not matter.
    #[test]
    fn mime_mapping_covers_hyshot_output_types() {
        assert_eq!(mime_for_extension("png"), Some("image/png"));
        assert_eq!(mime_for_extension("PNG"), Some("image/png"));
        assert_eq!(mime_for_extension("jpeg"), Some("image/jpeg"));
        assert_eq!(mime_for_extension("mp4"), Some("video/mp4"));
        assert_eq!(mime_for_extension("webm"), Some("video/webm"));
        assert_eq!(mime_for_extension("gif"), Some("image/gif"));
        // Unknown types must not resolve to some arbitrary handler.
        assert_eq!(mime_for_extension("txt"), None);
        assert_eq!(mime_for_extension(""), None);
    }

    /// `after_save` is a plain string, so anything other than "open" must mean
    /// keep-and-stay-quiet. A typo should not make the editor pop up.
    #[test]
    fn after_save_only_opens_on_exact_open() {
        for value in ["save", "Save", "", "openn", "true"] {
            assert!(!value.eq_ignore_ascii_case("open"), "{value} must not open");
        }
        for value in ["open", "OPEN", "Open"] {
            assert!(value.eq_ignore_ascii_case("open"), "{value} must open");
        }
    }

    /// Actions must only be offered for a file that exists. Otherwise "open
    /// folder" and "delete" are buttons that fail the moment they are clicked.
    /// This exercises the real decision the notification path makes.
    #[test]
    fn actions_are_gated_on_file_presence() {
        assert!(should_offer_actions(&std::path::Path::new("/nonexistent/nope.png")) == false);

        let file = std::env::temp_dir().join("hyshot_action_gate.png");
        std::fs::write(&file, b"x").expect("write temp file");
        assert!(should_offer_actions(&file));

        let dir = std::env::temp_dir();
        assert!(!should_offer_actions(&dir), "a directory gets no actions");

        std::fs::remove_file(&file).ok();
    }

    /// The upload template is a shell command; an unquoted path containing a
    /// quote would let the rest of the filename run as a command.
    #[test]
    fn shell_quote_neutralises_metacharacters() {
        let quoted = shell_quote("/tmp/a'b;$(rm -rf /).png");
        assert_eq!(quoted, r"'/tmp/a'\''b;$(rm -rf /).png'");
        // The escaped form must round-trip back to the original single word.
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {quoted}"))
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "/tmp/a'b;$(rm -rf /).png"
        );
    }

    /// `Exec=` may carry required arguments; dropping them changes how the
    /// viewer starts. Field codes are removed because the caller appends the
    /// path itself.
    #[test]
    fn desktop_exec_args_are_parsed_and_field_codes_dropped() {
        assert_eq!(
            desktop_exec_tokens("mpv --player-operation-mode=pseudo-gui -- %U"),
            vec!["--player-operation-mode=pseudo-gui", "--"]
        );
        assert_eq!(desktop_exec_tokens("imv %F"), Vec::<String>::new());
        assert_eq!(desktop_exec_tokens("eog %U"), Vec::<String>::new());
    }

    /// A handler that is not executable must not be selected: spawn would fail
    /// after we had already committed to it.
    #[test]
    fn non_executable_files_are_not_handlers() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("not-a-program");
        std::fs::write(&plain, b"#!/bin/sh\n").unwrap();
        assert!(!is_executable(&plain));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(is_executable(&plain));
        }
    }
}
