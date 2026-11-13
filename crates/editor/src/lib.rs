pub mod annotations;
pub mod assets;
pub mod clipboard;
pub mod config;
pub mod image_save;
pub mod notify;
pub mod renderer;
pub mod ui;

use annotations::ToolKind;
use anyhow::Result;
use assets::EditorAssets;
pub use config::{EditorPreferences, EditorToolState, ToolSettings};
use gpui::{
    AppContext, Application, Bounds, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowKind, WindowOptions, px, size,
};
use hyshot_core::ImageDocument;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use ui::editor_view::EditorView;
use ui::layout::{dock_layout, fit_image_to_screen, secondary_toolbar_width_for_tool};

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

pub fn run(input: EditorInput, options: EditorOptions) -> Result<EditorToolState> {
    let mut documents = input.decode()?;
    let Some(doc) = documents.pop() else {
        return Ok(options.tool_state);
    };

    let raw_image = doc.pixels().clone();
    let img_w = raw_image.width() as f32;
    let img_h = raw_image.height() as f32;

    let final_tool_state = Arc::new(Mutex::new(options.tool_state.clone()));
    let result_state = final_tool_state.clone();

    let app = Application::new().with_assets(EditorAssets);
    app.run(move |cx| {
        let output_dir = options.output_directory;
        let preferences = options.preferences;
        let tool_state = options.tool_state;
        let notifications = options.notifications;
        let timeout = options.notification_timeout;

        let screen_size = cx.displays().first().map(|d| {
            let b = d.bounds();
            (f32::from(b.size.width), f32::from(b.size.height))
        }).unwrap_or((1920.0, 1080.0));

        let (fitted_w, fitted_h, fit_scale) = fit_image_to_screen(img_w, img_h, screen_size.0, screen_size.1);
        let default_tool = ToolKind::from_config_value(&preferences.active_tool)
            .unwrap_or(ToolKind::Rectangle);
        let active_tool = if preferences.auto_activate_default_tool {
            Some(default_tool)
        } else {
            None
        };
        let secondary_w = secondary_toolbar_width_for_tool(active_tool);
        let dock = dock_layout(fitted_w, fitted_h, secondary_w, true);

        let screen_bounds = Bounds::centered(
            None,
            size(px(dock.window_width), px(dock.window_height)),
            cx,
        );

        let final_tool_state_ref = final_tool_state.clone();
        let _ = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(screen_bounds)),
                window_min_size: Some(size(px(dock.window_width), px(dock.window_height))),
                app_id: Some("site.nullable.annotator".into()),
                window_background: WindowBackgroundAppearance::Transparent,
                window_decorations: Some(WindowDecorations::Client),
                kind: WindowKind::PopUp,
                is_resizable: true,
                focus: true,
                ..Default::default()
            },
            |_, cx| {
                cx.new(|cx| {
                    EditorView::new(
                        cx,
                        raw_image.clone(),
                        output_dir,
                        preferences,
                        tool_state,
                        notifications,
                        timeout,
                        fit_scale,
                        Some(final_tool_state_ref),
                    )
                })
            },
        );
        cx.activate(true);
    });

    let state = result_state.lock().unwrap().clone();
    Ok(state)
}

pub fn run_with_input(input: EditorInput, options: EditorOptions) -> Result<EditorToolState> {
    run(input, options)
}

impl EditorInput {
    pub fn decode(self) -> Result<Vec<ImageDocument>> {
        match self {
            EditorInput::Document(document) => Ok(vec![document]),
            EditorInput::Files(mut paths) => {
                if paths.is_empty() {
                    if let Some(picked) = rfd::FileDialog::new()
                        .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp", "gif"])
                        .pick_files()
                    {
                        paths = picked;
                    }
                }
                let mut documents = Vec::with_capacity(paths.len());
                for path in paths {
                    documents.push(ImageDocument::from_file(&path)?);
                }
                Ok(documents)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyshot_core::ImageSource;

    #[test]
    fn memory_input_preserves_dimensions_and_rgba() {
        let source =
            image::RgbaImage::from_fn(7, 3, |x, y| image::Rgba([x as u8, y as u8, 200, 112]));
        let mut png_bytes = Vec::new();
        source
            .write_to(
                &mut std::io::Cursor::new(&mut png_bytes),
                image::ImageFormat::Png,
            )
            .unwrap();

        let document = ImageDocument::from_png(
            &png_bytes,
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
    fn invalid_image_fails_before_initialization() {
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
}
