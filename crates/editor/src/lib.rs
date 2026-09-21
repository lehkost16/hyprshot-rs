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
use std::path::PathBuf;
use std::sync::Arc;

pub use config::{EditorSettings, ToolSettings};

/// Input pixels never need to be written to a temporary file.
pub enum EditorInput {
    Files(Vec<PathBuf>),
    Png(Vec<u8>),
}

pub struct EditorOptions {
    pub settings: EditorSettings,
    pub output_directory: PathBuf,
    pub notifications: bool,
    pub notification_timeout: u32,
}

impl EditorInput {
    fn decode(self) -> Result<Vec<image::RgbaImage>> {
        match self {
            Self::Png(bytes) => Ok(vec![
                image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                    .context("Failed to decode captured PNG")?
                    .into_rgba8(),
            ]),
            Self::Files(mut paths) => {
                if paths.is_empty() {
                    paths = rfd::FileDialog::new()
                        .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp", "gif"])
                        .pick_files()
                        .unwrap_or_default();
                }
                paths
                    .into_iter()
                    .map(|path| {
                        image::open(&path)
                            .with_context(|| format!("Failed to open image {}", path.display()))
                            .map(image::DynamicImage::into_rgba8)
                    })
                    .collect()
            }
        }
    }
}

pub fn run(input: EditorInput, options: EditorOptions) -> Result<EditorSettings> {
    let images = input.decode()?;

    if images.is_empty() {
        return Ok(options.settings);
    }

    let session = config::EditorSession::new(options.settings);
    let mut app = Application::new(
        "site.nullable.annotator",
        true,
        session.clone(),
        options.output_directory,
        options.notifications,
        options.notification_timeout,
    )?;

    for image in images {
        let image = Arc::new(image);
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
            title: "Hyshot".to_owned(),
            size: window_size,
            preferred_size: None,
        };

        app.open_window(
            window_config,
            Box::new(move |input, egui_ctx, app, window, current_view| {
                build_annotator(input, egui_ctx, app, window, image.clone(), current_view)
            }),
        );
    }

    app.run()?;
    Ok(session.settings())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_input_preserves_dimensions_and_rgba() {
        let source =
            image::RgbaImage::from_fn(7, 3, |x, y| image::Rgba([x as u8, y as u8, 200, 112]));
        let bytes = crate::clipboard::to_png_bytes(&source).unwrap();
        let mut decoded = EditorInput::Png(bytes).decode().unwrap();
        assert_eq!(decoded.pop().unwrap(), source);
    }

    #[test]
    fn invalid_image_fails_before_wayland_initialization() {
        assert!(EditorInput::Png(b"not a png".to_vec()).decode().is_err());
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
        assert_eq!(decoded[0], source);
    }
}
