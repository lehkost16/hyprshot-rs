use crate::annotator::ExtraZoomFactorSupport;
use crate::platform::dpi::{LogicalSize, PhysicalSize};
use crate::platform::gpu::GpuContext;
use crate::ui::font::setup_chinese_fonts;
use egui::{FullOutput, RawInput, Rect, pos2, vec2};
use egui_wgpu::wgpu::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Device, Extent3d, MapMode, Origin3d,
    PollType, Queue, RenderPassColorAttachment, RenderPassDescriptor, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages,
};
use egui_wgpu::{Renderer, RendererOptions};
use image::{ImageBuffer, Rgba, RgbaImage};
use log::error;
use std::sync::oneshot::Receiver;
use std::sync::{Arc, oneshot};
use std::thread::spawn;

pub type BuildUI = Box<dyn FnOnce(RawInput, &mut egui::Context) -> FullOutput>;

pub struct EguiOffScreenRender {
    device: Arc<Device>,
    queue: Arc<Queue>,
    texture_format: TextureFormat,
}

impl EguiOffScreenRender {
    pub fn new(gpu_context: &GpuContext) -> Self {
        let texture_format = TextureFormat::Bgra8UnormSrgb;
        let device = gpu_context.device.clone();
        let queue = gpu_context.queue.clone();
        Self {
            device,
            queue,
            texture_format,
        }
    }

    fn create_egui_wgpu_renderer(&self) -> Renderer {
        Renderer::new(
            &self.device,
            self.texture_format,
            RendererOptions::default(),
        )
    }

    pub fn render_egui_to_image(
        &self,
        physical_size: PhysicalSize<u32>,
        pixels_per_point: f32,
        extra_zoom_factor: f32,
        build_ui: BuildUI,
    ) -> Receiver<Arc<RgbaImage>> {
        let virtual_screen_size: LogicalSize<f32> =
            physical_size.to_logical(pixels_per_point as f64);
        let texture_size = Extent3d {
            width: physical_size.width,
            height: physical_size.height,
            depth_or_array_layers: 1,
        };
        let texture = self.create_texture(texture_size);
        let texture_view = texture.create_view(&Default::default());

        let mut egui_ctx = egui::Context::default();
        egui_extras::install_image_loaders(&egui_ctx);
        setup_chinese_fonts(&egui_ctx);
        egui_ctx.set_pixels_per_point(pixels_per_point);
        egui_ctx.set_extra_zoom_factor(extra_zoom_factor);

        let mut raw_input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                pos2(0., 0.),
                vec2(virtual_screen_size.width, virtual_screen_size.height),
            )),
            ..Default::default()
        };
        crate::texture::set_texture_limit(
            &egui_ctx,
            &mut raw_input,
            self.device.limits().max_texture_dimension_2d,
        );
        let full_output = build_ui(raw_input, &mut egui_ctx);

        // 更新纹理
        // 将给定形状镶嵌成三角形网格
        let paint_jobs = egui_ctx.tessellate(full_output.shapes, pixels_per_point); // 通常由 run 内部处理，但也可手动

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [physical_size.width, physical_size.height],
            pixels_per_point,
        };

        let device = self.device.clone();
        let queue = self.queue.clone();

        let mut renderer = self.create_egui_wgpu_renderer();
        for (id, image_delta) in &full_output.textures_delta.set {
            renderer.update_texture(&device, &queue, *id, image_delta);
        }

        // 创建命令编码器
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());

        // 更新EGUI顶点/索引缓冲区
        renderer.update_buffers(
            &device,
            &queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // 开始渲染通道，使用离屏纹理视图
        {
            let render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("egui offscreen pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: egui_wgpu::wgpu::Operations {
                        load: egui_wgpu::wgpu::LoadOp::Clear(egui_wgpu::wgpu::Color::TRANSPARENT),
                        store: egui_wgpu::wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let mut render_pass = render_pass.forget_lifetime();

            // 执行渲染
            renderer.render(
                &mut render_pass,
                &paint_jobs, // 渲染同样的 shapes
                &screen_descriptor,
            );
        }

        // 提交命令
        queue.submit(Some(encoder.finish()));

        let (sender, receiver) = oneshot::channel();
        spawn(move || {
            let image = Self::get_render_result(device, queue, texture);
            match sender.send(Arc::new(image)) {
                Ok(_) => {}
                Err(err) => {
                    error!("Failed to send image by sender: {}", err);
                }
            }
        });
        receiver
    }

    fn create_texture(&self, texture_size: Extent3d) -> Texture {
        let texture_desc = TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: self.texture_format,
            usage: TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT,
            label: None,
            view_formats: &[],
        };
        self.device.create_texture(&texture_desc)
    }

    fn get_render_result(device: Arc<Device>, queue: Arc<Queue>, texture: Texture) -> RgbaImage {
        let texture_size = texture.size();
        // wgpu 需要使用 COPY_BYTES_PER_ROW_ALIGNMENT 对齐纹理 -> 缓冲区的复制
        // 因此，我们需要同时保存 padded_bytes_per_row 和 unpadded_bytes_per_row
        let pixel_size = size_of::<[u8; 4]>() as u32;
        // 计算对齐后的 bytes_per_row
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded_bytes_per_row = texture_size.width * pixel_size;
        let padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padding;
        let buffer_size = u64::from(padded_bytes_per_row) * u64::from(texture_size.height);

        // 创建目标缓冲区，用于接收像素数据
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("output buffer"),
            size: buffer_size as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(texture_size.height),
                },
            },
            texture_size,
        );
        queue.submit(Some(encoder.finish()));

        let buffer_slice = buffer.slice(..);
        let (tx, rx) = oneshot::channel();
        buffer_slice.map_async(MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        device.poll(PollType::wait_indefinitely()).unwrap();

        if let Ok(Ok(())) = rx.recv() {
            let padded_data = buffer_slice.get_mapped_range();
            let pixels_bgra = padded_data
                .chunks(padded_bytes_per_row as _)
                .flat_map(|chunk| &chunk[..unpadded_bytes_per_row as _])
                .copied()
                .collect::<Vec<_>>();

            // 转换为 RGBA
            let mut pixels_rgba = Vec::with_capacity(pixels_bgra.len());
            for chunk in pixels_bgra.as_chunks::<4>().0 {
                pixels_rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
            }

            ImageBuffer::<Rgba<u8>, _>::from_raw(
                texture_size.width,
                texture_size.height,
                pixels_rgba,
            )
            .unwrap()
        } else {
            panic!("从 GPU 读取数据失败！");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a working Vulkan/GLES adapter; run explicitly"]
    fn offscreen_export_preserves_original_pixels_at_fractional_scale() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let renderer = EguiOffScreenRender {
            device: Arc::new(device),
            queue: Arc::new(queue),
            texture_format: TextureFormat::Bgra8UnormSrgb,
        };
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let source = RgbaImage::from_fn(13, 7, |x, y| {
                Rgba([(x * 19) as u8, (y * 37) as u8, 90, 255])
            });
            let input_image = source.clone();
            let result = renderer.render_egui_to_image(
                PhysicalSize::new(13, 7),
                scale,
                1.0,
                Box::new(move |input, context| {
                    context.run_ui(input, |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::new())
                            .show(ctx, |ui| {
                                let texture = ui.ctx().load_texture(
                                    "test",
                                    egui::ColorImage::from_rgba_unmultiplied(
                                        [13, 7],
                                        input_image.as_raw(),
                                    ),
                                    crate::texture::screenshot_texture_options(),
                                );
                                egui::Image::new(&texture).paint_at(
                                    ui,
                                    Rect::from_min_size(
                                        pos2(0., 0.),
                                        vec2(13. / scale, 7. / scale),
                                    ),
                                );
                            });
                    })
                }),
            );
            let output = result.recv().unwrap();
            assert_eq!(output.dimensions(), source.dimensions());
            for (actual, expected) in output.as_raw().iter().zip(source.as_raw()) {
                assert!(
                    actual.abs_diff(*expected) <= 1,
                    "pixel mismatch at scale {scale}: {actual} != {expected}"
                );
            }
        }
        use crate::annotator::rectangle_based::{RectangleAnnotation, RectangleStyle};
        use crate::annotator::{ActivationSupport, Annotation, AnnotatorState};
        let original = Arc::new(RgbaImage::from_pixel(2246, 612, Rgba([10, 20, 30, 255])));
        let mut state = AnnotatorState {
            session: crate::config::EditorSession::new(Default::default()),
            extra_zoom_factor: 0.25,
            initial_zoom_factor: 0.25,
            background_image: original.clone(),
            background_texture_handle: None,
            renderer: Arc::new(renderer),
            annotation_tools: Default::default(),
            annotations_stack: vec![],
            redo_stack: vec![],
            current_annotation_tool: None,
            toolbar_visible: true,
            marker_pen_straight_mode: true,
            candidate_colors: vec![],
        };
        let untouched = state.take_screenshot(1.25).recv().unwrap();
        assert!(Arc::ptr_eq(&untouched, &original));
        state
            .annotations_stack
            .push(Annotation::Rectangle(RectangleAnnotation::new(
                Rect::from_min_size(pos2(10., 10.), vec2(30., 15.)),
                RectangleStyle::default(),
                ActivationSupport::NotSupported,
            )));
        let zoomed_out = state.take_screenshot(1.25).recv().unwrap();
        state.extra_zoom_factor = 2.0;
        let zoomed_in = state.take_screenshot(1.25).recv().unwrap();
        assert_eq!(zoomed_out.dimensions(), original.dimensions());
        assert_eq!(zoomed_out, zoomed_in);
        assert_ne!(zoomed_out, original);
    }
}
