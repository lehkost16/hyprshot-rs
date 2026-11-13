use gpui::AppContext;
use hyshot_editor::assets::EditorAssets;
use hyshot_editor::ui::editor_view::EditorView;
use hyshot_editor::{EditorPreferences, EditorToolState};
use image::RgbaImage;
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    let app = gpui::Application::new().with_assets(EditorAssets);
    app.run(|cx| {
        let base_image = Arc::new(RgbaImage::new(800, 600));
        let _ = cx.open_window(
            gpui::WindowOptions::default(),
            |_, cx| {
                cx.new(|cx| {
                    EditorView::new(
                        cx,
                        base_image,
                        PathBuf::from("/tmp"),
                        EditorPreferences::default(),
                        EditorToolState::default(),
                        false,
                        2000,
                        1.0,
                        None,
                    )
                })
            },
        );
    });
}
