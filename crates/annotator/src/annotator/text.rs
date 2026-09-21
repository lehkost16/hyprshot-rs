use crate::annotator::cursor::{CustomCursor, Move};
use crate::annotator::rectangle_based::{HitTarget, HitTest};
use crate::annotator::{
    ActivationState, ActivationSupport, Annotation, AnnotationActivationSupport,
    AnnotationToolCommon, AnnotatorState, ApplyExtraZoomFactor, FontColorSupport, PainterExt,
    RemoveExtraZoomFactor, SharedAnnotatorState, StackTopAccessor, UnsubmittedAnnotationHandler,
};
use crate::annotator::{
    FillColorSupport, StrokeColorSupport, StrokeType, StrokeTypeSupport, StrokeWidthSupport,
};
use crate::{
    declare_not_support_fill_color, declare_not_support_stroke_color,
    declare_not_support_stroke_type, impl_stack_top_access_for,
};
use delegate::delegate;
use egui::{
    Color32, CursorIcon, FontId, Margin, PointerButton, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, TextEdit, Ui, UiBuilder, Widget, vec2,
};
use log::info;
use std::cell::RefCell;
use std::ops::Add;
use std::rc::Weak;

#[derive(Debug, Clone)]
pub struct TextStyle {
    /// 文本颜色
    text_color: Color32,
    /// 字体族和字体大小
    font: FontId,
    /// 文本和边框的间距
    padding: Margin,
    /// 边框的线条样式
    stroke: Stroke,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            text_color: Color32::RED,
            font: FontId::proportional(18.),
            padding: Margin::same(10),
            stroke: Stroke::new(1.0_f32, Color32::WHITE),
        }
    }
}

impl StrokeWidthSupport for TextStyle {
    fn supports_get_stroke_width(&self) -> bool {
        true
    }

    fn stroke_width(&self) -> f32 {
        self.stroke.width
    }

    fn supports_set_stroke_width(&self) -> bool {
        true
    }

    fn set_stroke_width(&mut self, stroke_width: f32) {
        self.stroke.width = stroke_width;
        let font_size = match stroke_width as i32 {
            1 => 14.0,
            2 => 18.0,
            3 => 24.0,
            5 => 32.0,
            8 => 44.0,
            _ => (stroke_width * 6.0).max(12.0),
        };
        self.font = FontId::proportional(font_size);
    }
}

impl StrokeColorSupport for TextStyle {
    fn supports_get_stroke_color(&self) -> bool {
        true
    }

    fn stroke_color(&self) -> Color32 {
        self.stroke.color
    }

    fn supports_set_stroke_color(&self) -> bool {
        false
    }

    fn set_stroke_color(&mut self, _color: Color32) {
        unimplemented!()
    }
}

declare_not_support_fill_color!(TextStyle);
declare_not_support_stroke_type!(TextStyle);

#[derive(Clone)]
pub struct TextAnnotation {
    style: TextStyle,
    text: String,
    /// 把第一行文本的宽高当作一个矩形，pos是这个矩形左侧的边的中点
    pos: Pos2,
    activation: ActivationSupport,
    /// 仅用于记录文本标注组件的矩形区域信息，不应该从外部修改它
    rect: Option<Rect>,
}

impl TextAnnotation {
    pub fn new(style: TextStyle, text: String, pos: Pos2, activation: ActivationSupport) -> Self {
        Self {
            style,
            text,
            pos,
            activation,
            rect: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn text_mut(&mut self) -> &mut String {
        &mut self.text
    }

    pub fn pos(&self) -> Pos2 {
        self.pos
    }

    pub fn set_pos(&mut self, pos: Pos2) {
        self.pos = pos;
    }

    fn measure_text(&self, ui: &mut Ui) -> (f32, f32) {
        let galley = ui.fonts_mut(|fonts_view| {
            fonts_view.layout(
                self.text.clone(),
                self.style.font.clone(),
                self.style.text_color,
                f32::INFINITY, // 不因宽度折行，保留原始换行
            )
        });
        let max_line_width = galley
            .rows
            .iter()
            .map(|row| row.rect().width())
            .fold(0.0, f32::max);

        let height = galley
            .rows
            .iter()
            .map(|row| row.rect().height())
            .fold(0.0, f32::add);
        (max_line_width, height)
    }

    pub fn rect(&self) -> Option<&Rect> {
        self.rect.as_ref()
    }

    pub fn set_text_color(&mut self, color: Color32) {
        self.style.text_color = color;
    }
}

impl From<TextAnnotation> for Annotation {
    fn from(val: TextAnnotation) -> Self {
        Annotation::Text(val)
    }
}

const MIN_TEXT_WIDTH: f32 = 30.0;

impl Widget for &mut TextAnnotation {
    fn ui(self, ui: &mut Ui) -> Response {
        let (max_text_width, text_height) = self.measure_text(ui);
        let style = self.style.clone();
        let row_height = ui.fonts_mut(|f| f.row_height(&style.font));
        let text_width = max_text_width.max(MIN_TEXT_WIDTH);
        let text_height = text_height.max(row_height);
        let left_top = self.pos().apply_extra_zoom_factor_with_ctx(ui.ctx())
            - vec2(
                style.padding.leftf(),
                style.padding.topf() + row_height / 2.0,
            );
        // 文本的底部和边框添加额外的间距，不然边框底边的鼠标的拖动事件会被编辑框吞了，导致无法拖动
        let extra_padding_bottom = 20.;
        let right_bottom = left_top
            + vec2(
                text_width + style.padding.sum().x,
                text_height + style.padding.sum().y + extra_padding_bottom,
            );
        let rect = Rect::from_two_pos(left_top, right_bottom);
        self.rect = Some(rect);
        if self.activation.is_active() {
            let painter = ui.painter();
            painter.rect_stroke(rect, 0., self.style.stroke, StrokeKind::Middle);
            painter.small_rects(&rect);
        }
        let response = ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            let frame = egui::Frame::default().inner_margin(style.padding);
            let response = frame.show(ui, |ui| {
                let text_color = self.style.text_color;
                let interactive = self.activation.is_active();
                let text_edit = TextEdit::multiline(self.text_mut())
                    .frame(egui::Frame::NONE)
                    .text_color(text_color)
                    .font(style.font)
                    .interactive(interactive)
                    .margin(Margin::same(0))
                    .desired_width(f32::INFINITY)
                    .desired_rows(1);
                let response = ui.add_sized(
                    vec2(text_width, row_height + style.padding.sum().y),
                    text_edit,
                );
                if self.activation.is_active() {
                    response.request_focus();
                }
                response
            });
            response.inner
        });
        response.inner
    }
}

impl StrokeWidthSupport for TextAnnotation {
    delegate! {
        to self.style {
           /// 是否支持获取线条宽度
           fn supports_get_stroke_width(&self) -> bool;
           /// 获取线条宽度
           fn stroke_width(&self) -> f32;
           /// 是否支持设置线条宽度
           fn supports_set_stroke_width(&self) -> bool;
           /// 设置线条宽度
           fn set_stroke_width(&mut self, stroke_width: f32);
        }
    }
}

impl StrokeColorSupport for TextAnnotation {
    delegate! {
        to self.style {
            /// 是否支持获取线条颜色
            fn supports_get_stroke_color(&self) -> bool;
            /// 获取线条颜色
            fn stroke_color(&self) -> Color32;
            /// 是否支持设置线条颜色
            fn supports_set_stroke_color(&self) -> bool;
            /// 设置线条颜色
            fn set_stroke_color(&mut self, color: Color32);
        }
    }
}

impl AnnotationActivationSupport for TextAnnotation {
    fn activation(&self) -> &ActivationSupport {
        &self.activation
    }

    fn activation_mut(&mut self) -> &mut ActivationSupport {
        &mut self.activation
    }
}

declare_not_support_fill_color!(TextAnnotation);
declare_not_support_stroke_type!(TextAnnotation);

#[derive(Default)]
pub struct TextToolState {
    /// 样式
    style: TextStyle,
    /// 当前的标注
    current_annotation: Option<TextAnnotation>,
}

pub struct TextTool {
    annotator_state: Weak<RefCell<AnnotatorState>>,
    tool_state: TextToolState,
    custom_cursor: Option<Box<dyn CustomCursor>>,
    is_dragging: bool,
}

impl TextTool {
    pub fn new(annotator_state: Weak<RefCell<AnnotatorState>>) -> Self {
        Self {
            annotator_state,
            tool_state: TextToolState::default(),
            custom_cursor: None,
            is_dragging: false,
        }
    }

    /// 如果栈顶不存在TextAnnotation或者TextAnnotation暂时不知道rect，则返回None
    /// 否则返回碰撞检测结果
    fn hit_test_for_annotation_on_stack_top(&self, pointer_pos: &Pos2) -> Option<HitTarget> {
        self.peek_annotation(|annotation_on_stack_top: Option<&TextAnnotation>| {
            // 判断当前鼠标是否位于此标注上
            match annotation_on_stack_top {
                None => None,
                Some(annotation) => Self::hit_test_for_annotation(annotation, pointer_pos),
            }
        })
    }

    fn hit_test_for_annotation(
        annotation: &TextAnnotation,
        pointer_pos: &Pos2,
    ) -> Option<HitTarget> {
        // 判断当前鼠标是否位于此标注上
        let stroke_width = annotation.stroke_width();
        annotation
            .rect()
            .map(|rect| rect.hit_test(pointer_pos, stroke_width))
    }

    fn update_cursor_icon(&mut self, ui: &mut Ui) {
        let Some(pointer_pos) = ui.ctx().pointer_hover_pos() else {
            return;
        };
        if self.is_dragging {
            ui.ctx().set_cursor_icon(CursorIcon::None);
            self.custom_cursor = Some(Box::new(Move::new(pointer_pos)));
            return;
        }
        // 从标注栈的栈顶中获取最近的一个标注
        if let Some(annotation) = self.tool_state.current_annotation.as_ref() {
            let hit_target = Self::hit_test_for_annotation(
                annotation,
                &(pointer_pos.remove_extra_zoom_factor_with_ctx(ui.ctx())),
            );
            if let Some(hit_target) = hit_target {
                // 鼠标位于边框上
                if hit_target != HitTarget::Outside && hit_target != HitTarget::Inside {
                    ui.ctx().set_cursor_icon(CursorIcon::None);
                    self.custom_cursor = Some(Box::new(Move::new(pointer_pos)));
                } else {
                    ui.ctx().set_cursor_icon(CursorIcon::Text);
                }
            } else {
                ui.ctx().set_cursor_icon(CursorIcon::Text);
            }
        } else {
            ui.ctx().set_cursor_icon(CursorIcon::Text);
        }
    }

    pub fn submit_current_annotation(&mut self, annotator_state: &mut AnnotatorState) {
        if let Some(mut annotation) = self.tool_state.current_annotation.take() {
            annotation.activation.deactivate();
            if !annotation.text.is_empty() {
                annotator_state.submit_annotation(annotation.into());
            }
        }
    }
}

impl AnnotationToolCommon for TextTool {
    fn annotator_state(&self) -> SharedAnnotatorState {
        self.annotator_state.upgrade().unwrap()
    }
}

impl StrokeWidthSupport for TextTool {
    delegate! {
        to self.tool_state.style {
            /// 是否支持获取线条宽度
            fn supports_get_stroke_width(&self) -> bool;
            /// 获取线条宽度
            fn stroke_width(&self) -> f32;
            /// 是否支持设置线条宽度
            fn supports_set_stroke_width(&self) -> bool;
            /// 设置线条宽度
            fn set_stroke_width(&mut self, stroke_width: f32);
        }
    }
}

declare_not_support_fill_color!(TextTool);
declare_not_support_stroke_type!(TextTool);
declare_not_support_stroke_color!(TextTool);

impl FontColorSupport for TextTool {
    fn supports_get_font_color(&self) -> bool {
        true
    }

    fn font_color(&self) -> Color32 {
        self.tool_state.style.text_color
    }

    fn supports_set_font_color(&self) -> bool {
        true
    }

    fn set_font_color(&mut self, color: Color32) {
        self.tool_state.style.text_color = color;
        if let Some(annotation) = self.tool_state.current_annotation.as_mut() {
            annotation.set_text_color(color);
        }
    }
}

impl_stack_top_access_for!(TextTool=>TextAnnotation);

impl StackTopAccessor<TextAnnotation> for AnnotatorState {
    fn peek_annotation<F, R>(&self, func: F) -> Option<R>
    where
        F: Fn(Option<&TextAnnotation>) -> Option<R>,
    {
        // 从标注栈的栈顶中获取最近的一个文本标注
        let text_annotation_on_stack_top =
            self.annotations_stack
                .last()
                .and_then(|annotation| match annotation {
                    Annotation::Text(text_annotation) => Some(text_annotation),
                    _ => None,
                });
        func(text_annotation_on_stack_top)
    }

    fn peek_annotation_mut<F, R>(&mut self, func: F) -> Option<R>
    where
        F: Fn(Option<&mut TextAnnotation>) -> Option<R>,
    {
        // 从标注栈的栈顶中获取最近的一个文本标注
        let text_annotation_on_stack_top =
            self.annotations_stack
                .last_mut()
                .and_then(|annotation| match annotation {
                    Annotation::Text(text_annotation) => Some(text_annotation),
                    _ => None,
                });
        func(text_annotation_on_stack_top)
    }

    fn pop_annotation(&mut self) -> Option<TextAnnotation> {
        // 从标注栈的栈顶中获取最近的一个文本标注
        self.annotations_stack
            .pop()
            .and_then(|annotation| match annotation {
                Annotation::Text(text_annotation) => Some(text_annotation),
                _ => None,
            })
    }
}

impl Widget for &mut TextTool {
    fn ui(self, ui: &mut Ui) -> Response {
        let sense_area = Rect::from_min_size(Pos2::ZERO, ui.available_size());
        let response = ui.allocate_rect(sense_area, Sense::click_and_drag());

        self.update_cursor_icon(ui);

        if response.drag_started_by(PointerButton::Primary) {
            info!("drag started!");
            self.is_dragging = true;
        } else if response.clicked_by(PointerButton::Primary) {
            let pointer_pos = response.hover_pos().unwrap();
            let pointer_pos = pointer_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
            let hit_target = self.hit_test_for_annotation_on_stack_top(&pointer_pos);
            if hit_target != Some(HitTarget::Inside) {
                if self.tool_state.current_annotation.is_some() {
                    let annotation_state = self.annotator_state.upgrade().unwrap();
                    self.submit_current_annotation(&mut annotation_state.borrow_mut());
                }
                let annotation = TextAnnotation::new(
                    self.tool_state.style.clone(),
                    String::new(),
                    pointer_pos,
                    ActivationSupport::Supported(ActivationState::active()),
                );
                self.tool_state.current_annotation = Some(annotation);
            } else if hit_target == Some(HitTarget::Inside)
                && self.tool_state.current_annotation.is_none()
                && let Some(mut annotation) = self.pop_annotation()
            {
                annotation.activation.activate();
                self.tool_state.current_annotation = Some(annotation);
            }
        } else if response.dragged_by(PointerButton::Primary) {
            if let Some(annotation) = self.tool_state.current_annotation.as_mut() {
                let drag_delta = response
                    .drag_delta()
                    .remove_extra_zoom_factor_with_ctx(ui.ctx());
                let new_pos = annotation.pos + drag_delta;
                annotation.set_pos(new_pos);
                ui.add(annotation);
            }
        } else {
            if let Some(annotation) = self.tool_state.current_annotation.as_mut() {
                ui.add(annotation);
            }
        }

        if response.drag_stopped_by(PointerButton::Primary) {
            self.is_dragging = false;
        }

        if let Some(custom_cursor) = self.custom_cursor.take() {
            custom_cursor.paint_with(ui.painter());
        }

        response
    }
}

impl UnsubmittedAnnotationHandler for TextTool {
    fn has_uncommitted_annotations(&self) -> bool {
        self.tool_state.current_annotation.is_some()
    }

    fn submit_uncommitted_annotations(&mut self, annotator_state: &mut AnnotatorState) {
        self.submit_current_annotation(annotator_state);
    }

    fn drop_uncommitted_annotations(&mut self) -> Annotation {
        if self.tool_state.current_annotation.is_some() {
            let text_annotation = self.tool_state.current_annotation.take().unwrap();
            return Annotation::Text(text_annotation);
        }
        panic!("There are no unsubmitted annotations in TextTool!");
    }
}
