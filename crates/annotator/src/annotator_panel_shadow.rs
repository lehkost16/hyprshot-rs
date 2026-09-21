use crate::application::Application;
use crate::dpi::{LogicalPosition, LogicalSize};
use crate::view::ViewId;
use crate::window::AppWindow;
use egui::{Color32, Frame};

pub fn create_annotator_panel_shadow(
    view_id: ViewId,
    app: &mut Application,
    window: &mut AppWindow,
    logical_size: LogicalSize<u32>,
    logical_position: LogicalPosition<i32>,
) {
    let global_state = &app.global_state;
    window.create_sub_surface_view(
        view_id,
        global_state,
        logical_size,
        logical_position,
        Box::new(move |input, egui_ctx, _app, _window, _current_view| {
            // 构建 UI 的具体内容
            egui_ctx.run_ui(input, move |ctx| {
                egui::CentralPanel::default()
                    .frame(Frame::new().fill(Color32::TRANSPARENT))
                    .show(ctx, |_ui| {});
            })
        }),
        None,
    );
}
