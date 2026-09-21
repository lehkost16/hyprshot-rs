use crate::annotator::svg_button::SvgButton;
use crate::annotator::{AnnotatorState, ApplyExtraZoomFactor, SharedAnnotatorState, ToolName};
use crate::context::Command;
use crate::global::ReadGlobalMut;
use crate::platform::application::Application;
use crate::platform::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
use crate::platform::view::{View, ViewId};
use crate::platform::window::AppWindow;
use crate::ui::icon::Icons;
use egui::{Color32, Frame, vec2};

pub fn create_primary_toolbar(
    view_id: ViewId,
    app: &mut Application,
    window: &mut AppWindow,
    toolbar_size: LogicalSize<u32>,
    toolbar_position: LogicalPosition<i32>,
) {
    window.create_sub_surface_view(
        view_id,
        &app.global_state,
        toolbar_size,
        toolbar_position,
        Box::new(|input, egui_ctx, _app, window, current_view| {
            egui_ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default()
                    .frame(Frame::new().fill(Color32::TRANSPARENT))
                    .show(ctx, |ui| {
                        crate::ui::toolbar_style::configure(ui);
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                        crate::ui::toolbar_style::frame(false).show(ui, |ui| {
                            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                            let shared = window
                                .window_context
                                .globals_by_type
                                .require_ref_mut::<SharedAnnotatorState>()
                                .clone();
                            ui.horizontal_centered(|ui| {
                                let mut state = shared.borrow_mut();
                                if state.toolbar_visible {
                                    primary_controls(ui, window, current_view, &mut state);
                                }
                            });
                        });
                    });
            })
        }),
        None,
    );
}

pub(crate) fn primary_controls(
    ui: &mut egui::Ui,
    window: &mut AppWindow,
    _current_view: &mut dyn View,
    state: &mut AnnotatorState,
) {
    let active_tool = state
        .current_annotation_tool
        .as_ref()
        .map(|tool| tool.tool_name());

    if ui
        .add(SvgButton::new(
            "rectangle-tool".into(),
            Icons::DrawRectangle.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::Rectangle)),
        ))
        .on_hover_text("矩形")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::Rectangle)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::Rectangle);
        }
    }

    if ui
        .add(SvgButton::new(
            "ellipse-tool".into(),
            Icons::DrawEllipse.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::Ellipse)),
        ))
        .on_hover_text("椭圆")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::Ellipse)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::Ellipse);
        }
    }

    if ui
        .add(SvgButton::new(
            "straight-line-tool".into(),
            Icons::DrawLine.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::StraightLine)),
        ))
        .on_hover_text("直线")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::StraightLine)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::StraightLine);
        }
    }
    if ui
        .add(SvgButton::new(
            "arrow-tool".into(),
            Icons::DrawArrow.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::Arrow)),
        ))
        .on_hover_text("箭头")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::Arrow)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::Arrow);
        }
    }

    if ui
        .add(SvgButton::new(
            "pencil-tool".into(),
            Icons::DrawFreehand.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::Pencil)),
        ))
        .on_hover_text("铅笔")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::Pencil)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::Pencil);
        }
    }

    if ui
        .add(SvgButton::new(
            "marker-pen-tool".into(),
            Icons::DrawHighlight.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::MarkerPen)),
        ))
        .on_hover_text("荧光笔")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::MarkerPen)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::MarkerPen);
        }
    }

    if ui
        .add(SvgButton::new(
            "mosaic-tool".into(),
            Icons::PixelArtTrace.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::Mosaic)),
        ))
        .on_hover_text("马赛克")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::Mosaic)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::Mosaic);
        }
    }

    if ui
        .add(SvgButton::new(
            "blur-tool".into(),
            Icons::BlurFx.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::Blur)),
        ))
        .on_hover_text("模糊")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::Blur)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::Blur);
        }
    }

    if ui
        .add(SvgButton::new(
            "text-tool".into(),
            Icons::DrawText.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::Text)),
        ))
        .on_hover_text("文字")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::Text)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::Text);
        }
    }

    if ui
        .add(SvgButton::new(
            "serial-number-tool".into(),
            Icons::DrawNumber.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::SerialNumber)),
        ))
        .on_hover_text("序号")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::SerialNumber)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::SerialNumber);
        }
    }

    if ui
        .add(SvgButton::new(
            "eraser-tool".into(),
            Icons::DrawEraser.get_image(),
            LogicalSize::new(28., 28.),
            false,
            true,
            matches!(active_tool, Some(ToolName::Eraser)),
        ))
        .on_hover_text("橡皮擦")
        .clicked()
    {
        if matches!(active_tool, Some(ToolName::Eraser)) {
            state.deactivate_annotation_tool();
        } else {
            state.activate_annotation_tool(ToolName::Eraser);
        }
    }

    if ui
        .add(SvgButton::new(
            "undo-tool".into(),
            Icons::EditUndo.get_image(),
            LogicalSize::new(28., 28.),
            !state.can_undo(),
            false,
            false,
        ))
        .on_hover_text("撤销 (Ctrl+Z)")
        .clicked()
    {
        state.undo();
    }

    if ui
        .add(SvgButton::new(
            "redo-tool".into(),
            Icons::EditRedo.get_image(),
            LogicalSize::new(28., 28.),
            !state.can_redo(),
            false,
            false,
        ))
        .on_hover_text("重做 (Ctrl+Y)")
        .clicked()
    {
        state.redo();
    }

    if ui
        .add(SvgButton::new(
            "reset-zoom-tool".into(),
            Icons::ZoomOriginal.get_image(),
            LogicalSize::new(28., 28.),
            false,
            false,
            false,
        ))
        .on_hover_text("恢复初始缩放")
        .clicked()
    {
        let reset_zoom = state.initial_zoom_factor;
        state.extra_zoom_factor = reset_zoom;
        let image_w = state.background_image.width();
        let image_h = state.background_image.height();
        let new_size = PhysicalSize::new(image_w, image_h)
            .to_logical(ui.ctx().pixels_per_point() as f64)
            .apply_extra_zoom_factor(reset_zoom);

        window
            .window_context
            .commands
            .push_back(Command::ResizeView(
                crate::annotator::AnnotatorState::annotator_panel_id(),
                new_size,
            ));
        ui.ctx().request_repaint();
    }

    if ui
        .add(SvgButton::new(
            "save-tool".into(),
            Icons::DocumentSave.get_image(),
            LogicalSize::new(28., 28.),
            false,
            false,
            false,
        ))
        .on_hover_text("保存")
        .clicked()
    {
        let image_receiver = state.take_screenshot(ui.pixels_per_point());
        window
            .window_context
            .commands
            .push_back(Command::ExportImage(
                image_receiver,
                crate::export::ExportAction::Save,
            ));
    }

    if ui
        .add(SvgButton::new(
            "copy-tool".into(),
            Icons::EditCopy.get_image(),
            LogicalSize::new(28., 28.),
            false,
            false,
            false,
        ))
        .on_hover_text("复制")
        .clicked()
    {
        let image_receiver = state.take_screenshot(ui.pixels_per_point());
        window
            .window_context
            .commands
            .push_back(Command::ExportImage(
                image_receiver,
                crate::export::ExportAction::Copy,
            ));
    }
    let tool_name = state
        .current_annotation_tool
        .as_ref()
        .map(|t| t.tool_name());
    if active_tool != tool_name {
        // 将栈顶的标注更新为非激活状态
        if let Some(annotation) = state.annotations_stack.last_mut() {
            annotation.activation_mut().deactivate()
        }
    }
}
