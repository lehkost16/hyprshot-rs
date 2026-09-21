use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Clone, Copy)]
pub enum ExportAction {
    Copy,
    Save,
}

/// Both shortcuts and toolbar actions use the same success/close policy.
pub(crate) fn export(
    image: &RgbaImage,
    action: ExportAction,
    directory: &Path,
    notifications: bool,
    notification_timeout: u32,
) -> Result<()> {
    let bytes = crate::clipboard::to_png_bytes(image).map_err(anyhow::Error::msg)?;
    match action {
        ExportAction::Save => {
            let path = crate::image_save::save_png(&bytes, directory)?;
            if notifications {
                crate::notify::saved(path.display(), notification_timeout);
            }
        }
        ExportAction::Copy => {
            let mut child = Command::new("wl-copy")
                .args(["--type", "image/png"])
                .stdin(Stdio::piped())
                .spawn()
                .context("Failed to start wl-copy")?;
            let written = child
                .stdin
                .take()
                .context("Missing clipboard stdin")?
                .write_all(&bytes);
            let status = child.wait().context("Failed to wait for wl-copy")?;
            written.context("Failed to write clipboard image")?;
            ensure!(status.success(), "wl-copy exited with {status}");
            if notifications {
                crate::notify::copied(notification_timeout);
            }
        }
    }
    Ok(())
}
