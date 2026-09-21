#![feature(oneshot_channel)]

mod application;
mod egui_input;
mod gpu;
mod window;

mod wp_fractional_scaling;

mod annotator;
mod context;
mod dpi;
mod font;
mod global;
mod icon;

mod image_save;

mod annotator_panel;
mod annotator_panel_shadow;
mod clipboard;
mod config;
mod egui_off_screen_render;
mod layout;
mod notify;
mod primary_toolbar;
mod secondly_toolbar;
mod serial;
mod texture;
mod toolbar_style;
mod view;
mod wp_viewporter;

use crate::application::Application;
use crate::layout::{build_annotator, initial_window_size_for_image};
use crate::window::WindowConfiguration;
use anyhow::{Context, Result};
use std::sync::Arc;

pub fn open_images(mut paths: Vec<std::ffi::OsString>) -> Result<()> {
    let _ = env_logger::try_init();

    if paths.is_empty()
        && let Some(file_path) = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp", "gif"])
            .pick_file()
    {
        paths.push(file_path.into());
    }

    if paths.is_empty() {
        return Ok(());
    }

    let mut app = Application::new("site.nullable.annotator", true);

    for path in paths {
        let image = image::open(&path)
            .with_context(|| format!("Failed to open image {}", path.to_string_lossy()))?;

        let image = Arc::new(image.to_rgba8());
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
            title: "Annotator".to_owned(),
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

    app.run();
    Ok(())
}
