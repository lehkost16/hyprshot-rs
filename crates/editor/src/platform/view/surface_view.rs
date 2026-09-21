use crate::platform::application::{Application, GlobalState};
use crate::platform::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
use crate::platform::egui_input::EguiInput;
use crate::platform::gpu::GpuContext;
use crate::platform::view::{BuildViewFn, View, ViewId};
use crate::platform::window::AppWindow;
use crate::ui::font::setup_chinese_fonts;
use egui::{FullOutput, ImeEvent, PlatformOutput, RawInput};
use egui_wgpu::wgpu::TextureFormat;
use egui_wgpu::{RendererOptions, wgpu};
use log::info;
use sctk::seat::keyboard::{KeyEvent, Modifiers};
use sctk::seat::pointer::PointerEvent;
use smithay_clipboard::Clipboard;
use std::sync::Arc;
use wayland_backend::client::ObjectId;
use wayland_client::Proxy;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;

pub struct SurfaceView<'window> {
    id: ViewId,
    /// Wayland Surface 句柄
    surface: WlSurface,
    /// WGPU 渲染表面
    pub wgpu_surface: wgpu::Surface<'window>,
    pub wgpu_surface_configuration: wgpu::SurfaceConfiguration,
    /// 位置
    position: Option<LogicalPosition<i32>>,
    /// 逻辑尺寸
    size: LogicalSize<u32>,
    /// 当前缩放倍数
    scale_factor: f64,
    /// 视口（用于分数缩放适配）
    viewport: WpViewport,
    /// Egui 上下文
    egui_ctx: Option<egui::Context>,
    /// Egui 输入状态管理
    egui_input: EguiInput,
    /// Egui Skia 渲染器
    egui_renderer: Option<egui_wgpu::Renderer>,
    /// 用于构建UI的函数
    build_view: Option<BuildViewFn>,
    visible: bool,
    should_remove: bool,
}

impl<'window> SurfaceView<'window> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ViewId,
        surface: WlSurface,
        wgpu_surface: wgpu::Surface<'window>,
        wgpu_surface_configuration: wgpu::SurfaceConfiguration,
        size: LogicalSize<u32>,
        scale_factor: f64,
        position: Option<LogicalPosition<i32>>,
        viewport: WpViewport,
        clipboard: Arc<Clipboard>,
        build_view: BuildViewFn,
    ) -> Self {
        viewport.set_destination(size.width as i32, size.height as i32);

        // 初始化 Egui 环境
        let egui_ctx = egui::Context::default();
        egui_extras::install_image_loaders(&egui_ctx);
        setup_chinese_fonts(&egui_ctx);

        let egui_input = EguiInput::new().with_clipboard(clipboard.clone());

        Self {
            id,
            surface,
            wgpu_surface,
            wgpu_surface_configuration,
            position,
            size,
            scale_factor,
            viewport,
            egui_ctx: Some(egui_ctx),
            egui_input,
            egui_renderer: None,
            build_view: Some(build_view),
            visible: true,
            should_remove: false,
        }
    }

    /// 获取关联的 Wayland Surface。
    pub fn surface(&self) -> &WlSurface {
        &self.surface
    }
}

impl<'window> View for SurfaceView<'window> {
    fn id(&self) -> ViewId {
        self.id.clone()
    }

    fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    fn set_scale_factor(&mut self, scale_factor: f64, gpu: &GpuContext) {
        self.scale_factor = scale_factor;

        let physical_size = self.size.to_physical(scale_factor);
        self.resize_surface(physical_size, gpu);

        // 同步 viewport destination，确保 Wayland 视口与实际 buffer 尺寸一致
        self.viewport
            .set_destination(self.size.width as i32, self.size.height as i32);
    }

    fn position(&self) -> Option<LogicalPosition<i32>> {
        self.position
    }

    fn size(&self) -> LogicalSize<u32> {
        self.size
    }

    fn viewport_size(&self) -> PhysicalSize<u32> {
        self.size.to_physical(self.scale_factor)
    }

    fn viewport(&self) -> &WpViewport {
        &self.viewport
    }

    fn resize(&mut self, new_size: LogicalSize<u32>, gpu: &GpuContext) {
        let current_size = self.size;
        info!(
            "Resize viewport {:?} from {:?} to: {:?}",
            self.id, current_size, new_size
        );

        let physical_size = new_size.to_physical(self.scale_factor());
        self.resize_surface(physical_size, gpu);

        self.size = new_size;
        self.viewport()
            .set_destination(new_size.width as i32, new_size.height as i32);
    }

    fn surface(&self) -> &WlSurface {
        self.surface()
    }

    fn surface_id(&self) -> ObjectId {
        self.surface().id()
    }

    fn handle_keyboard_event(&mut self, event: KeyEvent, pressed: bool, repeat: bool) {
        self.egui_input
            .handle_keyboard_event(event, pressed, repeat);
    }

    fn update_modifiers(&mut self, modifiers: Modifiers) {
        self.egui_input.update_modifiers(modifiers);
    }

    fn handle_ime_event(&mut self, event: &ImeEvent) {
        self.egui_input.handle_ime_event(event);
    }

    fn handle_pointer_event(&mut self, event: &PointerEvent, _globals: &GlobalState) {
        self.egui_input.handle_pointer_event(event);
    }
    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn should_remove(&self) -> bool {
        self.should_remove
    }

    fn close_later(&mut self) {
        self.should_remove = true;
    }

    /// 使用 GPU 渲染视图内容
    fn draw(&mut self, app: &mut Application, window: &mut AppWindow) -> Option<PlatformOutput> {
        if !self.visible {
            self.surface.attach(None, 0, 0);
            self.surface.commit();
            return None;
        }
        let egui_output = self.run_egui(app, window);

        // 获取当前帧纹理
        let frame = match self.wgpu_surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(tex)
            | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => tex,
            _ => return None,
        };

        // 创建纹理视图
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let global_state = &app.global_state;
        let gpu_context = global_state.gpu.borrow();
        let gpu = gpu_context.as_ref().unwrap();

        // 创建命令编码器
        let device = &gpu.device;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        let egui_renderer = if let Some(ref mut renderer) = self.egui_renderer {
            renderer
        } else {
            let renderer_option = RendererOptions::default();
            let renderer =
                egui_wgpu::Renderer::new(device, TextureFormat::Bgra8UnormSrgb, renderer_option);
            self.egui_renderer = Some(renderer);
            self.egui_renderer.as_mut().unwrap()
        };

        let physical_size = self.size.to_physical(self.scale_factor);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [physical_size.width, physical_size.height],
            pixels_per_point: egui_output.pixels_per_point,
        };

        // 更新纹理
        // 将给定形状镶嵌成三角形网格
        let paint_jobs = self
            .egui_ctx
            .as_ref()
            .unwrap()
            .tessellate(egui_output.shapes, egui_output.pixels_per_point);
        for (id, image_delta) in &egui_output.textures_delta.set {
            egui_renderer.update_texture(device, &gpu.queue, *id, image_delta);
        }

        // 更新EGUI顶点/索引缓冲区
        egui_renderer.update_buffers(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        {
            // 6. 执行渲染
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let mut render_pass = render_pass.forget_lifetime();

            egui_renderer.render(
                &mut render_pass,
                &paint_jobs, // 渲染同样的 shapes
                &screen_descriptor,
            );
        }

        // 提交命令到队列
        gpu.queue.submit(std::iter::once(encoder.finish()));

        // 提交缓冲区到表面
        frame.present();

        // 请求frame回调以确保持续渲染
        self.surface.frame(
            &global_state.queue_handle,
            sctk::compositor::FrameCallbackData(self.surface.clone()),
        );

        // 释放不再需要的纹理
        for id in &egui_output.textures_delta.free {
            egui_renderer.free_texture(id);
        }

        Some(egui_output.platform_output)
    }
}

impl<'window> SurfaceView<'window> {
    fn resize_surface(&mut self, physical_size: PhysicalSize<u32>, gpu: &GpuContext) {
        let surface_config = &mut self.wgpu_surface_configuration;
        surface_config.width = physical_size.width;
        surface_config.height = physical_size.height;

        self.wgpu_surface.configure(&gpu.device, surface_config);
    }

    pub(super) fn record_position(&mut self, position: LogicalPosition<i32>) {
        self.position = Some(position);
    }

    fn run_egui(&mut self, app: &mut Application, window: &mut AppWindow) -> FullOutput {
        // 准备 Egui 输入
        let mut raw_input = self.egui_input.raw.take();

        // 设置逻辑像素与对应的物理像素的比例
        self.egui_ctx
            .as_mut()
            .unwrap()
            .set_pixels_per_point(self.scale_factor as f32);

        // screen_rect 是逻辑尺寸
        let screen_rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(self.size.width as f32, self.size.height as f32),
        );
        raw_input.screen_rect = Some(screen_rect);

        window.window_context.current_view_id = Some(self.id.clone());
        let build_ui_fn = self.build_view.take().unwrap();
        let mut egui_ctx = self.egui_ctx.take().unwrap();
        let egui_output = build_ui_fn(raw_input, &mut egui_ctx, app, window, self);
        self.build_view.replace(build_ui_fn);
        self.egui_ctx.replace(egui_ctx);

        // 重要：为下一帧准备新的 RawInput
        // egui 在渲染后需要重置输入状态，保留持久性信息
        self.egui_input.raw = RawInput::default();

        egui_output
    }
}
