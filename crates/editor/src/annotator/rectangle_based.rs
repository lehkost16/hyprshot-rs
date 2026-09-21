use crate::annotator::cursor::{Crosshair, CustomCursor};
use crate::annotator::{
    ActivationState, ActivationSupport, Annotation, AnnotationActivationSupport, AnnotationStyle,
    AnnotationToolCommon, AnnotatorState, ApplyExtraZoomFactor, FillColorSupport, FontColorSupport,
    PainterExt, RemoveExtraZoomFactor, SharedAnnotatorState, StackTopAccessor, StrokeColorSupport,
    StrokeType, StrokeTypeSupport, StrokeWidthSupport, UnsubmittedAnnotationHandler,
};
use crate::{declare_not_support_font_color, impl_stack_top_access_for};
use egui::PointerButton;
use egui::epaint::EllipseShape;
use egui::{
    Color32, CursorIcon, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Widget, pos2, vec2,
};
use std::cell::RefCell;
use std::rc::Weak;

#[derive(Debug, Copy, Clone)]
pub struct RectangleStyle {
    /// 线条颜色和宽度
    stroke: Stroke,
    /// 线条类型
    stroke_type: StrokeType,
    /// 填充颜色
    fill_color: Option<Color32>,
}

impl StrokeWidthSupport for RectangleStyle {
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
    }
}

impl StrokeColorSupport for RectangleStyle {
    fn supports_get_stroke_color(&self) -> bool {
        true
    }

    fn stroke_color(&self) -> Color32 {
        self.stroke.color
    }

    fn supports_set_stroke_color(&self) -> bool {
        true
    }

    fn set_stroke_color(&mut self, color: Color32) {
        self.stroke.color = color;
    }
}

impl StrokeTypeSupport for RectangleStyle {
    fn supports_get_stroke_type(&self) -> bool {
        true
    }

    fn stroke_type(&self) -> StrokeType {
        self.stroke_type
    }

    fn supports_set_stroke_type(&self) -> bool {
        true
    }

    fn set_stroke_type(&mut self, stroke_type: StrokeType) {
        self.stroke_type = stroke_type;
    }
}

impl FillColorSupport for RectangleStyle {
    fn supports_get_fill_color(&self) -> bool {
        true
    }

    fn fill_color(&self) -> Option<Color32> {
        self.fill_color
    }

    fn supports_set_fill_color(&self) -> bool {
        true
    }

    fn set_fill_color(&mut self, color: Color32) {
        if color.a() == 0 {
            self.fill_color = None;
        } else {
            self.fill_color = Some(color);
        }
    }
}

impl AnnotationStyle for RectangleStyle {}

impl Default for RectangleStyle {
    fn default() -> Self {
        Self {
            stroke: Stroke::new(1_f32, Color32::from_rgb(255, 0, 0)),
            stroke_type: StrokeType::SolidLine,
            fill_color: None,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct EllipseStyle {
    /// 线条颜色和宽度
    pub stroke: Stroke,
    /// 线条类型
    pub stroke_type: StrokeType,
    /// 填充颜色
    pub fill_color: Option<Color32>,
}

impl StrokeWidthSupport for EllipseStyle {
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
    }
}

impl StrokeColorSupport for EllipseStyle {
    fn supports_get_stroke_color(&self) -> bool {
        true
    }

    fn stroke_color(&self) -> Color32 {
        self.stroke.color
    }

    fn supports_set_stroke_color(&self) -> bool {
        true
    }

    fn set_stroke_color(&mut self, color: Color32) {
        self.stroke.color = color;
    }
}

impl StrokeTypeSupport for EllipseStyle {
    fn supports_get_stroke_type(&self) -> bool {
        true
    }

    fn stroke_type(&self) -> StrokeType {
        self.stroke_type
    }

    fn supports_set_stroke_type(&self) -> bool {
        false
    }

    fn set_stroke_type(&mut self, _stroke_type: StrokeType) {
        unimplemented!()
    }
}

impl FillColorSupport for EllipseStyle {
    fn supports_get_fill_color(&self) -> bool {
        true
    }

    fn fill_color(&self) -> Option<Color32> {
        self.fill_color
    }

    fn supports_set_fill_color(&self) -> bool {
        true
    }

    fn set_fill_color(&mut self, color: Color32) {
        if color.a() == 0 {
            self.fill_color = None;
        } else {
            self.fill_color = Some(color);
        }
    }
}

impl AnnotationStyle for EllipseStyle {}

impl Default for EllipseStyle {
    fn default() -> Self {
        Self {
            stroke: Stroke::new(1_f32, Color32::from_rgb(255, 0, 0)),
            stroke_type: StrokeType::SolidLine,
            fill_color: None,
        }
    }
}

/// 基于矩形的标注
#[derive(Debug, Clone)]
pub struct RectangleBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    /// 区域
    rect: Rect,
    /// 样式
    style: S,
    /// 激活状态
    activation: ActivationSupport,
}

impl<S> RectangleBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    pub fn new(rect: Rect, style: S, activation: ActivationSupport) -> Self {
        Self {
            rect,
            style,
            activation,
        }
    }

    pub fn rect(&self) -> &Rect {
        &self.rect
    }
}

impl<S> StrokeWidthSupport for RectangleBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    fn supports_get_stroke_width(&self) -> bool {
        self.style.supports_get_stroke_width()
    }

    fn stroke_width(&self) -> f32 {
        self.style.stroke_width()
    }

    fn supports_set_stroke_width(&self) -> bool {
        self.style.supports_set_stroke_width()
    }

    fn set_stroke_width(&mut self, stroke_width: f32) {
        self.style.set_stroke_width(stroke_width)
    }
}

impl<S> StrokeColorSupport for RectangleBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    fn supports_get_stroke_color(&self) -> bool {
        self.style.supports_get_stroke_color()
    }

    fn stroke_color(&self) -> Color32 {
        self.style.stroke_color()
    }

    fn supports_set_stroke_color(&self) -> bool {
        self.style.supports_set_stroke_color()
    }

    fn set_stroke_color(&mut self, color: Color32) {
        self.style.set_stroke_color(color);
    }
}

impl<S> StrokeTypeSupport for RectangleBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    fn supports_get_stroke_type(&self) -> bool {
        self.style.supports_get_stroke_type()
    }

    fn stroke_type(&self) -> StrokeType {
        self.style.stroke_type()
    }

    fn supports_set_stroke_type(&self) -> bool {
        self.style.supports_set_stroke_type()
    }

    fn set_stroke_type(&mut self, stroke_type: StrokeType) {
        self.style.set_stroke_type(stroke_type);
    }
}

impl<S> FillColorSupport for RectangleBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    fn supports_get_fill_color(&self) -> bool {
        self.style.supports_get_fill_color()
    }

    fn fill_color(&self) -> Option<Color32> {
        self.style.fill_color()
    }

    fn supports_set_fill_color(&self) -> bool {
        self.style.supports_set_fill_color()
    }

    fn set_fill_color(&mut self, color: Color32) {
        self.style.set_fill_color(color);
    }
}

impl<S> AnnotationActivationSupport for RectangleBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    fn activation(&self) -> &ActivationSupport {
        &self.activation
    }

    fn activation_mut(&mut self) -> &mut ActivationSupport {
        &mut self.activation
    }
}

pub type RectangleAnnotation = RectangleBasedAnnotation<RectangleStyle>;
pub type EllipseAnnotation = RectangleBasedAnnotation<EllipseStyle>;

impl From<RectangleAnnotation> for Annotation {
    fn from(val: RectangleAnnotation) -> Self {
        Annotation::Rectangle(val)
    }
}

impl From<EllipseAnnotation> for Annotation {
    fn from(val: EllipseAnnotation) -> Self {
        Annotation::Ellipse(val)
    }
}

impl Widget for &mut RectangleAnnotation {
    fn ui(self, ui: &mut Ui) -> Response {
        let rect = self.rect().apply_extra_zoom_factor_with_ctx(ui.ctx());
        let response = ui.allocate_rect(rect, Sense::hover());
        let painter = ui.painter();
        if let Some(fill_color) = self.style.fill_color {
            painter.rectangle(
                &rect,
                fill_color,
                self.style.stroke,
                StrokeKind::Middle,
                self.style.stroke_type,
            );
        } else {
            painter.rectangle(
                &rect,
                Color32::TRANSPARENT,
                self.style.stroke,
                StrokeKind::Middle,
                self.style.stroke_type,
            );
        }

        if self.activation.is_active() {
            painter.small_rects(&rect);
        }

        response
    }
}

impl Widget for &mut EllipseAnnotation {
    fn ui(self, ui: &mut Ui) -> Response {
        let rect = self.rect().apply_extra_zoom_factor_with_ctx(ui.ctx());
        let response = ui.allocate_rect(rect, Sense::hover());
        let painter = ui.painter();
        let fill = if let Some(fill_color) = self.style.fill_color {
            fill_color
        } else {
            Color32::TRANSPARENT
        };

        let ellipse_shape = EllipseShape {
            center: rect.center(),
            fill,
            stroke: self.style.stroke,
            radius: vec2(rect.width() / 2., rect.height() / 2.),
            angle: 0.0,
        };

        painter.add(ellipse_shape);

        if self.activation.is_active() {
            // 绘制虚线矩形框以及外框上的各个角以及边上的小矩形
            painter.small_rects(&rect);
        }
        response
    }
}

pub struct RectangleBasedToolState<S>
where
    S: AnnotationStyle + Default,
{
    /// 样式
    style: S,
    /// 当前的标注
    current_annotation: Option<RectangleBasedAnnotation<S>>,
    /// 当拖动鼠标的时候需要执行的操作
    drag_action: DragAction,
}

impl<S> Default for RectangleBasedToolState<S>
where
    S: AnnotationStyle + Default,
{
    fn default() -> Self {
        Self {
            style: Default::default(),
            current_annotation: None,
            drag_action: DragAction::None,
        }
    }
}

#[allow(dead_code)]
const NOTHING: Option<()> = None::<()>;

impl StackTopAccessor<RectangleAnnotation> for AnnotatorState {
    fn peek_annotation<F, R>(&self, func: F) -> Option<R>
    where
        F: Fn(Option<&RectangleAnnotation>) -> Option<R>,
    {
        let rectangle_annotation_on_stack_top =
            self.annotations_stack
                .last()
                .and_then(|annotation| match annotation {
                    Annotation::Rectangle(rectangle_annotation) => Some(rectangle_annotation),
                    _ => None,
                });
        func(rectangle_annotation_on_stack_top)
    }

    fn peek_annotation_mut<F, R>(&mut self, func: F) -> Option<R>
    where
        F: Fn(Option<&mut RectangleAnnotation>) -> Option<R>,
    {
        let rectangle_annotation_on_stack_top =
            self.annotations_stack
                .last_mut()
                .and_then(|annotation| match annotation {
                    Annotation::Rectangle(rectangle_annotation) => Some(rectangle_annotation),
                    _ => None,
                });
        func(rectangle_annotation_on_stack_top)
    }

    fn pop_annotation(&mut self) -> Option<RectangleAnnotation> {
        self.annotations_stack
            .pop()
            .and_then(|annotation| match annotation {
                Annotation::Rectangle(rectangle_annotation) => Some(rectangle_annotation),
                _ => None,
            })
    }
}

impl StackTopAccessor<EllipseAnnotation> for AnnotatorState {
    fn peek_annotation<F, R>(&self, func: F) -> Option<R>
    where
        F: Fn(Option<&EllipseAnnotation>) -> Option<R>,
    {
        let ellipse_annotation_on_stack_top =
            self.annotations_stack
                .last()
                .and_then(|annotation| match annotation {
                    Annotation::Ellipse(ellipse_annotation) => Some(ellipse_annotation),
                    _ => None,
                });
        func(ellipse_annotation_on_stack_top)
    }

    fn peek_annotation_mut<F, R>(&mut self, func: F) -> Option<R>
    where
        F: Fn(Option<&mut EllipseAnnotation>) -> Option<R>,
    {
        let ellipse_annotation_on_stack_top =
            self.annotations_stack
                .last_mut()
                .and_then(|annotation| match annotation {
                    Annotation::Ellipse(ellipse_annotation) => Some(ellipse_annotation),
                    _ => None,
                });
        func(ellipse_annotation_on_stack_top)
    }

    fn pop_annotation(&mut self) -> Option<EllipseAnnotation> {
        self.annotations_stack
            .pop()
            .and_then(|annotation| match annotation {
                Annotation::Ellipse(ellipse_annotation) => Some(ellipse_annotation),
                _ => None,
            })
    }
}

pub struct RectangleBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    annotator_state: Weak<RefCell<AnnotatorState>>,
    tool_state: RectangleBasedToolState<S>,
}

impl<S> RectangleBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    pub fn new(annotator_state: Weak<RefCell<AnnotatorState>>) -> RectangleBasedTool<S> {
        let tool_state = RectangleBasedToolState::default();
        Self {
            annotator_state,
            tool_state,
        }
    }
}

impl<S> StrokeWidthSupport for RectangleBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    fn supports_get_stroke_width(&self) -> bool {
        self.tool_state.style.supports_get_stroke_width()
    }

    fn stroke_width(&self) -> f32 {
        self.tool_state.style.stroke_width()
    }

    fn supports_set_stroke_width(&self) -> bool {
        self.tool_state.style.supports_set_stroke_width()
    }

    fn set_stroke_width(&mut self, stroke_width: f32) {
        self.tool_state.style.set_stroke_width(stroke_width);
    }
}

impl<S> StrokeColorSupport for RectangleBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    fn supports_get_stroke_color(&self) -> bool {
        self.tool_state.style.supports_get_stroke_color()
    }

    fn stroke_color(&self) -> Color32 {
        self.tool_state.style.stroke_color()
    }

    fn supports_set_stroke_color(&self) -> bool {
        self.tool_state.style.supports_set_stroke_color()
    }

    fn set_stroke_color(&mut self, color: Color32) {
        self.tool_state.style.set_stroke_color(color);
        if let Some(annotation) = &mut self.tool_state.current_annotation {
            annotation.set_stroke_color(color);
        }
    }
}

impl<S> StrokeTypeSupport for RectangleBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    fn supports_get_stroke_type(&self) -> bool {
        self.tool_state.style.supports_get_stroke_type()
    }

    fn stroke_type(&self) -> StrokeType {
        self.tool_state.style.stroke_type()
    }

    fn supports_set_stroke_type(&self) -> bool {
        self.tool_state.style.supports_set_stroke_type()
    }

    fn set_stroke_type(&mut self, stroke_type: StrokeType) {
        self.tool_state.style.set_stroke_type(stroke_type);
        if let Some(annotation) = &mut self.tool_state.current_annotation {
            annotation.set_stroke_type(stroke_type);
        }
    }
}

impl<S> FillColorSupport for RectangleBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    fn supports_get_fill_color(&self) -> bool {
        self.tool_state.style.supports_get_fill_color()
    }

    fn fill_color(&self) -> Option<Color32> {
        self.tool_state.style.fill_color()
    }

    fn supports_set_fill_color(&self) -> bool {
        self.tool_state.style.supports_set_fill_color()
    }

    fn set_fill_color(&mut self, color: Color32) {
        self.tool_state.style.set_fill_color(color);
        if let Some(annotation) = &mut self.tool_state.current_annotation {
            annotation.set_fill_color(color);
        }
    }
}

impl<S> AnnotationToolCommon for RectangleBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    fn annotator_state(&self) -> SharedAnnotatorState {
        self.annotator_state.upgrade().unwrap()
    }
}

impl<S> UnsubmittedAnnotationHandler for RectangleBasedTool<S>
where
    S: AnnotationStyle + Default,
    RectangleBasedAnnotation<S>: Into<Annotation>,
{
    fn has_uncommitted_annotations(&self) -> bool {
        self.tool_state.current_annotation.is_some()
    }

    fn submit_uncommitted_annotations(&mut self, annotator_state: &mut AnnotatorState) {
        if let Some(mut annotation) = self.tool_state.current_annotation.take() {
            annotation.activation_mut().deactivate();
            annotator_state.submit_annotation(annotation.into());
        }
    }

    fn drop_uncommitted_annotations(&mut self) -> Annotation {
        let mut annotation = self.tool_state.current_annotation.take().unwrap();
        annotation.activation_mut().deactivate();
        annotation.into()
    }
}

pub type RectangleTool = RectangleBasedTool<RectangleStyle>;
pub type EllipseTool = RectangleBasedTool<EllipseStyle>;

impl_stack_top_access_for!(RectangleTool=>RectangleAnnotation, EllipseTool=>EllipseAnnotation);

declare_not_support_font_color!(RectangleTool, EllipseTool);

macro_rules! impl_widget_for {
    ($($tool:ty=>$annotation:ty),*) => {
        $(
        impl $tool {
            fn update_cursor_icon(&self, ui: &mut Ui) {
                let Some(raw_pointer_pos) = ui.ctx().pointer_hover_pos() else {
                    return;
                };
                let pointer_pos = raw_pointer_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());

                // 1. 优先查 current_annotation
                if let Some(current) = &self.tool_state.current_annotation {
                    let target = current.rect().hit_test(&pointer_pos, current.stroke_width());
                    if target != HitTarget::Outside {
                        if let Some(cursor) = target.get_cursor() {
                            ui.ctx().set_cursor_icon(cursor);
                            return;
                        }
                    }
                }

                // 2. 查栈顶最新标注
                let hit_target = self.peek_annotation(|latest: Option<&$annotation>| {
                    match latest {
                        None => None,
                        Some(annotation) => {
                            let stroke_width = annotation.stroke_width();
                            Some(annotation.rect().hit_test(&pointer_pos, stroke_width))
                        }
                    }
                });

                if let Some(hit_target) = hit_target {
                    if let Some(cursor) = hit_target.get_cursor() {
                        ui.ctx().set_cursor_icon(cursor);
                        return;
                    }
                }

                ui.ctx().set_cursor_icon(CursorIcon::None);
                Crosshair::new(
                    raw_pointer_pos,
                    Color32::RED,
                    self.tool_state.style.stroke.width,
                )
                .paint_with(ui.painter());
            }
         }

         impl Widget for &mut $tool {
            fn ui(self, ui: &mut Ui) -> Response {
                let sense_area = Rect::from_min_size(Pos2::ZERO, ui.available_size());
                let response = ui.allocate_rect(sense_area, Sense::click_and_drag());

                let Some(pointer_pos) = ui.ctx().pointer_hover_pos() else {
                    return response;
                };

                // 检测鼠标碰撞并绘制光标
                self.update_cursor_icon(ui);

                if response.drag_started_by(PointerButton::Primary) {
                    let drag_started_pos = ui.ctx().input(|i| i.pointer.press_origin()).unwrap();
                    let drag_started_pos = drag_started_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());

                    // 1. 优先检测当前在编辑态的 current_annotation
                    let mut hit_current = None;
                    if let Some(current) = &self.tool_state.current_annotation {
                        let target = current.rect().hit_test(&drag_started_pos, current.stroke_width());
                        if target != HitTarget::Outside {
                            hit_current = Some(target);
                        }
                    }

                    if let Some(hit_target) = hit_current {
                        self.tool_state.drag_action = if hit_target == HitTarget::Inside {
                            DragAction::Move(drag_started_pos, *self.tool_state.current_annotation.as_ref().unwrap().rect())
                        } else {
                            hit_target.get_drag_action()
                        };
                    } else {
                        // 2. 尝试获取栈顶最新那一个标注
                        let hit_latest = self.peek_annotation(|latest: Option<&$annotation>| {
                            match latest {
                                None => None,
                                Some(annotation) => {
                                    let stroke_width = annotation.stroke_width();
                                    let target = annotation.rect().hit_test(&drag_started_pos, stroke_width);
                                    if target != HitTarget::Outside {
                                        Some(target)
                                    } else {
                                        None
                                    }
                                }
                            }
                        });

                        if let Some(hit_target) = hit_latest {
                            if let Some(mut prev) = self.tool_state.current_annotation.take() {
                                prev.activation_mut().deactivate();
                                self.annotator_state().borrow_mut().submit_annotation(prev.into());
                            }

                            if let Some(mut concrete) = self.pop_annotation() {
                                concrete.activation_mut().activate();

                                // 同步样式到工具栏
                                self.tool_state.style.set_stroke_color(concrete.stroke_color());
                                self.tool_state.style.set_stroke_width(concrete.stroke_width());
                                if let Some(fill) = concrete.fill_color() {
                                    self.tool_state.style.set_fill_color(fill);
                                } else {
                                    self.tool_state.style.set_fill_color(Color32::TRANSPARENT);
                                }

                                let start_rect = *concrete.rect();
                                self.tool_state.current_annotation = Some(concrete);
                                self.tool_state.drag_action = if hit_target == HitTarget::Inside {
                                    DragAction::Move(drag_started_pos, start_rect)
                                } else {
                                    hit_target.get_drag_action()
                                };
                            }
                        } else {
                            // 3. 点击空白位置：提交先前标注，准备绘制新的标注
                            if let Some(mut prev) = self.tool_state.current_annotation.take() {
                                prev.activation_mut().deactivate();
                                self.annotator_state().borrow_mut().submit_annotation(prev.into());
                            }

                            let rect = Rect::from_two_pos(drag_started_pos, drag_started_pos);
                            let annotation = <$annotation>::new(
                                rect,
                                self.tool_state.style,
                                ActivationSupport::Supported(ActivationState::new(true)),
                            );
                            self.tool_state.current_annotation = Some(annotation);
                            self.tool_state.drag_action = DragAction::None;
                        }
                    }
                }

                if response.dragged_by(PointerButton::Primary) {
                    let pointer_pos = pointer_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
                    if let Some(annotation) = &mut self.tool_state.current_annotation {
                        match self.tool_state.drag_action {
                            DragAction::Move(start_pos, start_rect) => {
                                let delta = pointer_pos - start_pos;
                                annotation.rect = start_rect.translate(delta);
                            }
                            DragAction::AdjustTopEdge => {
                                annotation.rect.min.y = pointer_pos.y;
                            }
                            DragAction::AdjustBottomEdge => {
                                annotation.rect.max.y = pointer_pos.y;
                            }
                            DragAction::AdjustLeftEdge => {
                                annotation.rect.min.x = pointer_pos.x;
                            }
                            DragAction::AdjustRightEdge => {
                                annotation.rect.max.x = pointer_pos.x;
                            }
                            DragAction::AdjustTopLeftCorner => {
                                annotation.rect.min = pointer_pos;
                            }
                            DragAction::AdjustTopRightCorner => {
                                annotation.rect.min.y = pointer_pos.y;
                                annotation.rect.max.x = pointer_pos.x;
                            }
                            DragAction::AdjustBottomRightCorner => {
                                annotation.rect.max = pointer_pos;
                            }
                            DragAction::AdjustBottomLeftCorner => {
                                annotation.rect.min.x = pointer_pos.x;
                                annotation.rect.max.y = pointer_pos.y;
                            }
                            DragAction::None => {
                                let drag_started_pos = ui.ctx().input(|i| i.pointer.press_origin()).unwrap();
                                let drag_started_pos = drag_started_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
                                annotation.rect = Rect::from_two_pos(drag_started_pos, pointer_pos);
                            }
                        }
                    }
                }

                if response.drag_stopped_by(PointerButton::Primary) {
                    self.tool_state.drag_action = DragAction::None;
                    if let Some(mut current_annotation) = self.tool_state.current_annotation.take() {
                        ui.add(&mut current_annotation);
                        current_annotation.activation_mut().deactivate();
                        if let Some(state) = self.annotator_state.upgrade() {
                            state.borrow_mut().submit_annotation(current_annotation.into());
                        }
                    }
                    ui.ctx().request_repaint();
                }

                // 绘制当前正在编辑态的标注
                if let Some(annotation) = &mut self.tool_state.current_annotation {
                    ui.add(annotation);
                }

                response
            }
        }

        )*
    };
}

impl_widget_for!(RectangleTool => RectangleAnnotation, EllipseTool => EllipseAnnotation);

/// 描述一个矩形的四条边
#[derive(PartialEq, Eq, Copy, Clone)]
#[allow(dead_code)]
pub enum Edges {
    Top,
    Right,
    Bottom,
    Left,
}

/// 描述一个矩形的4个角
#[derive(PartialEq, Eq, Copy, Clone)]
#[allow(dead_code)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum HitTarget {
    // 边
    TopEdge,
    BottomEdge,
    LeftEdge,
    RightEdge,

    // 角
    TopLeftCorner,
    TopRightCorner,
    BottomLeftCorner,
    BottomRightCorner,

    // 其他可能的情况
    Inside,
    Outside,
}

impl HitTarget {
    /// 根据HitTarget获取对应的光标
    pub fn get_cursor(&self) -> Option<CursorIcon> {
        match self {
            HitTarget::TopLeftCorner => Some(CursorIcon::ResizeNorthWest),
            HitTarget::TopRightCorner => Some(CursorIcon::ResizeNorthEast),
            HitTarget::BottomRightCorner => Some(CursorIcon::ResizeSouthEast),
            HitTarget::BottomLeftCorner => Some(CursorIcon::ResizeSouthWest),
            HitTarget::TopEdge => Some(CursorIcon::ResizeNorth),
            HitTarget::RightEdge => Some(CursorIcon::ResizeEast),
            HitTarget::BottomEdge => Some(CursorIcon::ResizeSouth),
            HitTarget::LeftEdge => Some(CursorIcon::ResizeWest),
            HitTarget::Inside => Some(CursorIcon::Move),
            _ => None,
        }
    }

    pub fn get_drag_action(&self) -> DragAction {
        match self {
            HitTarget::TopLeftCorner => DragAction::AdjustTopLeftCorner,
            HitTarget::TopRightCorner => DragAction::AdjustTopRightCorner,
            HitTarget::BottomRightCorner => DragAction::AdjustBottomRightCorner,
            HitTarget::BottomLeftCorner => DragAction::AdjustBottomLeftCorner,
            HitTarget::TopEdge => DragAction::AdjustTopEdge,
            HitTarget::RightEdge => DragAction::AdjustRightEdge,
            HitTarget::BottomEdge => DragAction::AdjustBottomEdge,
            HitTarget::LeftEdge => DragAction::AdjustLeftEdge,
            _ => DragAction::None,
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum DragAction {
    // 移动整个标注 (起始拖拽坐标, 起始矩形位置)
    Move(Pos2, Rect),

    // 调整边
    AdjustTopEdge,
    AdjustBottomEdge,
    AdjustLeftEdge,
    AdjustRightEdge,

    // 调整角
    AdjustTopLeftCorner,
    AdjustTopRightCorner,
    AdjustBottomLeftCorner,
    AdjustBottomRightCorner,

    None,
}

pub trait HitTest {
    /// 对矩形做碰撞检测
    /// 矩形的stroke_kind固定为StrokeKind::Middle
    fn hit_test(&self, pointer_pos: &Pos2, stroke_width: f32) -> HitTarget;
}

impl HitTest for Rect {
    /// 对矩形的4条边做碰撞检测（不会特别精准），返回发生碰撞的边
    /// 矩形的stroke_kind固定为StrokeKind::Middle
    fn hit_test(&self, pointer_pos: &Pos2, stroke_width: f32) -> HitTarget {
        // 允许一定的误差
        let tolerance = 6.;

        let tolerance = if tolerance > stroke_width {
            tolerance
        } else {
            stroke_width
        };
        let half = tolerance / 2.;

        let mut edges = Vec::new();

        // 矩形：
        //  (min.x, min.y)   (max.x, min.y)
        //
        //  (min.x, max.y)   (max.x, max.y)

        let min_pos = self.min;
        let max_pos = self.max;

        // 把每一条边当作一个小矩形来对待
        // 上边
        let small_rect = Rect::from_two_pos(
            pos2(min_pos.x - half, min_pos.y - half),
            pos2(max_pos.x + half, min_pos.y + half),
        );
        if small_rect.contains(*pointer_pos) {
            edges.push(HitTarget::TopEdge);
        }

        // 右边
        let small_rect = Rect::from_two_pos(
            pos2(max_pos.x - half, min_pos.y - half),
            pos2(max_pos.x + half, max_pos.y + half),
        );
        if small_rect.contains(*pointer_pos) {
            edges.push(HitTarget::RightEdge);
        }

        // 下边
        let small_rect = Rect::from_two_pos(
            pos2(min_pos.x - half, max_pos.y - half),
            pos2(max_pos.x + half, max_pos.y + half),
        );
        if small_rect.contains(*pointer_pos) {
            edges.push(HitTarget::BottomEdge);
        }

        // 左边
        let small_rect = Rect::from_two_pos(
            pos2(min_pos.x - half, min_pos.y - half),
            pos2(min_pos.x + half, max_pos.y + half),
        );
        if small_rect.contains(*pointer_pos) {
            edges.push(HitTarget::LeftEdge);
        }

        if edges.contains(&HitTarget::TopEdge) && edges.contains(&HitTarget::LeftEdge) {
            return HitTarget::TopLeftCorner;
        }

        if edges.contains(&HitTarget::TopEdge) && edges.contains(&HitTarget::RightEdge) {
            return HitTarget::TopRightCorner;
        }

        if edges.contains(&HitTarget::BottomEdge) && edges.contains(&HitTarget::RightEdge) {
            return HitTarget::BottomRightCorner;
        }

        if edges.contains(&HitTarget::BottomEdge) && edges.contains(&HitTarget::LeftEdge) {
            return HitTarget::BottomLeftCorner;
        }

        if edges.is_empty() {
            if self.contains(*pointer_pos) {
                return HitTarget::Inside;
            }
        } else {
            return *edges.first().unwrap();
        }

        HitTarget::Outside
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 创建测试用的矩形 (x: 0-100, y: 0-50)
    fn test_rect() -> Rect {
        Rect::from_two_pos(pos2(0.0, 0.0), pos2(100.0, 50.0))
    }

    // 测试基本边界情况
    #[test]
    fn test_hit_test_basic_edges() {
        let rect = test_rect();

        // 测试上边 - 中点
        assert_eq!(rect.hit_test(&pos2(50.0, 0.0), 1.0), HitTarget::TopEdge);

        // 测试上边 - 偏移3像素内 (tolerance=6, half=3)
        assert_eq!(rect.hit_test(&pos2(50.0, 2.9), 1.0), HitTarget::TopEdge);

        // 测试下边
        assert_eq!(rect.hit_test(&pos2(50.0, 50.0), 1.0), HitTarget::BottomEdge);

        // 测试左边
        assert_eq!(rect.hit_test(&pos2(0.0, 25.0), 1.0), HitTarget::LeftEdge);

        // 测试右边
        assert_eq!(rect.hit_test(&pos2(100.0, 25.0), 1.0), HitTarget::RightEdge);
    }

    // 测试角点检测
    #[test]
    fn test_hit_test_corners() {
        let rect = test_rect();

        // 左上角
        assert_eq!(
            rect.hit_test(&pos2(0.0, 0.0), 1.0),
            HitTarget::TopLeftCorner
        );

        // 右上角
        assert_eq!(
            rect.hit_test(&pos2(100.0, 0.0), 1.0),
            HitTarget::TopRightCorner
        );

        // 左下角
        assert_eq!(
            rect.hit_test(&pos2(0.0, 50.0), 1.0),
            HitTarget::BottomLeftCorner
        );

        // 右下角
        assert_eq!(
            rect.hit_test(&pos2(100.0, 50.0), 1.0),
            HitTarget::BottomRightCorner
        );
    }

    // 测试角点区域扩展 (tolerance=6, half=3)
    #[test]
    fn test_hit_test_corner_extended() {
        let rect = test_rect();

        // 左上角扩展区域 (x: -3 to 3, y: -3 to 3)
        assert_eq!(
            rect.hit_test(&pos2(2.9, 2.9), 1.0),
            HitTarget::TopLeftCorner
        );

        // 右上角扩展区域 (x: 97 to 103, y: -3 to 3)
        assert_eq!(
            rect.hit_test(&pos2(97.1, 2.9), 1.0),
            HitTarget::TopRightCorner
        );
    }

    // 测试内部和外部
    #[test]
    fn test_hit_test_inside_outside() {
        let rect = test_rect();

        // 内部中心点
        assert_eq!(rect.hit_test(&pos2(50.0, 25.0), 1.0), HitTarget::Inside);

        // 内部但不是中心
        assert_eq!(rect.hit_test(&pos2(10.0, 10.0), 1.0), HitTarget::Inside);

        // 完全外部
        assert_eq!(rect.hit_test(&pos2(-10.0, 25.0), 1.0), HitTarget::Outside);

        assert_eq!(rect.hit_test(&pos2(50.0, -10.0), 1.0), HitTarget::Outside);

        assert_eq!(rect.hit_test(&pos2(150.0, 25.0), 1.0), HitTarget::Outside);
    }

    // 测试边缘扩展区域
    #[test]
    fn test_hit_test_edge_extended() {
        let rect = test_rect();

        // 上边扩展区域 (y: -3 to 3)
        assert_eq!(rect.hit_test(&pos2(50.0, -2.9), 1.0), HitTarget::TopEdge);

        // 超出扩展区域
        assert_eq!(rect.hit_test(&pos2(50.0, -3.1), 1.0), HitTarget::Outside);

        // 下边扩展区域 (y: 47 to 53)
        assert_eq!(rect.hit_test(&pos2(50.0, 52.9), 1.0), HitTarget::BottomEdge);

        // 左边扩展区域 (x: -3 to 3)
        assert_eq!(rect.hit_test(&pos2(-2.9, 25.0), 1.0), HitTarget::LeftEdge);

        // 右边扩展区域 (x: 97 to 103)
        assert_eq!(rect.hit_test(&pos2(102.9, 25.0), 1.0), HitTarget::RightEdge);
    }

    // 测试不同 stroke_width 值
    #[test]
    fn test_hit_test_different_stroke_width() {
        let rect = test_rect();

        // stroke_width=1.0，tolerance=6
        assert_eq!(rect.hit_test(&pos2(50.0, 2.9), 1.0), HitTarget::TopEdge);

        assert_eq!(
            rect.hit_test(&pos2(50.0, 3.1), 1.0),
            HitTarget::Inside // 在扩展区域外，但在矩形内
        );

        // stroke_width=10.0，tolerance=10，half=5
        // 现在扩展区域更大
        assert_eq!(rect.hit_test(&pos2(50.0, 4.9), 10.0), HitTarget::TopEdge);

        assert_eq!(
            rect.hit_test(&pos2(50.0, 5.1), 10.0),
            HitTarget::Inside // 在扩展区域外，但在矩形内
        );

        // stroke_width=20.0，tolerance=20，half=10
        // 扩展区域非常大
        assert_eq!(rect.hit_test(&pos2(50.0, 9.9), 20.0), HitTarget::TopEdge);

        // 注意：当扩展区域非常大时，甚至可能覆盖到矩形内部
        // 测试一个在矩形内部但在扩展区域内的点
        assert_eq!(rect.hit_test(&pos2(50.0, 8.0), 20.0), HitTarget::TopEdge);
    }

    // 测试边界条件和特殊情况
    #[test]
    fn test_hit_test_edge_cases() {
        let rect = test_rect();

        // 点在边上但x坐标超出矩形范围（但在扩展区域内）
        assert_eq!(
            rect.hit_test(&pos2(-2.9, 0.0), 1.0),
            HitTarget::TopLeftCorner // 同时在上边和左边
        );

        // 点在角的扩展区域边缘
        assert_eq!(
            rect.hit_test(&pos2(3.0, 3.0), 1.0),
            HitTarget::TopLeftCorner
        );

        // 点刚好在扩展区域边界上
        assert_eq!(
            rect.hit_test(&pos2(103.0, 3.0), 1.0),
            HitTarget::TopRightCorner
        );

        // 负坐标测试
        let rect2 = Rect::from_two_pos(pos2(-50.0, -50.0), pos2(50.0, 50.0));
        assert_eq!(rect2.hit_test(&pos2(0.0, -50.0), 1.0), HitTarget::TopEdge);
    }

    // 性能测试：测试多个点
    #[test]
    fn test_hit_test_multiple_points() {
        let rect = test_rect();
        let stroke_width = 1.0;

        // 创建测试点网格
        for x in -10..=110 {
            for y in -10..=60 {
                let x = x as f32;
                let y = y as f32;
                let point = pos2(x, y);
                let _result = rect.hit_test(&point, stroke_width);
                // 这里我们不验证具体结果，只是确保不会panic
            }
        }
    }
}
