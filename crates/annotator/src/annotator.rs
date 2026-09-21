mod cursor;
pub mod drop_down_box;
pub mod free_line_based;
pub mod image_based;
pub mod rectangle_based;
pub mod serial_number;
pub mod straight_line_based;
pub(crate) mod svg_button;
pub mod text;
pub mod watermark;

use crate::annotator::free_line_based::{
    MarkerPenAnnotation, MarkerPenTool, PencilAnnotation, PencilTool,
};
use crate::annotator::image_based::{
    BackgroundImageProvider, BackgroundImageWithAnnotationsProvider, BlurAnnotation, BlurTool,
    EraserAnnotation, EraserTool, MosaicAnnotation, MosaicTool,
};
use crate::annotator::rectangle_based::{
    EllipseAnnotation, EllipseTool, RectangleAnnotation, RectangleTool,
};
use crate::annotator::serial_number::{SerialNumberAnnotation, SerialNumberTool};
use crate::annotator::straight_line_based::{
    ArrowAnnotation, ArrowTool, StraightLineAnnotation, StraightLineTool,
};
use crate::annotator::text::{TextAnnotation, TextTool};
use crate::dpi::{LogicalSize, Pixel};
use crate::egui_off_screen_render::EguiOffScreenRender;
use crate::global::Global;
use crate::view::ViewId;
use delegate::delegate;
use egui::{
    Color32, Context, Id, Painter, Pos2, Rect, Response, Shape, Stroke, StrokeKind, TextureHandle,
    Ui, Vec2, Widget, pos2, vec2,
};
use image::RgbaImage;
use spire_enum::prelude::{delegate_impl, delegated_enum};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::oneshot::Receiver;

/// 线条类型（实线、虚线、点线）
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum StrokeType {
    /// 实线
    SolidLine,

    /// 虚线
    DashedLine,

    /// 点线
    DottedLine,
}

/// 线条宽度
pub trait StrokeWidthSupport {
    /// 是否支持获取线条宽度
    fn supports_get_stroke_width(&self) -> bool;
    /// 获取线条宽度
    fn stroke_width(&self) -> f32;
    /// 是否支持设置线条宽度
    fn supports_set_stroke_width(&self) -> bool;
    /// 设置线条宽度
    fn set_stroke_width(&mut self, stroke_width: f32);
}

/// 线条颜色
pub trait StrokeColorSupport {
    /// 是否支持获取线条颜色
    fn supports_get_stroke_color(&self) -> bool;
    /// 获取线条颜色
    fn stroke_color(&self) -> Color32;
    /// 是否支持设置线条颜色
    fn supports_set_stroke_color(&self) -> bool;
    /// 设置线条颜色
    fn set_stroke_color(&mut self, color: Color32);
}

/// 线条类型
pub trait StrokeTypeSupport {
    /// 是否支持获取线条类型
    fn supports_get_stroke_type(&self) -> bool;
    /// 线条类型
    fn stroke_type(&self) -> StrokeType;
    /// 是否支持设置线条类型
    fn supports_set_stroke_type(&self) -> bool;
    /// 设置线条类型
    fn set_stroke_type(&mut self, stroke_type: StrokeType);
}

/// 填充颜色
pub trait FillColorSupport {
    /// 是否支持获取填充颜色
    fn supports_get_fill_color(&self) -> bool;
    /// 填充颜色
    fn fill_color(&self) -> Option<Color32>;
    /// 是否支持设置填充颜色
    fn supports_set_fill_color(&self) -> bool;
    /// 设置填充颜色
    fn set_fill_color(&mut self, color: Color32);
}

/// 字体颜色
pub trait FontColorSupport {
    /// 是否支持获取字体颜色
    fn supports_get_font_color(&self) -> bool;
    /// 字体颜色
    fn font_color(&self) -> Color32;
    /// 是否支持设置字体颜色
    fn supports_set_font_color(&self) -> bool;
    /// 设置字体颜色
    fn set_font_color(&mut self, color: Color32);
}

#[macro_export]
macro_rules! declare_not_support_stroke_width {
    ($($type_name:ty),*) => {
        $(

            impl StrokeWidthSupport for $type_name {
                /// 是否支持获取线条宽度
                fn supports_get_stroke_width(&self) -> bool {
                    false
                }

                /// 获取线条宽度
                fn stroke_width(&self) -> f32 {
                    unimplemented!()
                }

                /// 是否支持设置线条宽度
                fn supports_set_stroke_width(&self) -> bool {
                    false
                }
                /// 设置线条宽度
                fn set_stroke_width(&mut self, _stroke_width: f32) {
                    unimplemented!()
                }
            }

        )*
    }
}

#[macro_export]
macro_rules! declare_not_support_stroke_color {
    ($($type_name:ty),*) => {
        $(

            impl StrokeColorSupport for $type_name {
                /// 是否支持获取线条颜色
                fn supports_get_stroke_color(&self) -> bool{
                    false
                }
                /// 获取线条颜色
                fn stroke_color(&self) -> Color32 {
                    unimplemented!()
                }
                /// 是否支持设置线条颜色
                fn supports_set_stroke_color(&self) -> bool{
                    false
                }
                /// 设置线条颜色
                fn set_stroke_color(&mut self, _color: Color32){
                    unimplemented!()
                }
            }

        )*
    }
}

#[macro_export]
macro_rules! declare_not_support_stroke_type {
    ($($type_name:ty),*) => {
        $(

            impl StrokeTypeSupport for $type_name {
                /// 是否支持获取线条类型
                fn supports_get_stroke_type(&self) -> bool{
                    false
                }
                /// 线条类型
                fn stroke_type(&self) -> StrokeType{
                    unimplemented!()
                }
                /// 是否支持设置线条类型
                fn supports_set_stroke_type(&self) -> bool{
                    false
                }
                /// 设置线条类型
                fn set_stroke_type(&mut self, _stroke_type: StrokeType){
                    unimplemented!()
                }
            }

        )*
    }
}

#[macro_export]
macro_rules! declare_not_support_fill_color {
    ($($type_name:ty),*) => {
        $(

            impl FillColorSupport for $type_name {
                /// 是否支持获取填充颜色
                fn supports_get_fill_color(&self) -> bool{
                    false
                }
                /// 填充颜色
                fn fill_color(&self) -> Option<Color32>{
                    unimplemented!()
                }
                /// 是否支持设置填充颜色
                fn supports_set_fill_color(&self) -> bool{
                    false
                }
                /// 设置填充颜色
                fn set_fill_color(&mut self, _color: Color32){
                    unimplemented!()
                }
            }

        )*
    }
}

#[macro_export]
macro_rules! declare_not_support_font_color {
    ($($type_name:ty),*) => {
        $(

            impl FontColorSupport for $type_name {
                /// 是否支持获取字体颜色
                fn supports_get_font_color(&self) -> bool{
                    false
                }
                /// 字体颜色
                fn font_color(&self) -> Color32 {
                    unimplemented!()
                }
                /// 是否支持设置字体颜色
                fn supports_set_font_color(&self) -> bool{
                    false
                }
                /// 设置字体颜色
                fn set_font_color(&mut self, _color: Color32){
                    unimplemented!()
                }
            }

        )*
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivationState {
    pub active: bool,
}

impl ActivationState {
    pub fn new(active: bool) -> Self {
        ActivationState { active }
    }

    pub fn active() -> Self {
        Self::new(true)
    }

    pub fn not_active() -> Self {
        Self::new(false)
    }
}

#[derive(Clone, Debug)]
pub enum ActivationSupport {
    NotSupported,
    Supported(ActivationState),
}

impl ActivationSupport {
    /// 当前的标注是否支持激活
    /// 某些类型的标注可能不支持激活，那么和激活相关的逻辑、依赖激活状态的逻辑将与此标注无关(例如：一个标注被添加到栈顶后将不再可编辑)
    pub fn supports_activate(&self) -> bool {
        matches!(self, ActivationSupport::Supported(_))
    }

    /// 激活此标注
    pub fn activate(&mut self) {
        match self {
            ActivationSupport::Supported(state) => state.active = true,
            _ => unimplemented!(),
        }
    }

    /// 取消激活此标注
    pub fn deactivate(&mut self) {
        if let ActivationSupport::Supported(state) = self {
            state.active = false;
        }
    }

    /// 此标注是否处于激活状态
    pub fn is_active(&self) -> bool {
        match self {
            ActivationSupport::Supported(state) => state.active,
            _ => false,
        }
    }
}

pub trait AnnotationStyle:
    StrokeWidthSupport + StrokeColorSupport + StrokeTypeSupport + FillColorSupport + Default
{
}

pub trait AnnotationActivationSupport {
    fn activation(&self) -> &ActivationSupport;
    fn activation_mut(&mut self) -> &mut ActivationSupport;
}

pub trait AnnotationToolCommon:
    StrokeWidthSupport + StrokeColorSupport + StrokeTypeSupport + FillColorSupport
{
    fn annotator_state(&self) -> SharedAnnotatorState;
}

pub trait WheelHandler {
    fn handle_wheel_event(&mut self, ui: &mut Ui) {
        // 滚动鼠标滚轮调整线条大小
        let scroll_delta = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0. {
            ui.memory_mut(|memory| {
                let step_threshold = 9f32;
                let value = memory
                    .data
                    .get_temp_mut_or_default::<f32>(Id::from("wheel-scroll-value-accumulate"));
                *value += scroll_delta;

                while *value >= step_threshold {
                    *value -= step_threshold;
                    self.on_scroll_delta_changed(step_threshold);
                }

                while *value <= -step_threshold {
                    *value += step_threshold;
                    self.on_scroll_delta_changed(-step_threshold);
                }
            });
        }
    }

    fn on_scroll_delta_changed(&mut self, value: f32);
}

#[macro_export]
macro_rules! impl_stroke_width_handler_for {
    ($($tool:ty=>$max_stroke_width:expr),*) => {
        $(

        impl WheelHandler for $tool {
            fn on_scroll_delta_changed(&mut self, value: f32) {
                if !self.supports_set_stroke_width() {
                    return;
                }
                let mut stroke_width = self.stroke_width();
                if value > 0. {
                    if stroke_width > 1. {
                        stroke_width -= 1.;
                    }
                }else {
                    if stroke_width < $max_stroke_width {
                        stroke_width += 1.0;
                    }
                }
                self.set_stroke_width(stroke_width);
                self.peek_annotation_mut(|option| {
                    match option {
                        Some(annotation) => {
                            if annotation.activation.is_active() && annotation.supports_get_stroke_width() {
                                annotation.set_stroke_width(stroke_width);
                            }
                        }
                        _ => ()
                    }
                    NOTHING
                });
            }
        }

        )*
    }
}

#[derive(Clone)]
#[delegated_enum]
pub enum Annotation {
    /// 矩形
    Rectangle(RectangleAnnotation),

    /// 椭圆
    Ellipse(EllipseAnnotation),

    /// 直线
    StraightLine(StraightLineAnnotation),

    /// 箭头
    Arrow(ArrowAnnotation),

    /// 铅笔
    Pencil(PencilAnnotation),

    /// 记号笔
    MarkerPen(MarkerPenAnnotation),

    /// 马赛克
    Mosaic(MosaicAnnotation),

    /// 模糊
    Blur(BlurAnnotation),

    /// 文本
    Text(TextAnnotation),

    /// 序号
    SerialNumber(SerialNumberAnnotation),

    /// 水印
    // Watermark(WaterMarkState),

    /// 橡皮擦
    Eraser(EraserAnnotation),
}

impl Annotation {
    /// 检测当前标注是否是由指定工具创建的
    pub fn was_created_by(&mut self, tool: &AnnotationTool) -> bool {
        let tool_name = tool.tool_name();
        match self {
            Annotation::Rectangle(_) => tool_name == ToolName::Rectangle,
            Annotation::Ellipse(_) => tool_name == ToolName::Ellipse,
            Annotation::StraightLine(_) => tool_name == ToolName::StraightLine,
            Annotation::Arrow(_) => tool_name == ToolName::Arrow,
            Annotation::Pencil(_) => tool_name == ToolName::Pencil,
            Annotation::MarkerPen(_) => tool_name == ToolName::MarkerPen,
            Annotation::Mosaic(_) => tool_name == ToolName::Mosaic,
            Annotation::Blur(_) => tool_name == ToolName::Blur,
            Annotation::Text(_) => tool_name == ToolName::Text,
            Annotation::SerialNumber(_) => tool_name == ToolName::SerialNumber,
            // Annotation::Watermark(_) => tool_name == ToolName::Watermark,
            Annotation::Eraser(_) => tool_name == ToolName::Eraser,
        }
    }

    delegate! {
        to match self {
            Annotation::Rectangle(inner) => inner,
            Annotation::Ellipse(inner) => inner,
            Annotation::StraightLine(inner) => inner,
            Annotation::Arrow(inner) => inner,
            Annotation::Pencil(inner) => inner,
            Annotation::MarkerPen(inner) => inner,
            Annotation::Mosaic(inner) => inner,
            Annotation::Blur(inner) => inner,
            Annotation::Text(inner) => inner,
            Annotation::SerialNumber(inner) => inner,
            // Annotation::Watermark(inner) => inner,
            Annotation::Eraser(inner) => inner,
        } {
            pub fn activation(&self) -> &ActivationSupport;
            pub fn activation_mut(&mut self) -> &mut ActivationSupport;
        }
    }
}

#[delegate_impl]
impl StrokeWidthSupport for Annotation {
    fn supports_get_stroke_width(&self) -> bool;
    fn stroke_width(&self) -> f32;
    fn supports_set_stroke_width(&self) -> bool;
    fn set_stroke_width(&mut self, stroke_width: f32);
}

#[delegate_impl]
impl StrokeColorSupport for Annotation {
    fn supports_get_stroke_color(&self) -> bool;
    fn stroke_color(&self) -> Color32;
    fn supports_set_stroke_color(&self) -> bool;
    fn set_stroke_color(&mut self, color: Color32);
}
#[delegate_impl]
impl StrokeTypeSupport for Annotation {
    fn supports_get_stroke_type(&self) -> bool;
    fn stroke_type(&self) -> StrokeType;
    fn supports_set_stroke_type(&self) -> bool;
    fn set_stroke_type(&mut self, stroke_type: StrokeType);
}

#[delegate_impl]
impl FillColorSupport for Annotation {
    fn supports_get_fill_color(&self) -> bool;
    fn fill_color(&self) -> Option<Color32>;
    fn supports_set_fill_color(&self) -> bool;
    fn set_fill_color(&mut self, color: Color32);
}

#[delegate_impl]
impl Widget for &mut Annotation {
    fn ui(self, ui: &mut Ui) -> Response;
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum ToolName {
    /// 矩形
    Rectangle,

    /// 椭圆
    Ellipse,

    /// 直线
    StraightLine,

    /// 箭头
    Arrow,

    /// 铅笔
    Pencil,

    /// 记号笔
    MarkerPen,

    /// 马赛克
    Mosaic,

    /// 模糊
    Blur,

    /// 文本
    Text,

    /// 序号
    SerialNumber,

    /// 水印
    Watermark,

    /// 橡皮擦
    Eraser,
}

impl ToolName {
    pub fn from_config_value(value: &str) -> Option<Self> {
        match value.trim() {
            "Rectangle" => Some(Self::Rectangle),
            "Ellipse" => Some(Self::Ellipse),
            "StraightLine" => Some(Self::StraightLine),
            "Arrow" => Some(Self::Arrow),
            "Pencil" => Some(Self::Pencil),
            "MarkerPen" => Some(Self::MarkerPen),
            "Mosaic" => Some(Self::Mosaic),
            "Blur" => Some(Self::Blur),
            "Text" => Some(Self::Text),
            "SerialNumber" => Some(Self::SerialNumber),
            "Eraser" => Some(Self::Eraser),
            _ => None,
        }
    }
}

/// 标注工具的类型
#[delegated_enum]
pub enum AnnotationTool {
    /// 矩形
    Rectangle(RectangleTool),

    /// 椭圆
    Ellipse(EllipseTool),

    /// 直线
    StraightLine(StraightLineTool),

    /// 箭头
    Arrow(ArrowTool),

    /// 铅笔
    Pencil(PencilTool),

    /// 记号笔
    MarkerPen(MarkerPenTool),

    /// 马赛克
    Mosaic(MosaicTool),

    /// 模糊
    Blur(BlurTool),

    /// 文本
    Text(TextTool),

    /// 序号
    SerialNumber(SerialNumberTool),

    // /// 水印
    // Watermark,
    /// 橡皮擦
    Eraser(EraserTool),
}

impl AnnotationTool {
    pub fn tool_name(&self) -> ToolName {
        match self {
            AnnotationTool::Rectangle(_) => ToolName::Rectangle,
            AnnotationTool::Ellipse(_) => ToolName::Ellipse,
            AnnotationTool::StraightLine(_) => ToolName::StraightLine,
            AnnotationTool::Arrow(_) => ToolName::Arrow,
            AnnotationTool::Pencil(_) => ToolName::Pencil,
            AnnotationTool::MarkerPen(_) => ToolName::MarkerPen,
            AnnotationTool::Mosaic(_) => ToolName::Mosaic,
            AnnotationTool::Blur(_) => ToolName::Blur,
            AnnotationTool::Text(_) => ToolName::Text,
            AnnotationTool::SerialNumber(_) => ToolName::SerialNumber,
            // AnnotationTool::Watermark(_) => ToolName::Watermark,
            AnnotationTool::Eraser(_) => ToolName::Eraser,
        }
    }
}

pub trait UnsubmittedAnnotationHandler {
    fn has_uncommitted_annotations(&self) -> bool {
        false
    }

    fn submit_uncommitted_annotations(&mut self, _annotator_state: &mut AnnotatorState) {
        unimplemented!()
    }

    fn drop_uncommitted_annotations(&mut self) -> Annotation {
        unimplemented!()
    }
}

#[delegate_impl]
impl StrokeWidthSupport for AnnotationTool {
    fn supports_get_stroke_width(&self) -> bool;
    fn stroke_width(&self) -> f32;
    fn supports_set_stroke_width(&self) -> bool;
    fn set_stroke_width(&mut self, stroke_width: f32);
}

#[delegate_impl]
impl StrokeColorSupport for AnnotationTool {
    fn supports_get_stroke_color(&self) -> bool;
    fn stroke_color(&self) -> Color32;
    fn supports_set_stroke_color(&self) -> bool;
    fn set_stroke_color(&mut self, color: Color32);
}

#[delegate_impl]
impl StrokeTypeSupport for AnnotationTool {
    fn supports_get_stroke_type(&self) -> bool;
    fn stroke_type(&self) -> StrokeType;
    fn supports_set_stroke_type(&self) -> bool;
    fn set_stroke_type(&mut self, stroke_type: StrokeType);
}

#[delegate_impl]
impl FillColorSupport for AnnotationTool {
    fn supports_get_fill_color(&self) -> bool;

    fn fill_color(&self) -> Option<Color32>;

    fn supports_set_fill_color(&self) -> bool;

    fn set_fill_color(&mut self, color: Color32);
}
#[delegate_impl]
impl FontColorSupport for AnnotationTool {
    /// 是否支持获取字体颜色
    fn supports_get_font_color(&self) -> bool;
    /// 字体颜色
    fn font_color(&self) -> Color32;
    /// 是否支持设置字体颜色
    fn supports_set_font_color(&self) -> bool;
    /// 设置字体颜色
    fn set_font_color(&mut self, color: Color32);
}

impl AnnotationToolCommon for AnnotationTool {
    fn annotator_state(&self) -> SharedAnnotatorState {
        todo!()
    }
}

#[delegate_impl]
impl Widget for &mut AnnotationTool {
    fn ui(self, ui: &mut Ui) -> Response;
}

#[delegate_impl]
impl UnsubmittedAnnotationHandler for AnnotationTool {
    fn has_uncommitted_annotations(&self) -> bool;
    fn submit_uncommitted_annotations(&mut self, _annotator_state: &mut AnnotatorState);
    fn drop_uncommitted_annotations(&mut self) -> Annotation;
}

pub trait ExtraZoomFactorSupport {
    fn extra_zoom_factor(&self) -> f32;
    fn set_extra_zoom_factor(&self, extra_zoom_factor: f32);
}

impl ExtraZoomFactorSupport for Context {
    fn extra_zoom_factor(&self) -> f32 {
        self.memory(|memory| {
            memory
                .data
                .get_temp(AnnotatorState::extra_zoom_factor_id())
                .unwrap_or(1.)
        })
    }

    fn set_extra_zoom_factor(&self, extra_zoom_factor: f32) {
        self.memory_mut(|memory| {
            memory
                .data
                .insert_temp(AnnotatorState::extra_zoom_factor_id(), extra_zoom_factor);
        });
    }
}

pub trait ApplyExtraZoomFactor<T> {
    /// 对T类型的数据应用context.extra_zoom_factor()，返回一个新的数据（不改变原来的数据）
    fn apply_extra_zoom_factor_with_ctx(&self, context: &Context) -> T;
    /// 对T类型的数据应用extra_zoom_factor，返回一个新的数据（不改变原来的数据）
    fn apply_extra_zoom_factor(&self, extra_zoom_factor: f32) -> T;
}

/// ApplyExtraZoomFactor<T>的反向操作
pub trait RemoveExtraZoomFactor<T> {
    fn remove_extra_zoom_factor_with_ctx(&self, context: &Context) -> T;
    fn remove_extra_zoom_factor(&self, extra_zoom_factor: f32) -> T;
}

impl ApplyExtraZoomFactor<Pos2> for Pos2 {
    fn apply_extra_zoom_factor_with_ctx(&self, context: &Context) -> Pos2 {
        self.apply_extra_zoom_factor(context.extra_zoom_factor())
    }

    fn apply_extra_zoom_factor(&self, extra_zoom_factor: f32) -> Pos2 {
        pos2(self.x * extra_zoom_factor, self.y * extra_zoom_factor)
    }
}

impl RemoveExtraZoomFactor<Pos2> for Pos2 {
    fn remove_extra_zoom_factor_with_ctx(&self, context: &Context) -> Pos2 {
        self.remove_extra_zoom_factor(context.extra_zoom_factor())
    }

    fn remove_extra_zoom_factor(&self, extra_zoom_factor: f32) -> Pos2 {
        self.apply_extra_zoom_factor(1. / extra_zoom_factor)
    }
}

impl ApplyExtraZoomFactor<Vec2> for Vec2 {
    fn apply_extra_zoom_factor_with_ctx(&self, context: &Context) -> Vec2 {
        self.apply_extra_zoom_factor(context.extra_zoom_factor())
    }

    fn apply_extra_zoom_factor(&self, extra_zoom_factor: f32) -> Vec2 {
        vec2(self.x * extra_zoom_factor, self.y * extra_zoom_factor)
    }
}

impl RemoveExtraZoomFactor<Vec2> for Vec2 {
    fn remove_extra_zoom_factor_with_ctx(&self, context: &Context) -> Vec2 {
        self.remove_extra_zoom_factor(context.extra_zoom_factor())
    }

    fn remove_extra_zoom_factor(&self, extra_zoom_factor: f32) -> Vec2 {
        self.apply_extra_zoom_factor(1. / extra_zoom_factor)
    }
}

impl ApplyExtraZoomFactor<Rect> for Rect {
    fn apply_extra_zoom_factor_with_ctx(&self, context: &Context) -> Rect {
        let extra_zoom_factor = context.extra_zoom_factor();
        self.apply_extra_zoom_factor(extra_zoom_factor)
    }

    fn apply_extra_zoom_factor(&self, extra_zoom_factor: f32) -> Rect {
        let min = self.min.apply_extra_zoom_factor(extra_zoom_factor);
        let max = self.max.apply_extra_zoom_factor(extra_zoom_factor);
        Rect::from_two_pos(min, max)
    }
}

impl RemoveExtraZoomFactor<Rect> for Rect {
    fn remove_extra_zoom_factor_with_ctx(&self, context: &Context) -> Rect {
        self.remove_extra_zoom_factor(context.extra_zoom_factor())
    }

    fn remove_extra_zoom_factor(&self, extra_zoom_factor: f32) -> Rect {
        self.apply_extra_zoom_factor(1. / extra_zoom_factor)
    }
}

impl ApplyExtraZoomFactor<Vec<Pos2>> for Vec<Pos2> {
    fn apply_extra_zoom_factor_with_ctx(&self, context: &Context) -> Vec<Pos2> {
        let extra_zoom_factor = context.extra_zoom_factor();
        self.apply_extra_zoom_factor(extra_zoom_factor)
    }

    fn apply_extra_zoom_factor(&self, extra_zoom_factor: f32) -> Vec<Pos2> {
        self.iter()
            .map(|point| point.apply_extra_zoom_factor(extra_zoom_factor))
            .collect()
    }
}

impl RemoveExtraZoomFactor<Vec<Pos2>> for Vec<Pos2> {
    fn remove_extra_zoom_factor_with_ctx(&self, context: &Context) -> Vec<Pos2> {
        self.remove_extra_zoom_factor(context.extra_zoom_factor())
    }

    fn remove_extra_zoom_factor(&self, extra_zoom_factor: f32) -> Vec<Pos2> {
        self.apply_extra_zoom_factor(1. / extra_zoom_factor)
    }
}

impl<P: Pixel> ApplyExtraZoomFactor<LogicalSize<P>> for LogicalSize<P> {
    fn apply_extra_zoom_factor_with_ctx(&self, context: &Context) -> LogicalSize<P> {
        let extra_zoom_factor = context.extra_zoom_factor();
        self.apply_extra_zoom_factor(extra_zoom_factor)
    }

    fn apply_extra_zoom_factor(&self, extra_zoom_factor: f32) -> LogicalSize<P> {
        let width = self.width.cast::<f64>();
        let height = self.height.cast::<f64>();
        LogicalSize::new(
            width * extra_zoom_factor as f64,
            height * extra_zoom_factor as f64,
        )
        .cast::<P>()
    }
}

/// 当前标注状态
pub struct AnnotatorState {
    /// 对标注面板进行缩放的缩放因子
    /// 标注面板的逻辑大小=背景图片的逻辑大小=(背景图片的物理大小/scale_factor) * extra_zoom_factor)
    /// 几个概念：
    /// 1. 窗口的缩放因子：scale_factor，这是操作系统中设置的缩放比例，例如缩放125%，那么scale_factor就是1.25
    /// 2. egui的缩放因子：pixels_per_point, 表示使用多少个物理像素来绘制一个逻辑像素，这个和窗口的scale_factor保持一致
    /// 3. 标注面板的额外缩放因子：extra_zoom_factor，表示对面板进行额外的缩放，以支持通过滚轮调整标注面板的尺寸的功能
    /// 4. 每个标注（Annotation）中的数据（位置、宽高）都是基于extra_zoom_factor为1的情况存放的，在绘制的时候才会应用extra_zoom_factor
    pub extra_zoom_factor: f32,

    /// 初始适配屏幕的额外缩放因子，用于恢复缩放按钮
    pub initial_zoom_factor: f32,

    /// 背景图片
    pub background_image: Arc<RgbaImage>,

    /// 背景图片的纹理句柄
    pub background_texture_handle: Option<TextureHandle>,

    /// 用于离线渲染
    pub renderer: Arc<EguiOffScreenRender>,

    /// 标注工具
    pub annotation_tools: HashMap<ToolName, AnnotationTool>,

    /// 界面上显示的标注内容
    pub annotations_stack: Vec<Annotation>,

    /// "重做"栈：因"撤销"操作而从annotations_stack中弹出的内容会被放入这里，以支持重做
    pub redo_stack: Vec<Annotation>,

    /// 当前激活的标注工具
    pub current_annotation_tool: Option<AnnotationTool>,

    /// 用户是否希望显示工具栏。实际显示还会受可用宽度限制。
    pub toolbar_visible: bool,

    /// 荧光笔模式：默认使用直线高亮，关闭后可自由绘制。
    pub marker_pen_straight_mode: bool,

    /// 颜色选择器中可用的颜色
    pub candidate_colors: Vec<Color32>,
}

pub type SharedAnnotatorState = Rc<RefCell<AnnotatorState>>;

impl Global for SharedAnnotatorState {}

/// 从栈顶访问T类型的标注
pub trait StackTopAccessor<T> {
    fn peek_annotation<F, R>(&self, func: F) -> Option<R>
    where
        F: Fn(Option<&T>) -> Option<R>;

    #[allow(dead_code)]
    fn peek_annotation_mut<F, R>(&mut self, func: F) -> Option<R>
    where
        F: Fn(Option<&mut T>) -> Option<R>;

    fn pop_annotation(&mut self) -> Option<T>;
}

#[macro_export]
macro_rules! impl_stack_top_access_for {
    ($($tool:ty=>$annotation:ty),*) => {
        $(
            impl $tool {
                fn peek_annotation<F, R>(&self, func: F) -> Option<R>
                where
                    F: Fn(Option<&$annotation>) -> Option<R>,
                {
                    let annotator_state = self.annotator_state();
                    let annotator_state = annotator_state.borrow();
                    annotator_state.peek_annotation(func)
                }

                #[allow(dead_code)]
    fn peek_annotation_mut<F, R>(&self, func: F) -> Option<R>
                where
                    F: Fn(Option<&mut $annotation>) -> Option<R>,
                {
                    let annotator_state = self.annotator_state();
                    let mut annotator_state = annotator_state.borrow_mut();
                    annotator_state.peek_annotation_mut(func)
                }

                fn pop_annotation(&self) -> Option<$annotation> {
                    let annotator_state = self.annotator_state();
                    annotator_state.borrow_mut().pop_annotation()
                }
            }
        )*
    };
}

pub trait SharedAnnotatorStateUtil {
    fn with_current_annotation_tool<F>(&self, func: F)
    where
        F: FnOnce(&mut AnnotationTool);
}

impl SharedAnnotatorStateUtil for SharedAnnotatorState {
    fn with_current_annotation_tool<F>(&self, func: F)
    where
        F: FnOnce(&mut AnnotationTool),
    {
        let mut annotator_state_mut_ref = self.borrow_mut();
        let mut current_annotation_tool = annotator_state_mut_ref
            .current_annotation_tool
            .take()
            .unwrap();
        drop(annotator_state_mut_ref);

        func(&mut current_annotation_tool);

        let mut annotator_state_mut_ref = self.borrow_mut();
        annotator_state_mut_ref
            .current_annotation_tool
            .replace(current_annotation_tool);
    }
}

impl AnnotatorState {
    pub fn annotator_panel_id() -> ViewId {
        "annotator-panel".into()
    }
    pub fn primary_toolbar_id() -> ViewId {
        "primary-toolbar".into()
    }
    pub fn secondly_toolbar_id() -> ViewId {
        "secondly-toolbar".into()
    }

    pub fn extra_zoom_factor_id() -> Id {
        "extra-zoom-factor".into()
    }

    pub fn activate_annotation_tool(&mut self, tool_name: ToolName) {
        if let Some(active_tool) = &self.current_annotation_tool
            && active_tool.tool_name() == tool_name
        {
            return;
        }
        let tool = self
            .annotation_tools
            .remove(&tool_name)
            .unwrap_or_else(|| panic!("{:?}Tool does not exist", tool_name));
        let mut tool = tool;

        // 荧光笔使用固定的半透明笔触；旧配置和其他工具保存的颜色通常是完全不透明的。
        // 在切换时归一化，避免必须点击一次调色盘才应用荧光笔样式。
        if tool_name == ToolName::MarkerPen && tool.supports_get_stroke_color() {
            let color = tool.stroke_color();
            if color.a() != 112 {
                tool.set_stroke_color(Color32::from_rgba_unmultiplied(
                    color.r(),
                    color.g(),
                    color.b(),
                    112,
                ));
            }
        }
        if let Some(mut previous_tool) = self.current_annotation_tool.replace(tool) {
            if previous_tool.has_uncommitted_annotations() {
                previous_tool.submit_uncommitted_annotations(self);
            }
            self.annotation_tools
                .insert(previous_tool.tool_name(), previous_tool);
        }
        self.save_current_config();
    }

    pub fn deactivate_annotation_tool(&mut self) {
        if let Some(mut previous_tool) = self.current_annotation_tool.take() {
            if previous_tool.has_uncommitted_annotations() {
                previous_tool.submit_uncommitted_annotations(self);
            }
            self.annotation_tools
                .insert(previous_tool.tool_name(), previous_tool);
        }
    }

    pub fn submit_annotation(&mut self, annotation: Annotation) {
        self.annotations_stack.push(annotation);
        self.redo_stack.clear();

        let config = crate::config::AnnotatorConfig::load();
        if config.auto_deactivate_tool_after_draw {
            self.deactivate_annotation_tool();
        }
    }

    pub fn can_undo(&self) -> bool {
        if let Some(current_tool) = self.current_annotation_tool.as_ref()
            && current_tool.has_uncommitted_annotations()
        {
            return true;
        }
        if !self.annotations_stack.is_empty() {
            return true;
        }
        false
    }

    pub fn undo(&mut self) {
        let mut pop_stack_top_annotation = true;
        if let Some(tool) = self.current_annotation_tool.as_mut()
            && tool.has_uncommitted_annotations()
        {
            let annotation = tool.drop_uncommitted_annotations();
            self.redo_stack.push(annotation);
            pop_stack_top_annotation = false;
        }
        if pop_stack_top_annotation {
            let annotation = self.annotations_stack.pop();
            if let Some(annotation) = annotation {
                self.redo_stack.push(annotation);
            }
        }
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn redo(&mut self) {
        let annotation = self.redo_stack.pop();
        if let Some(annotation) = annotation {
            self.annotations_stack.push(annotation);
        }
    }

    pub fn take_screenshot(&mut self, scale_factor: f32) -> Receiver<Arc<RgbaImage>> {
        if let Some(mut tool) = self.current_annotation_tool.take() {
            if tool.has_uncommitted_annotations() {
                tool.submit_uncommitted_annotations(self);
            }
            self.current_annotation_tool = Some(tool);
        }
        self.annotations_stack.iter_mut().for_each(|annotation| {
            annotation.activation_mut().deactivate();
        });

        let provider = BackgroundImageWithAnnotationsProvider::new(self.renderer.clone());
        provider.background_image(self, scale_factor, self.extra_zoom_factor)
    }

    /// 保存当前工具配置到 config.toml（节流：最多每秒一次）
    pub fn save_current_config(&mut self) {
        let saved_config = crate::config::AnnotatorConfig::load();
        let mut config = saved_config.clone();
        config.marker_pen_straight_mode = self.marker_pen_straight_mode;
        if let Some(tool) = &self.current_annotation_tool {
            let tool_settings = config
                .tool_settings
                .entry(format!("{:?}", tool.tool_name()))
                .or_insert(crate::config::ToolSettings {
                    stroke_width: None,
                    stroke_color_rgba: None,
                    fill_color_rgba: None,
                    font_color_rgba: None,
                });
            if tool.supports_get_stroke_width() {
                tool_settings.stroke_width = Some(tool.stroke_width());
            }
            if tool.supports_get_stroke_color() {
                let c = tool.stroke_color();
                let rgba = c.to_srgba_unmultiplied();
                tool_settings.stroke_color_rgba = Some(rgba);
            }
            // 只有纯填充工具（无描边）才保存 fill_color，避免下次启动时矩形/椭圆被错误填充
            if tool.supports_get_fill_color()
                && !tool.supports_get_stroke_color()
                && let Some(c) = tool.fill_color()
            {
                let rgba = c.to_srgba_unmultiplied();
                tool_settings.fill_color_rgba = Some(rgba);
            }
            if tool.supports_get_font_color() {
                let c = tool.font_color();
                let rgba = c.to_srgba_unmultiplied();
                tool_settings.font_color_rgba = Some(rgba);
            }
            config.active_tool = format!("{:?}", tool.tool_name());
        }
        if config != saved_config {
            config.save();
        }
    }
}

trait SmallRect {
    /// 将一个点扩展成一个小矩形
    fn rect(&self, width: f32, height: f32) -> Rect;
}

impl SmallRect for Pos2 {
    fn rect(&self, width: f32, height: f32) -> Rect {
        let pos = self;
        let half_width = width / 2f32;
        let half_height = height / 2f32;
        let top_left_pos = pos2(pos.x - half_width, pos.y - half_height);
        let right_bottom_pos = pos2(pos.x + half_width, pos.y + half_height);
        Rect::from_two_pos(top_left_pos, right_bottom_pos)
    }
}

/// 小矩形的默认宽度和高度
pub const DEFAULT_SIZE_FOR_SMALL_RECT: (f32, f32) = (6., 6.);

pub trait PainterExt {
    /// 将一个的扩展成一个小矩形并绘制它
    fn small_rect(&self, pos: &Pos2);

    /// 为一个矩形绘制各个角以及边上的小矩形
    fn small_rects(&self, rect: &Rect);

    /// 绘制矩形，支持填充和不同风格的边框
    ///
    /// # 参数
    /// - `painter`: egui 绘制器
    /// - `rect`: 矩形区域（填充区域）
    /// - `fill_color`: 填充颜色
    /// - `stroke`: 边框样式（颜色、宽度）
    /// - `stroke_kind`: 边框对齐方式（Inside / Outside / Middle）
    /// - `stroke_type`: 线条类型（实线 / 虚线 / 点线）
    fn rectangle(
        &self,
        rect: &Rect,
        fill_color: impl Into<Color32>,
        stroke: impl Into<Stroke>,
        stroke_kind: StrokeKind,
        stroke_type: StrokeType,
    );
}

impl PainterExt for Painter {
    fn small_rect(&self, pos: &Pos2) {
        let painter = self;
        let width = DEFAULT_SIZE_FOR_SMALL_RECT.0;
        let height = DEFAULT_SIZE_FOR_SMALL_RECT.1;
        painter.rect(
            pos.rect(width, height),
            0,
            Color32::TRANSPARENT,
            Stroke::new(1f32, Color32::WHITE),
            StrokeKind::Middle,
        );
    }

    fn small_rects(&self, rect: &Rect) {
        let painter = self;

        let top_left_pos = rect.left_top();
        let top_right_pos = rect.right_top();
        let bottom_right_pos = rect.right_bottom();
        let bottom_left_pos = rect.left_bottom();

        let center_left_edge = pos2(top_left_pos.x, top_left_pos.y + rect.height() / 2f32);
        let center_right_edge = pos2(top_right_pos.x, top_right_pos.y + rect.height() / 2f32);
        let center_top_edge = pos2(top_left_pos.x + rect.width() / 2f32, top_left_pos.y);
        let center_bottom_edge = pos2(bottom_left_pos.x + rect.width() / 2f32, bottom_left_pos.y);

        painter.small_rect(&top_left_pos);
        painter.small_rect(&top_right_pos);
        painter.small_rect(&bottom_right_pos);
        painter.small_rect(&bottom_left_pos);

        painter.small_rect(&center_left_edge);
        painter.small_rect(&center_right_edge);
        painter.small_rect(&center_top_edge);
        painter.small_rect(&center_bottom_edge);
    }

    fn rectangle(
        &self,
        rect: &Rect,
        fill_color: impl Into<Color32>,
        stroke: impl Into<Stroke>,
        stroke_kind: StrokeKind,
        stroke_type: StrokeType,
    ) {
        let painter = self;
        let fill_color = fill_color.into();
        let stroke = stroke.into();

        // 1. 绘制填充矩形
        painter.rect_filled(*rect, 0.0, fill_color);

        // 2. 根据对齐方式计算边框路径所在的矩形
        let half_width = stroke.width / 2.0;
        let path_rect = match stroke_kind {
            StrokeKind::Inside => rect.shrink(half_width),
            StrokeKind::Outside => rect.expand(half_width),
            StrokeKind::Middle => *rect,
        };

        // 3. 绘制边框
        match stroke_type {
            StrokeType::SolidLine => {
                // 实线直接用 rect_stroke
                painter.rect_stroke(path_rect, 0.0, stroke, stroke_kind);
            }
            StrokeType::DashedLine => {
                // 虚线：使用 dashed_line，自定义 dash 和 gap 长度
                let dash_len = dash_len_for_dashed_line(stroke.width);
                let gap_len = gap_len_for_dashed_line(stroke.width);
                draw_dashed_rect(painter, path_rect, stroke, dash_len, gap_len);
            }
            StrokeType::DottedLine => {
                // 点线：使用 dotted_line，根据线宽计算点间距和半径
                let spacing = spacing_for_dotted_line(stroke.width); // 点间距
                let radius = radius_for_dotted_line(stroke.width); // 点半径
                draw_dotted_rect(painter, path_rect, stroke.color, spacing, radius);
            }
        }
    }
}

pub fn dash_len_for_dashed_line(stroke_width: f32) -> f32 {
    if stroke_width * 3. < 6. {
        6.
    } else {
        stroke_width * 3.
    }
}

pub fn gap_len_for_dashed_line(stroke_width: f32) -> f32 {
    if stroke_width * 3. < 6. {
        6.
    } else {
        stroke_width * 3.
    }
}

pub fn spacing_for_dotted_line(stroke_width: f32) -> f32 {
    let spacing = stroke_width * 2.0; // 点间距
    if spacing < 6. { 6. } else { spacing }
}

pub fn radius_for_dotted_line(stroke_width: f32) -> f32 {
    stroke_width / 2.0
}

/// 绘制矩形的虚线边框
fn draw_dashed_rect(painter: &Painter, rect: Rect, stroke: Stroke, dash_len: f32, gap_len: f32) {
    let [left, right, top, bottom] = [rect.left(), rect.right(), rect.top(), rect.bottom()];

    let edges = [
        (Pos2::new(left, top), Pos2::new(right, top)), // 上边
        (Pos2::new(right, top), Pos2::new(right, bottom)), // 右边
        (Pos2::new(right, bottom), Pos2::new(left, bottom)), // 下边
        (Pos2::new(left, bottom), Pos2::new(left, top)), // 左边
    ];

    for (start, end) in edges {
        let shape = Shape::dashed_line(&[start, end], stroke, dash_len, gap_len);
        painter.add(shape);
    }
}

/// 绘制矩形的点线边框
fn draw_dotted_rect(painter: &Painter, rect: Rect, color: Color32, spacing: f32, radius: f32) {
    let [left, right, top, bottom] = [rect.left(), rect.right(), rect.top(), rect.bottom()];

    let edges = [
        (Pos2::new(left, top), Pos2::new(right, top)), // 上边
        (Pos2::new(right, top), Pos2::new(right, bottom)), // 右边
        (Pos2::new(right, bottom), Pos2::new(left, bottom)), // 下边
        (Pos2::new(left, bottom), Pos2::new(left, top)), // 左边
    ];

    for (start, end) in edges {
        // dotted_line 返回 Vec<Shape>，需要逐个添加
        let shapes = Shape::dotted_line(&[start, end], color, spacing, radius);
        for shape in shapes {
            painter.add(shape);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ToolName;

    #[test]
    fn tool_name_from_config_value_parses_supported_tools() {
        assert_eq!(
            ToolName::from_config_value("Rectangle"),
            Some(ToolName::Rectangle)
        );
        assert_eq!(
            ToolName::from_config_value("  SerialNumber  "),
            Some(ToolName::SerialNumber)
        );
        assert_eq!(ToolName::from_config_value("Watermark"), None);
        assert_eq!(ToolName::from_config_value("Unknown"), None);
    }
}
