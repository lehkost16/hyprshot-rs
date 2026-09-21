use crate::annotator::{
    ActivationSupport, Annotation, AnnotationActivationSupport, AnnotatorState,
    ApplyExtraZoomFactor, FillColorSupport, FontColorSupport, RemoveExtraZoomFactor,
    StrokeColorSupport, StrokeType, StrokeTypeSupport, StrokeWidthSupport,
    UnsubmittedAnnotationHandler,
};
use crate::dpi::{LogicalBounds, PhysicalBounds, PhysicalSize};
use crate::egui_off_screen_render::EguiOffScreenRender;
use crate::{
    declare_not_support_fill_color, declare_not_support_font_color,
    declare_not_support_stroke_color, declare_not_support_stroke_type,
    declare_not_support_stroke_width,
};
use egui::PointerButton;
use egui::load::SizedTexture;
use egui::{
    Color32, ColorImage, CursorIcon, Frame, Image, ImageSource, Pos2, Rect, Response, Sense,
    TextureHandle, Ui, Widget, pos2, vec2,
};
use image::{GenericImageView, ImageError};
use image::{Rgba, RgbaImage};
use imageproc::filter::gaussian_blur_f32;
use log::error;
use std::cell::RefCell;
use std::cmp::{max, min};
use std::default::Default;
use std::rc::{Rc, Weak};
use std::sync::atomic::AtomicUsize;
use std::sync::oneshot::Receiver;
use std::sync::{Arc, oneshot};

pub trait ImageHandler {
    fn handle(
        &self,
        image: &RgbaImage,
        bounds: PhysicalBounds<u32>,
    ) -> Result<RgbaImage, image::ImageError>;
}

#[derive(Clone)]
pub struct MosaicHandler {
    block_size: u32,
}

impl MosaicHandler {
    pub fn new(block_size: u32) -> Self {
        Self { block_size }
    }
}

impl ImageHandler for MosaicHandler {
    fn handle(
        &self,
        image: &RgbaImage,
        bounds: PhysicalBounds<u32>,
    ) -> Result<RgbaImage, ImageError> {
        // 转换为 RgbImage 以便直接修改像素
        let x = max(bounds.origin.x, 0);
        let y = max(bounds.origin.y, 0);
        let width = min(bounds.size.width, image.width());
        let height = min(bounds.size.height, image.height());
        let block_size = self.block_size;

        let sub_image = image.view(x, y, width, height);
        let mut result = RgbaImage::new(width, height);

        // 遍历每个块
        for y in (0..height).step_by(block_size as usize) {
            for x in (0..width).step_by(block_size as usize) {
                // 确定当前块的实际边界（防止超出图像边缘）
                let block_width = block_size.min(width - x);
                let block_height = block_size.min(height - y);

                // 计算块内所有像素的 RGB 总和
                let (mut r_sum, mut g_sum, mut b_sum, mut a_sum) = (0u64, 0u64, 0u64, 0u64);
                let mut count = 0;

                for dy in 0..block_height {
                    for dx in 0..block_width {
                        let pixel = sub_image.get_pixel(x + dx, y + dy);
                        r_sum += pixel[0] as u64;
                        g_sum += pixel[1] as u64;
                        b_sum += pixel[2] as u64;
                        a_sum += pixel[3] as u64;
                        count += 1;
                    }
                }

                // 计算平均颜色
                let avg_r = (r_sum / count) as u8;
                let avg_g = (g_sum / count) as u8;
                let avg_b = (b_sum / count) as u8;
                let avg_a = (a_sum / count) as u8;
                let avg_color = Rgba([avg_r, avg_g, avg_b, avg_a]);

                // 用平均颜色填充整个块
                for dy in 0..block_height {
                    for dx in 0..block_width {
                        result.put_pixel(x + dx, y + dy, avg_color);
                    }
                }
            }
        }

        Ok(result)
    }
}

#[derive(Clone)]
pub struct BlurHandler {
    /// 标准差 sigma，值越大越模糊
    sigma: f32,
}

impl BlurHandler {
    pub fn new(sigma: f32) -> Self {
        Self { sigma }
    }
}

impl Default for BlurHandler {
    fn default() -> Self {
        BlurHandler::new(3.0)
    }
}

impl ImageHandler for BlurHandler {
    fn handle(
        &self,
        image: &RgbaImage,
        bounds: PhysicalBounds<u32>,
    ) -> Result<RgbaImage, ImageError> {
        // 转换为 RgbImage 以便直接修改像素
        let x = max(bounds.origin.x, 0);
        let y = max(bounds.origin.y, 0);
        let width = min(bounds.size.width, image.width());
        let height = min(bounds.size.height, image.height());

        let sub_image = image.view(x, y, width, height).to_image();

        let blurred = gaussian_blur_f32(&sub_image, self.sigma);
        Ok(blurred)
    }
}

#[derive(Clone)]
pub struct ExtractHandler {}
impl ExtractHandler {
    pub fn new() -> Self {
        Self {}
    }
}
impl ImageHandler for ExtractHandler {
    fn handle(
        &self,
        image: &RgbaImage,
        bounds: PhysicalBounds<u32>,
    ) -> Result<RgbaImage, ImageError> {
        // 转换为 RgbImage 以便直接修改像素
        let x = max(bounds.origin.x, 0);
        let y = max(bounds.origin.y, 0);
        let width = min(bounds.size.width, image.width());
        let height = min(bounds.size.height, image.height());

        let sub_image = image.view(x, y, width, height).to_image();
        Ok(sub_image)
    }
}

#[derive(Clone)]
pub struct ImageBasedAnnotation<T: Default + Clone, H: ImageHandler> {
    _style: T,
    rect: Rect,
    source_image: Arc<RgbaImage>,
    texture_handle: Option<TextureHandle>,
    activation: ActivationSupport,
    image_handler: Rc<H>,
}

impl<T: Clone + Default, H: ImageHandler> ImageBasedAnnotation<T, H> {
    pub fn new(rect: Rect, background_image: Arc<RgbaImage>, image_handler: Rc<H>) -> Self {
        Self {
            _style: Default::default(),
            rect,
            source_image: background_image,
            texture_handle: None,
            activation: ActivationSupport::NotSupported,
            image_handler,
        }
    }
}

static TEXTURE_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn try_correct_bounds(
    original_bounds: PhysicalBounds<u32>,
    valid_bounds: PhysicalBounds<u32>,
) -> Result<PhysicalBounds<u32>, String> {
    let mut left_x = original_bounds.origin.x;
    let mut top_y = original_bounds.origin.y;
    let mut right_x = original_bounds.origin.x + original_bounds.size.width;
    let mut bottom_y = original_bounds.origin.y + original_bounds.size.height;
    if original_bounds.size.width == 0 || original_bounds.size.height == 0 {
        return Err(String::from("Invalid bounds"));
    }
    if left_x < valid_bounds.origin.x {
        left_x = original_bounds.origin.x;
    }
    if top_y < valid_bounds.origin.y {
        top_y = original_bounds.origin.y;
    }
    if right_x > valid_bounds.origin.x + valid_bounds.size.width {
        right_x = valid_bounds.origin.x + valid_bounds.size.width;
    }
    if bottom_y > valid_bounds.origin.y + valid_bounds.size.height {
        bottom_y = valid_bounds.origin.y + valid_bounds.size.height;
    }
    let result_bounds = PhysicalBounds::new(left_x, top_y, right_x - left_x, bottom_y - top_y);
    Ok(result_bounds)
}

impl<T: Clone + Default, H: ImageHandler> Widget for &mut ImageBasedAnnotation<T, H> {
    fn ui(self, ui: &mut Ui) -> Response {
        let response = ui.allocate_rect(self.rect, Sense::hover());
        let scale_factor = ui.ctx().pixels_per_point();
        let logical_bounds = LogicalBounds::new(
            self.rect.min.x,
            self.rect.min.y,
            self.rect.width(),
            self.rect.height(),
        );
        let physical_bounds: PhysicalBounds<u32> = logical_bounds.to_physical(scale_factor as f64);

        let block_size = 10;

        if physical_bounds.size.width < block_size || physical_bounds.size.height < block_size {
            return response;
        }

        let Ok(physical_bounds) = try_correct_bounds(
            physical_bounds,
            PhysicalBounds::new(0, 0, self.source_image.width(), self.source_image.height()),
        ) else {
            return response;
        };

        let painter = ui.painter();
        if let Some(texture_handle) = self.texture_handle.clone() {
            let texture_id = texture_handle.id();
            painter.image(
                texture_id,
                self.rect.apply_extra_zoom_factor_with_ctx(ui.ctx()),
                Rect::from_two_pos(pos2(0., 0.), pos2(1., 1.)),
                Color32::WHITE,
            );
        } else {
            let image_handler = self.image_handler.clone();
            if let Ok(mosaic_image) = image_handler.handle(&self.source_image, physical_bounds) {
                let color_image = Arc::new(ColorImage::from_rgba_premultiplied(
                    [
                        mosaic_image.width() as usize,
                        mosaic_image.height() as usize,
                    ],
                    mosaic_image.as_raw(),
                ));
                let texture_handle = painter.ctx().load_texture(
                    format!(
                        "mosaic-{}",
                        TEXTURE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    ),
                    egui::ImageData::Color(color_image),
                    crate::texture::screenshot_texture_options(),
                );
                self.texture_handle = Some(texture_handle.clone());
                let texture_id = texture_handle.id();
                painter.image(
                    texture_id,
                    self.rect.apply_extra_zoom_factor_with_ctx(ui.ctx()),
                    Rect::from_two_pos(pos2(0., 0.), pos2(1., 1.)),
                    Color32::WHITE,
                );
            }
        }
        response
    }
}

impl<T: Clone + Default, H: ImageHandler> AnnotationActivationSupport
    for ImageBasedAnnotation<T, H>
{
    fn activation(&self) -> &ActivationSupport {
        &self.activation
    }

    fn activation_mut(&mut self) -> &mut ActivationSupport {
        &mut self.activation
    }
}

#[derive(Default, Clone)]
pub struct MosaicStyle {}

#[derive(Default, Clone)]
pub struct BlurStyle {}

#[derive(Default, Clone)]
pub struct EraserStyle {}

pub type MosaicAnnotation = ImageBasedAnnotation<MosaicStyle, MosaicHandler>;
pub type BlurAnnotation = ImageBasedAnnotation<BlurStyle, BlurHandler>;
pub type EraserAnnotation = ImageBasedAnnotation<EraserStyle, ExtractHandler>;

declare_not_support_stroke_width!(MosaicAnnotation, BlurAnnotation, EraserAnnotation);
declare_not_support_stroke_color!(MosaicAnnotation, BlurAnnotation, EraserAnnotation);
declare_not_support_stroke_type!(MosaicAnnotation, BlurAnnotation, EraserAnnotation);
declare_not_support_fill_color!(MosaicAnnotation, BlurAnnotation, EraserAnnotation);

#[derive(Default)]
pub struct ImageBasedToolState<S>
where
    S: Default + Clone,
{
    /// 样式
    _style: S,
    /// 拖动的起点
    drag_start_pos: Option<Pos2>,
}

pub struct ImageBasedTool<S: Default + Clone, H: ImageHandler> {
    annotator_state: Weak<RefCell<AnnotatorState>>,
    tool_state: ImageBasedToolState<S>,
    background_image_provider: Box<dyn BackgroundImageProvider>,
    background_image_receiver: Option<Receiver<Arc<RgbaImage>>>,
    background_image: Option<Arc<RgbaImage>>,
    image_handler: Rc<H>,
}

pub trait BackgroundImageProvider {
    fn background_image(
        &self,
        annotator_state: &AnnotatorState,
        pixels_per_point: f32,
        extra_zoom_factor: f32,
    ) -> Receiver<Arc<RgbaImage>>;
}

impl<S: Default + Clone, H: ImageHandler> ImageBasedTool<S, H> {
    pub fn new(
        annotator_state: Weak<RefCell<AnnotatorState>>,
        background_image_provider: Box<dyn BackgroundImageProvider>,
        image_handler: Rc<H>,
    ) -> Self {
        Self {
            annotator_state,
            tool_state: Default::default(),
            background_image_provider,
            background_image_receiver: None,
            background_image: None,
            image_handler,
        }
    }
}

impl<S, H> UnsubmittedAnnotationHandler for ImageBasedTool<S, H>
where
    S: Default + Clone,
    H: ImageHandler,
{
}

pub type MosaicTool = ImageBasedTool<MosaicStyle, MosaicHandler>;
pub type BlurTool = ImageBasedTool<BlurStyle, BlurHandler>;
pub type EraserTool = ImageBasedTool<EraserStyle, ExtractHandler>;

declare_not_support_stroke_width!(MosaicTool, BlurTool, EraserTool);
declare_not_support_stroke_color!(MosaicTool, BlurTool, EraserTool);
declare_not_support_stroke_type!(MosaicTool, BlurTool, EraserTool);
declare_not_support_fill_color!(MosaicTool, BlurTool, EraserTool);
declare_not_support_font_color!(MosaicTool, BlurTool, EraserTool);

impl From<MosaicAnnotation> for Annotation {
    fn from(val: MosaicAnnotation) -> Self {
        Annotation::Mosaic(val)
    }
}

impl From<BlurAnnotation> for Annotation {
    fn from(val: BlurAnnotation) -> Self {
        Annotation::Blur(val)
    }
}

impl From<EraserAnnotation> for Annotation {
    fn from(val: EraserAnnotation) -> Self {
        Annotation::Eraser(val)
    }
}

macro_rules! impl_widget_for {
    ($($tool:ty=>$annotation:ty),*) => {
        $(

impl Widget for &mut $tool {
    fn ui(self, ui: &mut Ui) -> Response {
        let sense_area = Rect::from_min_size(Pos2::ZERO, ui.available_size());
        let response = ui.allocate_rect(sense_area, Sense::click_and_drag());

        let Some(pointer_pos) = ui.ctx().pointer_hover_pos() else {
            return response;
        };

        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);

        if response.drag_started_by(PointerButton::Primary) {
            let drag_started_pos = ui.ctx().input(|i| i.pointer.press_origin()).unwrap();
            self.tool_state.drag_start_pos = Some(drag_started_pos);

            let annotator_state = self.annotator_state.upgrade().unwrap();
            let pixels_per_point = ui.ctx().pixels_per_point();
            self.background_image_receiver = Some(
                // 给ImageBasedTool提供的图片是未经extra_zoom_factor处理的
                self.background_image_provider
                    .background_image(&*annotator_state.borrow(), pixels_per_point, 1.),
            );
            self.background_image = None;
        }

        if response.dragged_by(PointerButton::Primary) {
            // 拖动中
            let drag_started_pos = self.tool_state.drag_start_pos.unwrap();
            let rect = Rect::from_two_pos(drag_started_pos, pointer_pos).remove_extra_zoom_factor_with_ctx(ui.ctx());
            if let Some(background_image) = self.background_image.clone() {
                let mut annotation =
                    <$annotation>::new(rect, background_image, self.image_handler.clone());
                ui.add(&mut annotation);
            } else {
                let background_image_receiver = self.background_image_receiver.take();
                if let Some(background_image_receiver) = background_image_receiver {
                    let background_image = background_image_receiver.try_recv();
                    match background_image {
                        Ok(background_image) => {
                            self.background_image = Some(background_image.clone());
                            let mut annotation = <$annotation>::new(
                                rect,
                                background_image,
                                self.image_handler.clone(),
                            );
                            ui.add(&mut annotation);
                        }
                        Err(oneshot::TryRecvError::Empty(rx)) => {
                            self.background_image_receiver = Some(rx);
                        }
                        Err(oneshot::TryRecvError::Disconnected) => {
                            error!("Failed to receive background image");
                        }
                    }
                }
            }
        }

        if response.drag_stopped_by(PointerButton::Primary) {
            // 拖动结束
            let drag_started_pos = self.tool_state.drag_start_pos.unwrap();
            let rect = Rect::from_two_pos(drag_started_pos, pointer_pos).remove_extra_zoom_factor_with_ctx(ui.ctx());

            if let Some(background_image) = self.background_image.clone() {
                let annotation =
                    <$annotation>::new(rect, background_image, self.image_handler.clone());
                self.annotator_state
                    .upgrade()
                    .unwrap()
                    .borrow_mut()
                    .submit_annotation(annotation.into());
            } else {
                let background_image_receiver = self.background_image_receiver.take();
                // TODO: 考虑是否处理可能存在的background_image_receiver.try_recv()失败的场景
                if let Some(background_image_receiver) = background_image_receiver {
                    let background_image = background_image_receiver.try_recv();
                    match background_image {
                        Ok(background_image) => {
                            self.background_image = Some(background_image.clone());
                            let annotation = <$annotation>::new(
                                rect,
                                background_image,
                                self.image_handler.clone(),
                            );
                            self.annotator_state
                                .upgrade()
                                .unwrap()
                                .borrow_mut()
                                .submit_annotation(annotation.into());
                        }
                        Err(oneshot::TryRecvError::Empty(rx)) => {
                            self.background_image_receiver = Some(rx);
                        }
                        Err(oneshot::TryRecvError::Disconnected) => {
                            error!("Failed to receive background image");
                        }
                    }
                }
            }
        }
        response
    }
}

        )*
    }
}

impl_widget_for!(MosaicTool=>MosaicAnnotation, BlurTool=>BlurAnnotation, EraserTool=>EraserAnnotation);

pub struct OriginalBackgroundImageProvider;

impl OriginalBackgroundImageProvider {
    pub fn new() -> Self {
        Self {}
    }
}

/// 提供（未经extra_zoom_factor缩放的）原始背景图片（不包含标注内容）
impl BackgroundImageProvider for OriginalBackgroundImageProvider {
    fn background_image(
        &self,
        annotator_state: &AnnotatorState,
        _: f32,
        _: f32,
    ) -> Receiver<Arc<RgbaImage>> {
        let image = annotator_state.background_image.clone();
        let (sender, receiver) = oneshot::channel::<Arc<RgbaImage>>();
        sender.send(image).unwrap();
        receiver
    }
}

pub struct BackgroundImageWithAnnotationsProvider {
    renderer: Arc<EguiOffScreenRender>,
}

impl BackgroundImageWithAnnotationsProvider {
    pub fn new(renderer: Arc<EguiOffScreenRender>) -> Self {
        Self { renderer }
    }
}
impl BackgroundImageProvider for BackgroundImageWithAnnotationsProvider {
    fn background_image(
        &self,
        annotator_state: &AnnotatorState,
        pixels_per_point: f32,
        extra_zoom_factor: f32,
    ) -> Receiver<Arc<RgbaImage>> {
        let original_background_image = annotator_state.background_image.clone();
        let annotations = annotator_state.annotations_stack.clone();

        let physical_size = PhysicalSize::new(
            original_background_image.width(),
            original_background_image.height(),
        );
        let logical_size = physical_size
            .to_logical(pixels_per_point as f64)
            .apply_extra_zoom_factor(extra_zoom_factor);

        self.renderer.render_egui_to_image(
            logical_size,
            pixels_per_point,
            extra_zoom_factor,
            Box::new(move |input, context| {
                let mut annotaions = annotations;
                context.run_ui(input, move |ctx| {
                    egui::CentralPanel::default()
                        .frame(Frame::new())
                        .show(ctx, |ui| {
                            // 创建 ColorImage
                            // 注意：RgbaImage 的 bytes 应该是连续的 RGBA 数据
                            let background_image = Arc::new(ColorImage::from_rgba_premultiplied(
                                [
                                    original_background_image.width() as usize,
                                    original_background_image.height() as usize,
                                ],
                                original_background_image.as_raw(),
                            ));

                            // Load the texture only once.
                            let texture_handle = ui.ctx().load_texture(
                                "background-image",
                                egui::ImageData::Color(background_image),
                                crate::texture::screenshot_texture_options(),
                            );

                            let bg_image = Image::new(ImageSource::Texture(
                                SizedTexture::from_handle(&texture_handle),
                            ));

                            let frame_size = PhysicalSize::new(
                                original_background_image.width(),
                                original_background_image.height(),
                            )
                            .to_logical(ui.ctx().pixels_per_point() as f64)
                            .apply_extra_zoom_factor_with_ctx(ui.ctx());

                            bg_image.paint_at(
                                ui,
                                Rect::from_min_size(
                                    pos2(0., 0.),
                                    vec2(frame_size.width, frame_size.height),
                                ),
                            );

                            annotaions.iter_mut().for_each(|annotation| {
                                ui.add(annotation);
                            });
                        });
                })
            }),
        )
    }
}
