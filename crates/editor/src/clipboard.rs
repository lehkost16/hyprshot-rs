use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use std::io::Write;
use std::process::{Command, Stdio};

pub fn copy_to_clipboard(image: &RgbaImage) -> Result<()> {
    let mut png_bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .context("Failed to encode PNG for clipboard")?;

    let mut child = Command::new("wl-copy")
        .args(["--type", "image/png"])
        .stdin(Stdio::piped())
        .spawn()
        .context("Failed to spawn wl-copy")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&png_bytes).context("Failed to write to wl-copy")?;
    }
    let status = child.wait().context("Failed to wait for wl-copy")?;
    ensure!(status.success(), "wl-copy exited with status {}", status);
    Ok(())
}
