use crate::annotator::drop_down_box::{
    STROKE_TYPE_SELECTOR_WIDTH, STROKE_WIDTH_SELECTOR_WIDTH, create_stroke_type_selector,
    create_stroke_width_selector,
};
use crate::annotator::{
    AnnotatorState, FillColorSupport, FontColorSupport, SharedAnnotatorState, StrokeColorSupport,
    StrokeTypeSupport, StrokeWidthSupport, ToolName,
};
use crate::global::ReadGlobal;
use crate::platform::application::Application;
use crate::platform::dpi::{LogicalPosition, LogicalSize};
use crate::platform::view::{View, ViewId};
use crate::platform::window::AppWindow;
use egui::{Button, Color32, Frame, Id, Response, Sense, Stroke, StrokeKind, Ui, Widget, vec2};
use std::any::TypeId;

pub fn current_tool_has_secondary_options(annotator_state: &AnnotatorState) -> bool {
    annotator_state
        .current_annotation_tool
        .as_ref()
        .map(|tool| {
            tool.supports_set_stroke_type()
                || tool.supports_set_stroke_color()
                || tool.supports_set_fill_color()
                || tool.supports_set_font_color()
                || tool.supports_set_stroke_width()
        })
        .unwrap_or(false)
}

const MARKER_MODE_SELECTOR_WIDTH: f32 = 62.0;

pub(crate) fn preferred_width(state: &AnnotatorState) -> u32 {
    if !current_tool_has_secondary_options(state) {
        return 0;
    }
    let Some(tool) = state.current_annotation_tool.as_ref() else {
        return 0;
    };
    (secondary_options_width(
        state,
        tool.supports_set_stroke_type(),
        tool.supports_set_stroke_width(),
        tool.supports_set_stroke_color() && tool.supports_set_fill_color(),
        tool.tool_name() == ToolName::MarkerPen,
        tool.supports_set_stroke_color()
            || tool.supports_set_fill_color()
            || tool.supports_set_font_color(),
        6.0,
    ) + 24.0)
        .ceil() as u32
}

fn create_marker_mode_selector(annotator_state: &mut AnnotatorState, ui: &mut Ui) {
    let straight = annotator_state.marker_pen_straight_mode;
    if ui
        .add_sized(
            [28.0, 20.0],
            Button::new("-").selected(straight).frame(true),
        )
        .on_hover_text("直线高亮")
        .clicked()
    {
        annotator_state.marker_pen_straight_mode = true;
    }
    if ui
        .add_sized(
            [28.0, 20.0],
            Button::new("~").selected(!straight).frame(true),
        )
        .on_hover_text("自由高亮")
        .clicked()
    {
        annotator_state.marker_pen_straight_mode = false;
    }
}

fn run_ui<F>(
    app: &mut Application,
    window: &mut AppWindow,
    current_view: &mut dyn View,
    ui: &mut Ui,
    func: F,
) where
    F: Fn(&mut Application, &mut AppWindow, &mut dyn View, &mut Ui, &mut AnnotatorState),
{
    let annotator_state = window
        .window_context
        .globals_by_type
        .take::<SharedAnnotatorState>()
        .unwrap();
    let mut annotator_state_mut_ref = annotator_state.borrow_mut();
    func(app, window, current_view, ui, &mut annotator_state_mut_ref);
    annotator_state_mut_ref.save_current_config();
    drop(annotator_state_mut_ref);
    window
        .window_context
        .globals_by_type
        .insert(TypeId::of::<SharedAnnotatorState>(), annotator_state);
}

pub fn create_secondly_toolbar(
    view_id: ViewId,
    app: &mut Application,
    window: &mut AppWindow,
    toolbar_size: LogicalSize<u32>,
    toolbar_positon: LogicalPosition<i32>,
) {
    let global_state = &app.global_state;

    window.create_sub_surface_view(
        view_id,
        global_state,
        toolbar_size,
        toolbar_positon,
        Box::new(|input, egui_ctx, app, window, current_view| {
            egui_ctx.run_ui(input, move |ctx| {
                egui::CentralPanel::default()
                    .frame(Frame::new().fill(Color32::TRANSPARENT))
                    .show(ctx, |ui| {
                        crate::ui::toolbar_style::configure(ui);
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Default);

                        {
                            let annotator_state = window
                                .window_context
                                .globals_by_type
                                .require_ref::<SharedAnnotatorState>()
                                .borrow();
                            let should_show = annotator_state.toolbar_visible
                                && current_tool_has_secondary_options(&annotator_state);

                            if !should_show {
                                current_view.set_visible(false);
                                return;
                            };
                        }

                        crate::ui::toolbar_style::frame(true).show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                            ui.horizontal_centered(|ui| {
                                run_ui(
                                    app,
                                    window,
                                    current_view,
                                    ui,
                                    move |app, window, current_view, ui, annotator_state| {
                                        let active_tool = annotator_state
                                            .current_annotation_tool
                                            .as_ref()
                                            .unwrap();

                                        let supports_set_stroke_type =
                                            active_tool.supports_set_stroke_type();
                                        let supports_set_stroke_width =
                                            active_tool.supports_set_stroke_width();
                                        let supports_set_stroke_color =
                                            active_tool.supports_set_stroke_color();
                                        let supports_set_fill_color =
                                            active_tool.supports_set_fill_color();
                                        let supports_set_font_color =
                                            active_tool.supports_set_font_color();
                                        let show_fill_mode_selector =
                                            supports_set_stroke_color && supports_set_fill_color;
                                        let show_marker_mode_selector =
                                            active_tool.tool_name() == ToolName::MarkerPen;

                                        let controls_width = secondary_options_width(
                                            annotator_state,
                                            supports_set_stroke_type,
                                            supports_set_stroke_width,
                                            show_fill_mode_selector,
                                            show_marker_mode_selector,
                                            supports_set_stroke_color
                                                || supports_set_fill_color
                                                || supports_set_font_color,
                                            ui.spacing().item_spacing.x,
                                        );
                                        let leading_space =
                                            ((ui.available_width() - controls_width) / 2.0)
                                                .max(0.0);
                                        ui.add_space(leading_space);

                                        if show_marker_mode_selector {
                                            create_marker_mode_selector(annotator_state, ui);
                                        }

                                        if supports_set_stroke_type {
                                            create_stroke_type_selector(
                                                Id::from("stroke-type-selector"),
                                                app,
                                                window,
                                                current_view,
                                                annotator_state,
                                                ui,
                                            );
                                        }

                                        // 笔触宽度下拉
                                        if supports_set_stroke_width {
                                            create_stroke_width_selector(
                                                Id::from("stroke-width-selector"),
                                                app,
                                                window,
                                                current_view,
                                                annotator_state,
                                                ui,
                                            );
                                        }

                                        // 空心/实心切换按钮
                                        if show_fill_mode_selector {
                                            create_fill_mode_selector(annotator_state, ui);
                                        }

                                        let show_color_selector = supports_set_stroke_color
                                            || supports_set_fill_color
                                            || supports_set_font_color;

                                        if show_color_selector {
                                            create_color_selector(annotator_state, ui);
                                        }
                                    },
                                );
                            });
                        });
                    });
            })
        }),
        None,
    );
}

// ─── 空心 / 实心切换选择器 ───────────────────────────────────────────────────

pub const FILL_MODE_SELECTOR_WIDTH: u32 = 32;

fn create_fill_mode_selector(annotator_state: &mut AnnotatorState, ui: &mut Ui) {
    let tool = annotator_state.current_annotation_tool.as_mut().unwrap();
    let is_solid = tool.fill_color().is_some_and(|c| c.a() > 0);

    if ui
        .add(FillModeButton::new(
            FILL_MODE_SELECTOR_WIDTH as f32,
            22.0,
            is_solid,
        ))
        .on_hover_text(if is_solid {
            "实心；点击切换为空心"
        } else {
            "空心；点击切换为实心"
        })
        .clicked()
    {
        let tool = annotator_state.current_annotation_tool.as_mut().unwrap();
        if is_solid {
            tool.set_fill_color(Color32::TRANSPARENT);
        } else {
            let stroke_c = tool.stroke_color();
            tool.set_fill_color(stroke_c);
        }
        sync_active_annotation(annotator_state);
        ui.ctx().request_repaint();
    }
}

struct FillModeButton {
    width: f32,
    height: f32,
    is_solid: bool,
}

impl FillModeButton {
    fn new(width: f32, height: f32, is_solid: bool) -> Self {
        Self {
            width,
            height,
            is_solid,
        }
    }
}

impl Widget for FillModeButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(vec2(self.width, self.height), Sense::click());

        let draw_rect = rect;
        let radius = 5.0;
        let bg = if response.hovered() {
            crate::ui::toolbar_style::HOVER
        } else {
            crate::ui::toolbar_style::SURFACE
        };

        ui.painter().rect(
            draw_rect,
            radius,
            bg,
            Stroke::new(1.0_f32, crate::ui::toolbar_style::BORDER),
            StrokeKind::Inside,
        );

        let center = draw_rect.center();
        let outer_rect = egui::Rect::from_center_size(center, vec2(14.0, 14.0));

        if self.is_solid {
            ui.painter().rect_filled(outer_rect, 2.5, Color32::WHITE);
        } else {
            ui.painter().rect(
                outer_rect,
                2.5,
                Color32::TRANSPARENT,
                Stroke::new(1.5_f32, Color32::WHITE),
                StrokeKind::Inside,
            );
        }

        response
    }
}

// ─── 颜色选择器 ──────────────────────────────────────────────────────────────

fn current_color(state: &AnnotatorState) -> Color32 {
    let tool = state.current_annotation_tool.as_ref().unwrap();
    if tool.supports_set_font_color() {
        tool.font_color()
    } else if tool.supports_get_stroke_color() {
        tool.stroke_color()
    } else if tool.supports_get_fill_color() {
        tool.fill_color().unwrap_or(Color32::TRANSPARENT)
    } else {
        tool.font_color()
    }
}

fn palette_color(color: Color32, marker: bool) -> Color32 {
    if marker {
        let [r, g, b, _] = color.to_srgba_unmultiplied();
        Color32::from_rgba_unmultiplied(r, g, b, 112)
    } else {
        color
    }
}

fn recolored_fill(fill: Option<Color32>, color: Color32) -> Option<Color32> {
    fill.map(|fill| if fill.a() > 0 { color } else { fill })
}

fn apply_color(state: &mut AnnotatorState, color: Color32) {
    let tool = state.current_annotation_tool.as_mut().unwrap();
    if tool.supports_set_stroke_color() {
        tool.set_stroke_color(color);
        if tool.supports_set_fill_color()
            && let Some(fill) = recolored_fill(tool.fill_color(), color)
        {
            tool.set_fill_color(fill);
        }
    } else if tool.supports_set_fill_color() {
        tool.set_fill_color(color);
    }
    if tool.supports_set_font_color() {
        tool.set_font_color(color);
    }
    sync_active_annotation(state);
}

fn sync_active_annotation(state: &mut AnnotatorState) {
    let Some(tool) = state.current_annotation_tool.as_ref() else {
        return;
    };
    let Some(annotation) = state.annotations_stack.last_mut() else {
        return;
    };
    if !annotation.was_created_by(tool) || !annotation.activation().is_active() {
        return;
    }
    if tool.supports_get_stroke_color() && annotation.supports_set_stroke_color() {
        annotation.set_stroke_color(tool.stroke_color());
    }
    if tool.supports_get_fill_color()
        && annotation.supports_set_fill_color()
        && let Some(fill) = tool.fill_color()
    {
        annotation.set_fill_color(fill);
    }
    if tool.supports_get_stroke_width() && annotation.supports_set_stroke_width() {
        annotation.set_stroke_width(tool.stroke_width());
    }
    if tool.supports_get_stroke_type() && annotation.supports_set_stroke_type() {
        annotation.set_stroke_type(tool.stroke_type());
    }
}

fn create_color_selector(state: &mut AnnotatorState, ui: &mut Ui) {
    let selected = current_color(state);
    ui.add(ColorButton::new(selected, 20.0, 20.0, true))
        .on_hover_text("当前颜色");
    let marker = state.current_annotation_tool.as_ref().unwrap().tool_name() == ToolName::MarkerPen;
    for color in state.candidate_colors.clone() {
        let applied = palette_color(color, marker);
        if ui
            .add(ColorButton::new(color, 20.0, 20.0, selected == applied))
            .clicked()
        {
            apply_color(state, applied);
        }
    }
}

fn secondary_options_width(
    annotator_state: &AnnotatorState,
    show_stroke_type_selector: bool,
    show_stroke_width_selector: bool,
    show_fill_mode_selector: bool,
    show_marker_mode_selector: bool,
    show_color_selector: bool,
    item_spacing: f32,
) -> f32 {
    let mut item_count = 0usize;
    let mut width = 0.0;

    if show_marker_mode_selector {
        width += MARKER_MODE_SELECTOR_WIDTH;
        item_count += 1;
    }

    if show_stroke_type_selector {
        width += STROKE_TYPE_SELECTOR_WIDTH as f32;
        item_count += 1;
    }

    if show_stroke_width_selector {
        width += STROKE_WIDTH_SELECTOR_WIDTH as f32;
        item_count += 1;
    }

    if show_fill_mode_selector {
        width += FILL_MODE_SELECTOR_WIDTH as f32;
        item_count += 1;
    }

    if show_color_selector {
        let color_count = annotator_state.candidate_colors.len() + 1;
        width += color_count as f32 * 20.0;
        item_count += color_count;
    }

    if item_count > 1 {
        width += (item_count - 1) as f32 * item_spacing;
    }

    width
}

// ─── ColorButton Widget ───────────────────────────────────────────────────────

struct ColorButton {
    color: Color32,
    width: f32,
    height: f32,
    checked: bool,
}

impl ColorButton {
    fn new(color: Color32, width: f32, height: f32, checked: bool) -> ColorButton {
        ColorButton {
            color,
            width,
            height,
            checked,
        }
    }
}

impl Widget for ColorButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(vec2(self.width, self.height), Sense::click());
        let draw_rect = rect;
        let radius = 4.0;
        if self.checked {
            // 选中样式：颜色球 + 2px 白色凸显边框 + 蓝色环形晕染
            ui.painter().rect(
                draw_rect,
                radius,
                self.color,
                Stroke::new(2.0_f32, Color32::WHITE),
                StrokeKind::Inside,
            );
        } else {
            let outline = if self.color == Color32::from_hex("#1c1c1e").unwrap() {
                Stroke::new(1.0_f32, Color32::from_hex("#555968").unwrap())
            } else {
                Stroke::new(1.0_f32, Color32::from_hex("#161821").unwrap())
            };
            ui.painter()
                .rect(draw_rect, radius, self.color, outline, StrokeKind::Inside);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_color_preserves_hollow_and_absent_fill() {
        assert_eq!(recolored_fill(None, Color32::RED), None);
        assert_eq!(
            recolored_fill(Some(Color32::TRANSPARENT), Color32::RED),
            Some(Color32::TRANSPARENT)
        );
        assert_eq!(
            recolored_fill(Some(Color32::BLUE), Color32::RED),
            Some(Color32::RED)
        );
    }

    #[test]
    fn marker_palette_keeps_its_alpha_and_rgb() {
        let color = Color32::from_rgb(100, 210, 255);
        let applied = palette_color(color, true);
        assert_eq!(applied.a(), 112);
        assert_eq!(applied, Color32::from_rgba_unmultiplied(100, 210, 255, 112));
        assert_eq!(palette_color(color, false), color);
    }

    #[test]
    fn marker_widths_remain_independent() {
        use crate::annotator::drop_down_box::stroke_widths_for_tool;
        assert_eq!(
            stroke_widths_for_tool(ToolName::MarkerPen),
            &[8.0, 12.0, 18.0, 24.0, 32.0]
        );
        assert_eq!(
            stroke_widths_for_tool(ToolName::Pencil),
            &[1.0, 2.0, 3.0, 5.0, 8.0]
        );
    }
}
