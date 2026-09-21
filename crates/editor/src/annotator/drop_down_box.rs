use crate::annotator::{
    AnnotatorState, SharedAnnotatorState, StrokeType, StrokeTypeSupport, StrokeWidthSupport,
};
use crate::context::Command;
use crate::global::ReadGlobal;
use crate::platform::application::Application;
use crate::platform::dpi::LogicalSize;
use crate::platform::view::View;
use crate::platform::view::xdg_popup_view::TriggerType;
use crate::platform::window::AppWindow;
use egui::{
    Color32, Id, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Widget, pos2, vec2,
};
use std::any::TypeId;
use std::ops::Add;
use std::sync::Arc;
use wayland_protocols::xdg::shell::client::xdg_positioner;

pub type BuildDropdownButtonFn = dyn Fn(&Id, &mut Ui, &mut AnnotatorState) -> Response;
pub type BuildDropdownAreaFn =
    dyn Fn(&mut Application, &mut AppWindow, &mut dyn View, &mut AnnotatorState, &mut Ui);

pub struct DropdownBox<'a, 'w, 's, 'v> {
    pub id: Id,
    pub app: &'a mut Application,
    pub window: &'w mut AppWindow,
    pub current_view: &'v mut dyn View,
    pub annotator_state: &'s mut AnnotatorState,
    pub build_drop_down_box_button_fn: Arc<Box<BuildDropdownButtonFn>>,
    pub build_drop_down_area_fn: Arc<Box<BuildDropdownAreaFn>>,
    pub drop_down_area_size: LogicalSize<u32>,
}

impl Widget for DropdownBox<'_, '_, '_, '_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let build_button_fn = self.build_drop_down_box_button_fn;
        let response = build_button_fn(&self.id, ui, self.annotator_state);
        let popup_view_id = format!("drop-down-box-area-{}", self.id.value()).into();
        if response.clicked() && !self.window.views.contains_key(&popup_view_id) {
            let build_drop_down_area_fn = self.build_drop_down_area_fn;
            let original_tool = self
                .annotator_state
                .current_annotation_tool
                .as_ref()
                .map(|t| t.tool_name());
            let parent_id = self.current_view.id();
            let parent_size = self.current_view.size();
            let parent_position = self.current_view.position();
            let parent_scale = self.current_view.scale_factor();

            let positioner = self.window.create_positioner(&self.app.global_state);

            let current_view_position = self.current_view.position().unwrap();

            let button_rect = &response.rect;

            // 弹出框的尺寸
            positioner.set_size(
                self.drop_down_area_size.width as i32,
                self.drop_down_area_size.height as i32,
            );

            // 按钮的左上角在窗口中的位置
            let button_top_left = button_rect.left_top().add(vec2(
                current_view_position.x as f32,
                current_view_position.y as f32,
            ));

            // 父表面内的锚点矩形
            positioner.set_anchor_rect(
                button_top_left.x.round() as i32,
                button_top_left.y.round() as i32,
                button_rect.width().round() as i32,
                button_rect.height().round() as i32,
            );

            // 指定锚定矩形的哪一条边或角与弹出窗口对齐
            positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
            // 弹窗相对于锚点的伸展方向
            positioner.set_gravity(xdg_positioner::Gravity::BottomLeft);
            positioner.set_offset(0, 4);
            // 空间不足时的自动调整策略
            positioner.set_constraint_adjustment(xdg_positioner::ConstraintAdjustment::all());
            self.window.create_xdg_popup_view(
                popup_view_id,
                &self.app.global_state,
                Some(TriggerType::MousePress),
                positioner,
                Box::new(move |input, egui_ctx, app, window, current_view| {
                    // 构建 UI 的具体内容
                    egui_ctx.run_ui(input, |ctx| {
                        let parent_changed = window
                            .views
                            .get(&parent_id)
                            .and_then(|v| v.as_ref())
                            .is_none_or(|v| {
                                !v.get_view_ref().visible()
                                    || v.get_view_ref().size() != parent_size
                                    || v.get_view_ref().scale_factor() != parent_scale
                                    || v.get_view_ref().position() != parent_position
                            });
                        let shared = window
                            .window_context
                            .globals_by_type
                            .require_ref::<SharedAnnotatorState>()
                            .clone();
                        let state = shared.borrow();
                        let tool_changed = state
                            .current_annotation_tool
                            .as_ref()
                            .map(|t| t.tool_name())
                            != original_tool;
                        if parent_changed
                            || !state.toolbar_visible
                            || tool_changed
                            || ctx.input_mut(|i| {
                                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            })
                        {
                            current_view.close_later();
                            return;
                        }
                        drop(state);
                        egui::CentralPanel::default()
                            .frame(crate::ui::toolbar_style::frame(true))
                            .show(ctx, |ui| {
                                crate::ui::toolbar_style::configure(ui);
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                                ui.spacing_mut().item_spacing = vec2(1.0, 0.0);

                                let annotator_state = window
                                    .window_context
                                    .globals_by_type
                                    .take::<SharedAnnotatorState>()
                                    .unwrap();

                                build_drop_down_area_fn(
                                    app,
                                    window,
                                    current_view,
                                    &mut annotator_state.borrow_mut(),
                                    ui,
                                );
                                annotator_state.borrow_mut().save_current_config();

                                window
                                    .window_context
                                    .globals_by_type
                                    .insert(TypeId::of::<SharedAnnotatorState>(), annotator_state);
                            });
                    })
                }),
            );
        } else if response.clicked() {
            self.window
                .window_context
                .commands
                .push_back(Command::DropView(popup_view_id));
        }
        response
    }
}

pub(crate) const STROKE_TYPE_SELECTOR_WIDTH: u32 = 100u32;

pub fn create_stroke_type_selector(
    id: Id,
    app: &mut Application,
    window: &mut AppWindow,
    current_view: &mut dyn View,
    annotator_state: &mut AnnotatorState,
    ui: &mut Ui,
) {
    let dropdown = DropdownBox {
        id,
        app,
        window,
        current_view,
        annotator_state,
        drop_down_area_size: LogicalSize::new(114, 120),
        build_drop_down_box_button_fn: Arc::new(Box::new(|_id, ui, annotator_state| {
            let tool = annotator_state.current_annotation_tool.as_ref().unwrap();
            let stroke_type = tool.stroke_type();
            let (rect, response) = ui
                .allocate_exact_size(vec2(STROKE_TYPE_SELECTOR_WIDTH as f32, 20.), Sense::click());
            if response.hovered() {
                ui.painter().rect(
                    rect,
                    3.,
                    crate::ui::toolbar_style::HOVER,
                    Stroke::new(1_f32, Color32::WHITE),
                    StrokeKind::Middle,
                );
            } else {
                ui.painter().rect(
                    rect,
                    3.,
                    crate::ui::toolbar_style::SURFACE,
                    Stroke::new(1_f32, Color32::WHITE),
                    StrokeKind::Middle,
                );
            }
            let padding = 7.;
            let dropdown_arrow_rect_size = vec2(8., 6.);
            let content_rect = rect.shrink(padding);
            let line_rect = Rect::from_min_size(
                content_rect.min,
                vec2(
                    content_rect.width() - dropdown_arrow_rect_size.x - padding,
                    content_rect.height(),
                ),
            );

            let dropdown_arrow_rect = Rect::from_min_size(
                pos2(line_rect.right() + padding, line_rect.top()),
                dropdown_arrow_rect_size,
            );
            ui.painter().add(Shape::convex_polygon(
                vec![
                    dropdown_arrow_rect.left_top(),
                    dropdown_arrow_rect.right_top(),
                    dropdown_arrow_rect.center_bottom(),
                ],
                Color32::WHITE,
                Stroke::new(1.0_f32, Color32::WHITE),
            ));

            let line = get_center_line_segment(&line_rect);
            match stroke_type {
                StrokeType::SolidLine => {
                    ui.painter()
                        .line_segment(line, Stroke::new(1_f32, Color32::WHITE));
                }
                StrokeType::DashedLine => {
                    let shape =
                        Shape::dashed_line(&line, Stroke::new(1_f32, Color32::WHITE), 6., 3.);
                    ui.painter().add(shape);
                }
                StrokeType::DottedLine => {
                    let shape = Shape::dotted_line(&line, Color32::WHITE, 6., 3.);
                    ui.painter().add(shape);
                }
            }
            response
        })),
        build_drop_down_area_fn: Arc::new(Box::new(
            |_app, _window, current_view, annotator_state, ui| {
                ui.vertical_centered(|ui| {
                    let tool = annotator_state.current_annotation_tool.as_mut().unwrap();
                    let stroke_type = tool.stroke_type();
                    if ui
                        .add(
                            StrokeTypeButton::new(90., 32., StrokeType::SolidLine)
                                .selected(stroke_type == StrokeType::SolidLine),
                        )
                        .clicked()
                    {
                        tool.set_stroke_type(StrokeType::SolidLine);
                        current_view.close_later();
                    }
                    if ui
                        .add(
                            StrokeTypeButton::new(90., 32., StrokeType::DashedLine)
                                .selected(stroke_type == StrokeType::DashedLine),
                        )
                        .clicked()
                    {
                        tool.set_stroke_type(StrokeType::DashedLine);
                        current_view.close_later();
                    }
                    if ui
                        .add(
                            StrokeTypeButton::new(90., 32., StrokeType::DottedLine)
                                .selected(stroke_type == StrokeType::DottedLine),
                        )
                        .clicked()
                    {
                        tool.set_stroke_type(StrokeType::DottedLine);
                        current_view.close_later();
                    }

                    let tool = annotator_state.current_annotation_tool.as_mut().unwrap();
                    if stroke_type != tool.stroke_type()
                        && let Some(annotation) = annotator_state.annotations_stack.last_mut()
                        && annotation.was_created_by(tool)
                        && annotation.activation().is_active()
                    {
                        annotation.set_stroke_type(tool.stroke_type());
                    }
                });
            },
        )),
    };
    ui.add(dropdown);
}

// ─── 笔触宽度下拉选择器 ────────────────────────────────────────────────────────

pub(crate) const STROKE_WIDTH_SELECTOR_WIDTH: u32 = 64u32;
const STROKE_WIDTHS: &[f32] = &[1.0, 2.0, 3.0, 5.0, 8.0];
const MARKER_PEN_WIDTHS: &[f32] = &[8.0, 12.0, 18.0, 24.0, 32.0];

pub(crate) fn stroke_widths_for_tool(tool_name: crate::annotator::ToolName) -> &'static [f32] {
    match tool_name {
        crate::annotator::ToolName::MarkerPen => MARKER_PEN_WIDTHS,
        _ => STROKE_WIDTHS,
    }
}

pub fn create_stroke_width_selector(
    id: Id,
    app: &mut Application,
    window: &mut AppWindow,
    current_view: &mut dyn View,
    annotator_state: &mut AnnotatorState,
    ui: &mut Ui,
) {
    let tool_name = annotator_state
        .current_annotation_tool
        .as_ref()
        .map(|tool| tool.tool_name())
        .expect("stroke width selector requires an active tool");
    let stroke_widths = stroke_widths_for_tool(tool_name);
    let dropdown = DropdownBox {
        id,
        app,
        window,
        current_view,
        annotator_state,
        drop_down_area_size: LogicalSize::new(
            STROKE_WIDTH_SELECTOR_WIDTH + 14,
            stroke_widths.len() as u32 * 32 + 14,
        ),
        build_drop_down_box_button_fn: Arc::new(Box::new(|_id, ui, annotator_state| {
            let tool = annotator_state.current_annotation_tool.as_ref().unwrap();
            let current_width = if tool.supports_get_stroke_width() {
                tool.stroke_width()
            } else {
                1.0
            };
            let (rect, response) = ui.allocate_exact_size(
                vec2(STROKE_WIDTH_SELECTOR_WIDTH as f32, 20.),
                Sense::click(),
            );
            if response.hovered() {
                ui.painter().rect(
                    rect,
                    3.,
                    crate::ui::toolbar_style::HOVER,
                    Stroke::new(1_f32, Color32::WHITE),
                    StrokeKind::Middle,
                );
            } else {
                ui.painter().rect(
                    rect,
                    3.,
                    crate::ui::toolbar_style::SURFACE,
                    Stroke::new(1_f32, Color32::WHITE),
                    StrokeKind::Middle,
                );
            }

            let padding = 7.;
            let dropdown_arrow_rect_size = vec2(8., 6.);
            let content_rect = rect.shrink(padding);

            // 下拉箭头
            let dropdown_arrow_rect = Rect::from_min_size(
                pos2(
                    content_rect.right() - dropdown_arrow_rect_size.x,
                    content_rect.center().y - 3.,
                ),
                dropdown_arrow_rect_size,
            );
            ui.painter().add(Shape::convex_polygon(
                vec![
                    dropdown_arrow_rect.left_top(),
                    dropdown_arrow_rect.right_top(),
                    dropdown_arrow_rect.center_bottom(),
                ],
                Color32::WHITE,
                Stroke::new(1.0_f32, Color32::WHITE),
            ));

            // 在左侧用横线预览当前粗细
            let preview_right = dropdown_arrow_rect.left() - padding;
            let center_y = rect.center().y;
            let preview_width = (current_width).min(content_rect.height() - 2.0);
            ui.painter().line_segment(
                [
                    pos2(content_rect.left(), center_y),
                    pos2(preview_right, center_y),
                ],
                Stroke::new(preview_width, Color32::WHITE),
            );

            response
        })),
        build_drop_down_area_fn: Arc::new(Box::new(
            move |_app, _window, current_view, annotator_state, ui| {
                ui.vertical_centered(|ui| {
                    let tool = annotator_state.current_annotation_tool.as_ref().unwrap();
                    let old_width = if tool.supports_get_stroke_width() {
                        tool.stroke_width()
                    } else {
                        1.0
                    };

                    for &w in stroke_widths {
                        let is_selected = (old_width - w).abs() < 0.01;
                        if ui
                            .add(StrokeWidthButton::new(
                                STROKE_WIDTH_SELECTOR_WIDTH as f32 - 10.,
                                32.,
                                w,
                                is_selected,
                            ))
                            .clicked()
                        {
                            let tool = annotator_state.current_annotation_tool.as_mut().unwrap();
                            if tool.supports_set_stroke_width() {
                                tool.set_stroke_width(w);
                            }
                            // 同步更新栈顶标注
                            let tool = annotator_state.current_annotation_tool.as_ref().unwrap();
                            if let Some(ann) = annotator_state.annotations_stack.last_mut()
                                && ann.was_created_by(tool)
                                && ann.activation().is_active()
                                && ann.supports_set_stroke_width()
                            {
                                ann.set_stroke_width(w);
                            }
                            current_view.close_later();
                        }
                    }
                });
            },
        )),
    };
    ui.add(dropdown);
}

pub(crate) struct StrokeTypeButton {
    width: f32,
    height: f32,
    stroke_type: StrokeType,
    selected: bool,
}

impl StrokeTypeButton {
    pub fn new(width: f32, height: f32, stroke_type: StrokeType) -> Self {
        Self {
            width,
            height,
            stroke_type,
            selected: false,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Widget for StrokeTypeButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(vec2(self.width, self.height), Sense::click());
        if self.selected {
            ui.painter()
                .rect_filled(rect, 4.0, crate::ui::toolbar_style::ACCENT);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, 4.0, crate::ui::toolbar_style::HOVER);
        } else {
            ui.painter()
                .rect_filled(rect, 3., crate::ui::toolbar_style::SURFACE);
        }
        let line_rect = rect.shrink(6.);
        let line = get_center_line_segment(&line_rect);

        match self.stroke_type {
            StrokeType::SolidLine => {
                ui.painter()
                    .line_segment(line, Stroke::new(1_f32, Color32::WHITE));
            }
            StrokeType::DashedLine => {
                let shape = Shape::dashed_line(&line, Stroke::new(1_f32, Color32::WHITE), 6., 3.);
                ui.painter().add(shape);
            }
            StrokeType::DottedLine => {
                let shape = Shape::dotted_line(&line, Color32::WHITE, 8., 2.);
                ui.painter().add(shape);
            }
        }

        response
    }
}

fn get_center_line_segment(rect: &Rect) -> [Pos2; 2] {
    let line_start = pos2(rect.left(), rect.left_center().y);
    let line_end = pos2(rect.right(), rect.right_center().y);
    [line_start, line_end]
}

pub(crate) struct StrokeWidthButton {
    width: f32,
    height: f32,
    stroke_width: f32,
    selected: bool,
}

impl StrokeWidthButton {
    pub fn new(width: f32, height: f32, stroke_width: f32, selected: bool) -> Self {
        Self {
            width,
            height,
            stroke_width,
            selected,
        }
    }
}

impl Widget for StrokeWidthButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) =
            ui.allocate_exact_size(vec2(self.width, self.height), Sense::click());

        let bg = if self.selected || response.hovered() {
            crate::ui::toolbar_style::ACCENT
        } else {
            crate::ui::toolbar_style::SURFACE
        };
        ui.painter().rect_filled(rect, 3., bg);

        // 居中横线，粗细直观反映 stroke_width
        let center_y = rect.center().y;
        let line_w = self.stroke_width.min(rect.height() - 6.0);
        let line_rect = rect.shrink(8.);
        ui.painter().line_segment(
            [
                pos2(line_rect.left(), center_y),
                pos2(line_rect.right(), center_y),
            ],
            Stroke::new(line_w, Color32::WHITE),
        );

        response
    }
}
