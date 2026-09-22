#![feature(oneshot_channel)]

mod platform;
mod ui;

mod annotator;
mod context;
mod global;

mod export;
mod image_save;

mod clipboard;
mod config;
mod egui_off_screen_render;
mod notify;
mod texture;

use crate::platform::application::Application;
use crate::platform::window::WindowConfiguration;
use crate::ui::layout::{build_annotator, initial_window_size_for_image};
use anyhow::{Context, Result};
use hyshot_core::{ImageDocument, ImageSource, MAX_IMAGE_BYTES};
use std::path::PathBuf;

pub use config::{EditorPreferences, EditorToolState, ToolSettings};

/// Input pixels never need to be written to a temporary file.
pub enum EditorInput {
    Files(Vec<PathBuf>),
    Document(ImageDocument),
}

pub struct EditorOptions {
    pub preferences: EditorPreferences,
    pub tool_state: EditorToolState,
    pub output_directory: PathBuf,
    pub notifications: bool,
    pub notification_timeout: u32,
}

impl EditorInput {
    fn decode(self) -> Result<Vec<ImageDocument>> {
        match self {
            Self::Document(document) => Ok(vec![document]),
            Self::Files(mut paths) => {
                if paths.is_empty() {
                    paths = rfd::FileDialog::new()
                        .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp", "gif"])
                        .pick_files()
                        .unwrap_or_default();
                }
                let mut remaining = MAX_IMAGE_BYTES;
                let mut documents = Vec::new();
                for path in paths {
                    let document = ImageDocument::from_file_with_budget(&path, remaining)
                        .with_context(|| format!("Cannot load image {}", path.display()))?;
                    remaining -= document.pixels().as_raw().len() as u64;
                    documents.push(document);
                }
                Ok(documents)
            }
        }
    }
}

pub fn run(input: EditorInput, options: EditorOptions) -> Result<EditorToolState> {
    let images = input.decode()?;

    if images.is_empty() {
        return Ok(options.tool_state);
    }

    let session = config::EditorSession::new(config::EditorSettings::from_parts(
        options.preferences,
        options.tool_state,
    ));
    let mut app = Application::new(
        "site.nullable.annotator",
        true,
        session.clone(),
        options.output_directory,
        options.notifications,
        options.notification_timeout,
    )?;

    for document in images {
        let image = document.pixels();
        let pixel_size = document.pixel_size();
        let (screen_w, screen_h) = app.screen_size();
        let screen_scale = app.screen_scale_factor();
        let window_size = initial_window_size_for_image(
            image.width(),
            image.height(),
            screen_w,
            screen_h,
            screen_scale,
        );

        let window_config = WindowConfiguration {
            app_id: app.app_id.to_owned(),
            title: match document.source() {
                ImageSource::Capture(_) => "Hyshot".to_owned(),
                ImageSource::File(path) | ImageSource::Stitched(path) => {
                    format!("Hyshot - {}", path.display())
                }
            },
            size: window_size,
            preferred_size: None,
        };

        app.open_window(
            window_config,
            Box::new(move |input, egui_ctx, app, window, current_view| {
                build_annotator(
                    input,
                    egui_ctx,
                    app,
                    window,
                    document.pixels().clone(),
                    current_view,
                )
            }),
        );
        // Windows initialize the device, but image upload starts in the event loop.
        let gpu = app.global_state.gpu.borrow();
        let limits = gpu
            .as_ref()
            .context("GPU was not initialized")?
            .device
            .limits();
        validate_texture_size(pixel_size, limits.max_texture_dimension_2d)?;
        let readback_bytes =
            (u64::from(pixel_size.0) * 4).div_ceil(256) * 256 * u64::from(pixel_size.1);
        anyhow::ensure!(
            readback_bytes <= limits.max_buffer_size,
            "Image exceeds GPU export-buffer limit; original image is not resized"
        );
    }

    app.run()?;
    Ok(session.settings().tool_state())
}

fn validate_texture_size((width, height): (u32, u32), maximum: u32) -> Result<()> {
    anyhow::ensure!(
        width <= maximum && height <= maximum,
        "Image {}x{} exceeds this GPU's {}-pixel texture limit; original image is not resized",
        width,
        height,
        maximum
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_input_preserves_dimensions_and_rgba() {
        let source =
            image::RgbaImage::from_fn(7, 3, |x, y| image::Rgba([x as u8, y as u8, 200, 112]));
        let bytes = crate::clipboard::to_png_bytes(&source).unwrap();
        let document = ImageDocument::from_png(
            &bytes,
            ImageSource::Capture(hyshot_core::LogicalRect::new(-7, 3, 7, 3).unwrap()),
        )
        .unwrap();
        let shared = document.pixels().clone();
        let mut decoded = EditorInput::Document(document).decode().unwrap();
        let decoded = decoded.pop().unwrap();
        assert_eq!(decoded.pixels().as_ref(), &source);
        assert!(std::sync::Arc::ptr_eq(decoded.pixels(), &shared));
    }

    #[test]
    fn invalid_image_fails_before_wayland_initialization() {
        assert!(
            ImageDocument::from_png(b"not a png", ImageSource::File("bad.png".into())).is_err()
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.png");
        let error = EditorInput::Files(vec![path]).decode().unwrap_err();
        assert!(error.to_string().contains("missing.png"));
    }

    #[test]
    fn file_input_preserves_pixels() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image with spaces.png");
        let source = image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255]));
        source.save(&path).unwrap();
        let decoded = EditorInput::Files(vec![path]).decode().unwrap();
        assert_eq!(decoded[0].pixels().as_ref(), &source);
    }

    #[test]
    fn oversized_texture_is_rejected_without_resizing() {
        assert!(validate_texture_size((1920, 16384), 16384).is_ok());
        assert!(validate_texture_size((1920, 16385), 16384).is_err());
    }
}
