use crate::annotator::cursor::{Circle, Crosshair, CustomCursor};
use crate::annotator::{
    ActivationSupport, Annotation, AnnotationActivationSupport, AnnotationStyle,
    AnnotationToolCommon, AnnotatorState, ApplyExtraZoomFactor, FillColorSupport, FontColorSupport,
    RemoveExtraZoomFactor, SharedAnnotatorState, StrokeColorSupport, StrokeType, StrokeTypeSupport,
    StrokeWidthSupport, UnsubmittedAnnotationHandler,
};
use crate::declare_not_support_font_color;
use egui::{
    Color32, CursorIcon, PointerButton, Pos2, Rect, Response, Sense, Stroke, Ui, Widget, pos2,
};
use std::cell::RefCell;
use std::rc::Weak;

#[derive(Debug, Copy, Clone)]
pub struct PencilStyle {
    /// 线条颜色和宽度
    stroke: Stroke,
    /// 线条类型
    stroke_type: StrokeType,
}

impl StrokeWidthSupport for PencilStyle {
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

impl StrokeColorSupport for PencilStyle {
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

impl StrokeTypeSupport for PencilStyle {
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

impl FillColorSupport for PencilStyle {
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

impl AnnotationStyle for PencilStyle {}

impl Default for PencilStyle {
    fn default() -> Self {
        Self {
            stroke: Stroke::new(1_f32, Color32::from_rgb(255, 0, 0)),
            stroke_type: StrokeType::SolidLine,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct MarkerPenStyle {
    /// 线条颜色和宽度
    pub stroke: Stroke,
    /// 线条类型
    pub stroke_type: StrokeType,
}

impl StrokeWidthSupport for MarkerPenStyle {
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

impl StrokeColorSupport for MarkerPenStyle {
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

impl StrokeTypeSupport for MarkerPenStyle {
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

impl FillColorSupport for MarkerPenStyle {
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

impl AnnotationStyle for MarkerPenStyle {}

impl Default for MarkerPenStyle {
    fn default() -> Self {
        Self {
            stroke: Stroke::new(20_f32, Color32::from_rgba_unmultiplied(255, 0, 0, 76)),
            stroke_type: StrokeType::SolidLine,
        }
    }
}

/// 基于自由曲线的标注
#[derive(Debug, Clone)]
pub struct FreeLineBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    /// 构成曲线的点
    points: Vec<Pos2>,
    /// 样式
    style: S,
    /// 激活状态
    activation: ActivationSupport,
}

impl<S> FreeLineBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    pub fn new(points: Vec<Pos2>, style: S, activation: ActivationSupport) -> Self {
        Self {
            points,
            style,
            activation,
        }
    }

    pub fn points(&self) -> &Vec<Pos2> {
        &self.points
    }
}

impl<S> StrokeWidthSupport for FreeLineBasedAnnotation<S>
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

impl<S> StrokeColorSupport for FreeLineBasedAnnotation<S>
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

impl<S> StrokeTypeSupport for FreeLineBasedAnnotation<S>
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

impl<S> FillColorSupport for FreeLineBasedAnnotation<S>
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

impl<S> AnnotationActivationSupport for FreeLineBasedAnnotation<S>
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

pub type PencilAnnotation = FreeLineBasedAnnotation<PencilStyle>;
pub type MarkerPenAnnotation = FreeLineBasedAnnotation<MarkerPenStyle>;

impl From<PencilAnnotation> for Annotation {
    fn from(val: PencilAnnotation) -> Self {
        Annotation::Pencil(val)
    }
}

impl From<MarkerPenAnnotation> for Annotation {
    fn from(val: MarkerPenAnnotation) -> Self {
        Annotation::MarkerPen(val)
    }
}

impl<S> Widget for &mut FreeLineBasedAnnotation<S>
where
    S: AnnotationStyle,
{
    fn ui(self, ui: &mut Ui) -> Response {
        let mut left = None;
        let mut top = None;
        let mut right = None;
        let mut bottom = None;
        let points = self.points().apply_extra_zoom_factor_with_ctx(ui.ctx());
        for point in &points {
            if let Some(ref mut l) = left {
                if point.x < *l {
                    *l = point.x;
                }
            } else {
                left = Some(point.x);
            }

            if let Some(ref mut r) = right {
                if point.x > *r {
                    *r = point.x;
                }
            } else {
                right = Some(point.x);
            }

            if let Some(ref mut t) = top {
                if point.y < *t {
                    *t = point.y;
                }
            } else {
                top = Some(point.y);
            }

            if let Some(ref mut b) = bottom {
                if point.y > *b {
                    *b = point.y;
                }
            } else {
                bottom = Some(point.y);
            }
        }
        let rect = Rect::from_two_pos(
            pos2(left.unwrap(), top.unwrap()),
            pos2(right.unwrap(), bottom.unwrap()),
        );
        let response = ui.allocate_rect(rect, Sense::hover());
        let painter = ui.painter();
        painter.line(
            points,
            Stroke::new(self.style.stroke_width(), self.style.stroke_color()),
        );
        response
    }
}

pub struct FreeLineBasedToolState<S>
where
    S: AnnotationStyle + Default,
{
    /// 样式
    style: S,
    /// 当前的标注
    current_annotation: Option<FreeLineBasedAnnotation<S>>,
    /// 当拖动鼠标的时候需要执行的操作
    #[allow(dead_code)]
    drag_action: DragAction,
}

impl<S> Default for FreeLineBasedToolState<S>
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

pub struct FreeLineBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    annotator_state: Weak<RefCell<AnnotatorState>>,
    tool_state: FreeLineBasedToolState<S>,
}

impl<S> FreeLineBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    pub fn new(annotator_state: Weak<RefCell<AnnotatorState>>) -> FreeLineBasedTool<S> {
        let tool_state = FreeLineBasedToolState::default();
        Self {
            annotator_state,
            tool_state,
        }
    }
}

impl<S> StrokeWidthSupport for FreeLineBasedTool<S>
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

impl<S> StrokeColorSupport for FreeLineBasedTool<S>
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

impl<S> StrokeTypeSupport for FreeLineBasedTool<S>
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

impl<S> FillColorSupport for FreeLineBasedTool<S>
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

impl<S> AnnotationToolCommon for FreeLineBasedTool<S>
where
    S: AnnotationStyle + Default,
{
    fn annotator_state(&self) -> SharedAnnotatorState {
        self.annotator_state.upgrade().unwrap()
    }
}

impl<S> UnsubmittedAnnotationHandler for FreeLineBasedTool<S> where S: AnnotationStyle + Default {}

pub type PencilTool = FreeLineBasedTool<PencilStyle>;
pub type MarkerPenTool = FreeLineBasedTool<MarkerPenStyle>;

declare_not_support_font_color!(PencilTool, MarkerPenTool);

impl PencilTool {
    fn update_cursor_icon(&self, ui: &mut Ui) {
        let Some(pointer_pos) = ui.ctx().pointer_hover_pos() else {
            return;
        };

        ui.ctx().set_cursor_icon(CursorIcon::None);
        // 绘制自定义光标
        Crosshair::new(
            pointer_pos,
            Color32::RED,
            self.tool_state.style.stroke_width(),
        )
        .paint_with(ui.painter());
    }
}

impl MarkerPenTool {
    fn update_cursor_icon(&self, ui: &mut Ui) {
        let Some(pointer_pos) = ui.ctx().pointer_hover_pos() else {
            return;
        };

        ui.ctx().set_cursor_icon(CursorIcon::None);
        // 绘制自定义光标
        Circle::new(
            pointer_pos,
            Color32::from_rgba_unmultiplied(
                self.tool_state.style.stroke_color().r(),
                self.tool_state.style.stroke_color().g(),
                self.tool_state.style.stroke_color().b(),
                200,
            ),
            self.tool_state.style.stroke_width(),
        )
        .paint_with(ui.painter());
    }
}

macro_rules! impl_widget_for {
    ($($tool:ty=>$annotation:ty),*) => {
        $(
impl Widget for &mut $tool  {
    fn ui(self, ui: &mut Ui) -> Response {
        let sense_area = Rect::from_min_size(Pos2::ZERO, ui.available_size());
        let response = ui.allocate_rect(sense_area, Sense::click_and_drag());

        let Some(pointer_pos) = ui.ctx().pointer_hover_pos() else {
            return response;
        };

        // 检测鼠标碰撞并绘制光标
        self.update_cursor_icon(ui);

        if response.drag_started_by(PointerButton::Primary){
            // 拖动开始
            let drag_started_pos = ui.ctx().input(|i| i.pointer.press_origin()).unwrap();
            let drag_started_pos = drag_started_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
            let points = vec![drag_started_pos];
            let annotation = <$annotation>::new(points, self.tool_state.style, ActivationSupport::NotSupported);
            self.tool_state.current_annotation = Some(annotation);
        }

        if response.dragged_by(PointerButton::Primary) {
            // 拖动中
            if let Some(annotation) = self.tool_state.current_annotation.as_mut() {
                let pointer_pos = pointer_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
                let straight_marker = stringify!($tool) == "MarkerPenTool"
                    && self
                        .annotator_state
                        .upgrade()
                        .is_some_and(|state| state.borrow().marker_pen_straight_mode);
                if straight_marker {
                    if annotation.points.len() == 1 {
                        annotation.points.push(pointer_pos);
                    } else {
                        *annotation.points.last_mut().unwrap() = pointer_pos;
                    }
                    ui.add(annotation);
                    return response;
                }
                // 过滤高频、微小抖动采样，避免半透明笔触在同一位置反复叠加变深。
                let min_distance = (self.tool_state.style.stroke_width() * 0.35).max(1.0);
                let should_append = annotation
                    .points
                    .last()
                    .is_none_or(|last| last.distance(pointer_pos) >= min_distance);
                if should_append {
                    annotation.points.push(pointer_pos);
                }
                ui.add(annotation);
            }
        }

        if response.drag_stopped_by(PointerButton::Primary) {
            // 拖动结束
            if let Some(mut annotation) = self.tool_state.current_annotation.take() {
                let pointer_pos = pointer_pos.remove_extra_zoom_factor_with_ctx(ui.ctx());
                if annotation
                    .points
                    .last()
                    .is_none_or(|last| last.distance(pointer_pos) >= 1.0)
                {
                    annotation.points.push(pointer_pos);
                }
                ui.add(&mut annotation);

                self.annotator_state
                    .upgrade()
                    .unwrap()
                    .borrow_mut()
                    .submit_annotation(annotation.into());
            }
            ui.ctx().request_repaint();
        }
        response
    }
}

   )*
  }
}

impl_widget_for!(PencilTool=>PencilAnnotation, MarkerPenTool=>MarkerPenAnnotation);

enum DragAction {
    AdjustStartPoint,
    AdjustEndPoint,
    None,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[allow(dead_code)]
enum HitTargetForFreeLine {
    Outside,
    StartPoint,
    EndPoint,
}

impl HitTargetForFreeLine {
    #[allow(dead_code)]
    fn get_drag_action(&self) -> DragAction {
        match self {
            HitTargetForFreeLine::Outside => DragAction::None,
            HitTargetForFreeLine::StartPoint => DragAction::AdjustStartPoint,
            HitTargetForFreeLine::EndPoint => DragAction::AdjustEndPoint,
        }
    }

    #[allow(dead_code)]
    fn get_cursor(&self) -> Option<CursorIcon> {
        match self {
            HitTargetForFreeLine::Outside => None,
            HitTargetForFreeLine::StartPoint => Some(CursorIcon::ResizeNwSe),
            HitTargetForFreeLine::EndPoint => Some(CursorIcon::ResizeNwSe),
        }
    }
}
