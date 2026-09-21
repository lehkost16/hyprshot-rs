use crate::annotator::cursor::{Crosshair, CustomCursor};
use crate::annotator::rectangle_based::{HitTarget, HitTest};
use crate::annotator::{
    ActivationState, ActivationSupport, Annotation, AnnotationActivationSupport, AnnotationStyle,
    AnnotationToolCommon, AnnotatorState, ApplyExtraZoomFactor, DEFAULT_SIZE_FOR_SMALL_RECT,
    ExtraZoomFactorSupport, FillColorSupport, FontColorSupport, PainterExt, RemoveExtraZoomFactor,
    SharedAnnotatorState, SmallRect, StackTopAccessor, StrokeColorSupport, StrokeType,
    StrokeTypeSupport, StrokeWidthSupport, UnsubmittedAnnotationHandler, dash_len_for_dashed_line,
    gap_len_for_dashed_line, radius_for_dotted_line, spacing_for_dotted_line,
};
use crate::{declare_not_support_font_color, impl_stack_top_access_for};
use egui::PointerButton;
use egui::{Color32, CursorIcon, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Widget, vec2};
use std::cell::RefCell;
use std::rc::Weak;

#[derive(Debug, Copy, Clone)]
pub struct StraightLineStyle {
    /// 线条颜色和宽度
    stroke: Stroke,
    /// 线条类型
    stroke_type: StrokeType,
}

impl StrokeWidthSupport for StraightLineStyle {
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

impl StrokeColorSupport for StraightLineStyle {
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

impl StrokeTypeSupport for StraightLineStyle {
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

impl FillColorSupport for StraightLineStyle {
    fn supports_get_fill_color(&self) -> bool {
        false
    }

    fn fill_color(&self) -> Option<Color32> {
        None
    }

    fn supports_set_fill_color(&self) -> bool {
        false
    }

    fn set_fill_color(&mut self, _color: Color32) {
        unimplemented!()
    }
}

impl AnnotationStyle for StraightLineStyle {}

impl Default for StraightLineStyle {
    fn default() -> Self {
        Self {
            stroke: Stroke::new(1_f32, Color32::from_rgb(255, 0, 0)),
            stroke_type: StrokeType::SolidLine,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ArrowStyle {
    /// 线条颜色和宽度
    pub stroke: Stroke,
    /// 线条类型
    pub stroke_type: StrokeType,
}

impl StrokeWidthSupport for ArrowStyle {
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

impl StrokeColorSupport for ArrowStyle {
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

impl StrokeTypeSupport for ArrowStyle {
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

impl FillColorSupport for ArrowStyle {
    fn supports_get_fill_color(&self) -> bool {
        false
    }

    fn fill_color(&self) -> Option<Color32> {
        None
    }

    fn supports_set_fill_color(&self) -> bool {
        false
    }

    fn set_fill_color(&mut self, _color: Color32) {
        unimplemented!()
    }
}

impl AnnotationStyle for ArrowStyle {}

impl Default for ArrowStyle {
    fn default() -> Self {
        Self {
            stroke: Stroke::new(1_f32, Color32::from_rgb(255, 0, 0)),
            stroke_type: StrokeType::SolidLine,
        }
    }
}

/// 基于直线的标注
#[derive(Debug, Clone)]
pub struct StraightLineBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    /// 起点
    start_position: Pos2,
    /// 终点
    end_position: Pos2,
    /// 样式
    style: S,
    /// 激活状态
    activation: ActivationSupport,
}

impl<S> StraightLineBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    pub fn new(
        start_position: Pos2,
        end_position: Pos2,
        style: S,
        activation: ActivationSupport,
    ) -> Self {
        Self {
            start_position,
            end_position,
            style,
            activation,
        }
    }

    pub fn start_position(&self) -> &Pos2 {
        &self.start_position
    }

    pub fn end_position(&self) -> &Pos2 {
        &self.end_position
    }

    pub fn hit_test(&self, pointer_position: &Pos2) -> HitTargetForStraightLine {
        let stroke_width = self.stroke_width();
        let start_position = self.start_position();
        let hit_target = start_position
            .rect(DEFAULT_SIZE_FOR_SMALL_RECT.0, DEFAULT_SIZE_FOR_SMALL_RECT.1)
            .hit_test(pointer_position, stroke_width);
        if hit_target != HitTarget::Outside {
            return HitTargetForStraightLine::StartPoint;
        }
        let end_position = self.end_position();
        let hit_target = end_position
            .rect(DEFAULT_SIZE_FOR_SMALL_RECT.0, DEFAULT_SIZE_FOR_SMALL_RECT.1)
            .hit_test(pointer_position, stroke_width);
        if hit_target != HitTarget::Outside {
            return HitTargetForStraightLine::EndPoint;
        }
        HitTargetForStraightLine::Outside
    }
}

impl<S> StrokeWidthSupport for StraightLineBasedAnnotation<S>
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

impl<S> StrokeColorSupport for StraightLineBasedAnnotation<S>
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

impl<S> StrokeTypeSupport for StraightLineBasedAnnotation<S>
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

impl<S> FillColorSupport for StraightLineBasedAnnotation<S>
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

impl<S> AnnotationActivationSupport for StraightLineBasedAnnotation<S>
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

pub type StraightLineAnnotation = StraightLineBasedAnnotation<StraightLineStyle>;
pub type ArrowAnnotation = StraightLineBasedAnnotation<ArrowStyle>;

impl From<StraightLineAnnotation> for Annotation {
    fn from(val: StraightLineAnnotation) -> Self {
        Annotation::StraightLine(val)
    }
}

impl From<ArrowAnnotation> for Annotation {
    fn from(val: ArrowAnnotation) -> Self {
        Annotation::Arrow(val)
    }
}

impl Widget for &mut StraightLineAnnotation {
    fn ui(self, ui: &mut Ui) -> Response {
        let start_position = self
            .start_position()
            .apply_extra_zoom_factor_with_ctx(ui.ctx());
        let end_position = self
            .end_position()
            .apply_extra_zoom_factor_with_ctx(ui.ctx());
        let rect = Rect::from_two_pos(start_position, end_position);
        let response = ui.allocate_rect(rect, Sense::hover());
        let painter = ui.painter();
        // 缩放时保持线宽视觉不变
        let zoom = ui.ctx().extra_zoom_factor();
        let mut stroke = self.style.stroke;
        stroke.width /= zoom;
        match self.stroke_type() {
            StrokeType::SolidLine => {
                painter.line_segment([start_position, end_position], stroke);
            }
            StrokeType::DashedLine => {
                let dash_len = dash_len_for_dashed_line(stroke.width);
                let gap_len = gap_len_for_dashed_line(stroke.width);
                let shape =
                    Shape::dashed_line(&[start_position, end_position], stroke, dash_len, gap_len);
                painter.add(shape);
            }
            StrokeType::DottedLine => {
                let spacing = spacing_for_dotted_line(stroke.width);
                let radius = radius_for_dotted_line(stroke.width);
                let shape = Shape::dotted_line(
                    &[start_position, end_position],
                    stroke.color,
                    spacing,
                    radius,
                );
                painter.add(shape);
            }
        }

        if self.activation.is_active() {
            painter.small_rect(&start_position);
            painter.small_rect(&end_position);
        }
        response
    }
}

impl Widget for &mut ArrowAnnotation {
    fn ui(self, ui: &mut Ui) -> Response {
        let start_position = self
            .start_position()
            .apply_extra_zoom_factor_with_ctx(ui.ctx());
        let end_position = self
            .end_position()
            .apply_extra_zoom_factor_with_ctx(ui.ctx());
        let rect = Rect::from_two_pos(start_position, end_position);
        let response = ui.allocate_rect(rect, Sense::hover());
        let painter = ui.painter();

        let stroke = self.style.stroke;
        let color = stroke.color;
        let line_width = stroke.width;

        // 箭头方向向量（单位化）
        let dir = vec2(
            end_position.x - start_position.x,
            end_position.y - start_position.y,
        );
        let len = dir.length();

        if len < 1.0 {
            // 箭头太短则退化为圆点
            painter.circle_filled(end_position, line_width / 2.0, color);
        } else {
            let unit = dir / len;
            // 垂直于方向的单位向量
            let perp = vec2(-unit.y, unit.x);

            // 箭头三角的大小随线宽自适应（最小 10px，线宽越大箭头越大）
            let tip_len = (line_width * 4.0 + 8.0).min(len * 0.45);
            let tip_half_w = (line_width * 1.5 + 3.0).min(tip_len * 0.55);

            // 箭尖顶点（精确到 end_position）
            let tip = end_position;
            // 箭头底边中点（箭身终止在此，使箭身不穿出箭头三角）
            let base_center = tip - unit * tip_len;
            // 箭头两翼
            let wing_l = base_center + perp * tip_half_w;
            let wing_r = base_center - perp * tip_half_w;

            // 1. 箭身线段：从 start 到箭头底边中点
            painter.line_segment([start_position, base_center], stroke);

            // 2. 实心箭头三角，顶端精确在 end_position，不留缺口
            painter.add(Shape::convex_polygon(
                vec![tip, wing_l, wing_r],
                color,
                Stroke::NONE,
            ));
        }

        if self.activation().is_active() {
            painter.small_rect(&start_position);
            painter.small_rect(&end_position);
        }
        response
    }
}

pub struct StraightLineBasedToolState<S>
where
    S: AnnotationStyle + Default,
{
    /// 样式
    style: S,
    /// 当前的标注
    current_annotation: Option<StraightLineBasedAnnotation<S>>,
    /// 当拖动鼠标的时候需要执行的操作
    drag_action: DragAction,
}

impl<S> Default for StraightLineBasedToolState<S>
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

impl StackTopAccessor<StraightLineAnnotation> for AnnotatorState {
    fn peek_annotation<F, R>(&self, func: F) -> Option<R>
    where
        F: Fn(Option<&StraightLineAnnotation>) -> Option<R>,
    {
        // 从标注栈的栈顶中获取最近的一个直线标注
        let straight_line_annotation_on_stack_top =
            self.annotations_stack
                .last()
                .and_then(|annotation| match annotation {
                    Annotation::StraightLine(straight_line_annotation) => {
                        Some(straight_line_annotation)
                    }
                    _ => None,
                });
        func(straight_line_annotation_on_stack_top)
    }

    fn peek_annotation_mut<F, R>(&mut self, func: F) -> Option<R>
    where
        F: Fn(Option<&mut StraightLineAnnotation>) -> Option<R>,
    {
        // 从标注栈的栈顶中获取最近的一个直线标注
        let straight_line_annotation_on_stack_top =
            self.annotations_stack
                .last_mut()
                .and_then(|annotation| match annotation {
                    Annotation::StraightLine(straight_line_annotation) => {
                        Some(straight_line_annotation)
                    }
                    _ => None,
                });
        func(straight_line_annotation_on_stack_top)
    }

    fn pop_annotation(&mut self) -> Option<StraightLineAnnotation> {
        // 从标注栈的栈顶中获取最近的一个直线标注
        self.annotations_stack
            .pop()
            .and_then(|annotation| match annotation {
                Annotation::StraightLine(straight_line_annotation) => {
                    Some(straight_line_annotation)
                }
                _ => None,
            })
    }
}

impl StackTopAccessor<ArrowAnnotation> for AnnotatorState {
    fn peek_annotation<F, R>(&self, func: F) -> Option<R>
    where
        F: Fn(Option<&ArrowAnnotation>) -> Option<R>,
    {
        // 从标注栈的栈顶中获取最近的一个箭头标注
        let arrow_annotation_on_stack_top =
            self.annotations_stack
                .last()
                .and_then(|annotation| match annotation {
                    Annotation::Arrow(arrow_annotation) => Some(arrow_annotation),
                    _ => None,
                });
        func(arrow_annotation_on_stack_top)
    }

    fn peek_annotation_mut<F, R>(&mut self, func: F) -> Option<R>
    where
        F: Fn(Option<&mut ArrowAnnotation>) -> Option<R>,
    {
        // 从标注栈的栈顶中获取最近的一个箭头标注
        let arrow_annotation_on_stack_top =
            self.annotations_stack
                .last_mut()
                .and_then(|annotation| match annotation {
                    Annotation::Arrow(arrow_annotation) => Some(arrow_annotation),
                    _ => None,
                });
        func(arrow_annotation_on_stack_top)
    }

    fn pop_annotation(&mut self) -> Option<ArrowAnnotation> {
        // 从标注栈的栈顶中获取最近的一个箭头标注
        self.annotations_stack
            .pop()
            .and_then(|annotation| match annotation {
                Annotation::Arrow(arrow_annotation) => Some(arrow_annotation),
                _ => None,
            })
    }
}

pub struct StraightLineBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    annotator_state: Weak<RefCell<AnnotatorState>>,
    tool_state: StraightLineBasedToolState<S>,
}

impl<S> StraightLineBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    pub fn new(annotator_state: Weak<RefCell<AnnotatorState>>) -> StraightLineBasedTool<S> {
        let tool_state = StraightLineBasedToolState::default();
        Self {
            annotator_state,
            tool_state,
        }
    }
}

impl<S> StrokeWidthSupport for StraightLineBasedTool<S>
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

impl<S> StrokeColorSupport for StraightLineBasedTool<S>
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
    }
}

impl<S> StrokeTypeSupport for StraightLineBasedTool<S>
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
    }
}

impl<S> FillColorSupport for StraightLineBasedTool<S>
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
    }
}

impl<S> AnnotationToolCommon for StraightLineBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    fn annotator_state(&self) -> SharedAnnotatorState {
        self.annotator_state.upgrade().unwrap()
    }
}

impl<S> UnsubmittedAnnotationHandler for StraightLineBasedTool<S> where S: AnnotationStyle + Default {}

pub type StraightLineTool = StraightLineBasedTool<StraightLineStyle>;
pub type ArrowTool = StraightLineBasedTool<ArrowStyle>;

declare_not_support_font_color!(StraightLineTool, ArrowTool);

impl_stack_top_access_for!(StraightLineTool=>StraightLineAnnotation, ArrowTool=>ArrowAnnotation);

macro_rules! impl_widget_for {
    ($($tool:ty=>$annotation:ty),*) => {
        $(


impl $tool {
    fn update_cursor_icon(&self, ui: &mut Ui) {
        let Some(raw_pointer_pos) = ui.ctx().pointer_hover_pos() else {
            return;
        };

        // 从标注栈的栈顶中获取最近的一个标注
        let hit_target =
            self.peek_annotation(|annotation_on_stack_top: Option<&$annotation>| {
                // 判断当前鼠标是否位于此标注上
                let pointer_pos = raw_pointer_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
                match annotation_on_stack_top {
                    None => None,
                    Some(annotation) => Some(annotation.hit_test(&pointer_pos)),
                }
            });

        if let Some(hit_target) = hit_target {
            let cursor_icon = hit_target.get_cursor();
            if let Some(cursor_icon) = cursor_icon {
                ui.ctx().set_cursor_icon(cursor_icon);
            } else {
                ui.ctx().set_cursor_icon(CursorIcon::None);
                Crosshair::new(raw_pointer_pos, Color32::RED, self.stroke_width())
                    .paint_with(ui.painter());
            }
        } else {
            ui.ctx().set_cursor_icon(CursorIcon::None);
            Crosshair::new(
                raw_pointer_pos,
                Color32::RED,
                self.tool_state.style.stroke.width,
            )
            .paint_with(ui.painter());
        }
    }
}

impl Widget for &mut $tool {
    fn ui(self, ui: &mut Ui) -> Response {
        let sense_area = Rect::from_min_size(Pos2::ZERO, ui.available_size());
        let response = ui.allocate_rect(sense_area, Sense::click_and_drag());

        let Some(pointer_pos) = ui.ctx().pointer_hover_pos() else {
            return response;
        };
        let pointer_pos = pointer_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());

        // 检测鼠标碰撞并绘制光标
        self.update_cursor_icon(ui);

        if response.drag_started_by(PointerButton::Primary) {
            // 拖动开始
            let drag_started_pos = ui.ctx().input(|i| i.pointer.press_origin()).unwrap();
            let drag_started_pos = drag_started_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
            let hit_target =
                self.peek_annotation(|annotation_on_stack_top: Option<&$annotation>| {
                    // 判断当前鼠标是否位于此标注上
                    match annotation_on_stack_top {
                        None => None,
                        Some(annotation) => Some(annotation.hit_test(&drag_started_pos)),
                    }
                });

            match hit_target {
                Some(hit_target) if hit_target != HitTargetForStraightLine::Outside => {
                    let support_activate = self
                        .peek_annotation(|annotation_on_stack_top: Option<&$annotation>| {
                            Some(
                                annotation_on_stack_top
                                    .unwrap()
                                    .activation()
                                    .supports_activate(),
                            )
                        })
                        .unwrap();

                    if support_activate {
                        // 调整现有的标注
                        let mut annotation = self.pop_annotation().unwrap();
                        annotation.activation_mut().activate();
                        self.tool_state.current_annotation = Some(annotation);
                        self.tool_state.drag_action = hit_target.get_drag_action();
                    }
                }
                Some(_hit_target) => {
                    self.peek_annotation_mut(|mut annotation_on_stack_top| {
                        // 把栈顶的标注设为非激活状态
                        annotation_on_stack_top
                            .as_mut()
                            .unwrap()
                            .activation_mut()
                            .deactivate();
                        None::<()>
                    });
                }
                _ => {}
            }
        } else if response.clicked() {
            self.peek_annotation_mut(|mut annotation_on_stack_top| {
                // 把栈顶的标注设为非激活状态
                if let Some(annotation) = annotation_on_stack_top.as_mut() {
                    annotation.activation_mut().deactivate();
                }

                None::<()>
            });
        }

        if response.dragged_by(PointerButton::Primary) {
            // 拖动中
            if let Some(annotation) = &mut self.tool_state.current_annotation {
                match self.tool_state.drag_action {
                    DragAction::AdjustStartPoint => {
                        annotation.start_position = pointer_pos;
                    }
                    DragAction::AdjustEndPoint => {
                        annotation.end_position = pointer_pos;
                    }
                    DragAction::None => {
                        let drag_started_pos =
                            ui.ctx().input(|i| i.pointer.press_origin()).unwrap();
                        let drag_started_pos = drag_started_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
                        annotation.start_position = drag_started_pos;
                        annotation.end_position = pointer_pos;
                    }
                }
                ui.add(annotation);
            } else {
                let drag_started_pos = ui.ctx().input(|i| i.pointer.press_origin()).unwrap();
                let drag_started_pos = drag_started_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
                let mut annotation = <$annotation>::new(
                    drag_started_pos,
                    pointer_pos,
                    self.tool_state.style,
                    ActivationSupport::Supported(ActivationState::new(true)),
                );
                self.tool_state.current_annotation = Some(annotation.clone());
                self.tool_state.drag_action = DragAction::None;
                ui.add(&mut annotation);
            }
        }

        if response.drag_stopped_by(PointerButton::Primary) {
            // 拖动结束
            self.tool_state.drag_action = DragAction::None;
            if let Some(mut current_annotation) = self.tool_state.current_annotation.take() {
                ui.add(&mut current_annotation);
                self.annotator_state
                    .upgrade()
                    .unwrap()
                    .borrow_mut()
                    .submit_annotation(current_annotation.into());
            }
            ui.ctx().request_repaint();
        }
        response
    }
}


        )*
    };
}

impl_widget_for!(StraightLineTool => StraightLineAnnotation, ArrowTool => ArrowAnnotation);

pub enum DragAction {
    AdjustStartPoint,
    AdjustEndPoint,
    None,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum HitTargetForStraightLine {
    Outside,
    StartPoint,
    EndPoint,
}

impl HitTargetForStraightLine {
    fn get_drag_action(&self) -> DragAction {
        match self {
            HitTargetForStraightLine::Outside => DragAction::None,
            HitTargetForStraightLine::StartPoint => DragAction::AdjustStartPoint,
            HitTargetForStraightLine::EndPoint => DragAction::AdjustEndPoint,
        }
    }

    fn get_cursor(&self) -> Option<CursorIcon> {
        match self {
            HitTargetForStraightLine::Outside => None,
            HitTargetForStraightLine::StartPoint => Some(CursorIcon::ResizeNwSe),
            HitTargetForStraightLine::EndPoint => Some(CursorIcon::ResizeNwSe),
        }
    }
}
