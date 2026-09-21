use crate::dpi::{LogicalPosition, LogicalSize};
use crate::view::surface_view::SurfaceView;
use crate::view::{BuildViewFn, PopupView, View, ViewId};
use egui_wgpu::wgpu::Surface;
use sctk::shell::xdg::popup::Popup;
use smithay_clipboard::Clipboard;
use std::sync::Arc;
use wayland_backend::client::ObjectId;
use wayland_client::Proxy;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::xdg::shell::client::xdg_positioner::XdgPositioner;

pub struct XdgPopupView<'window> {
    view: SurfaceView<'window>,
    popup: Popup,
    #[allow(dead_code)]
    positioner: XdgPositioner,
    /// 是否已完成首次配置（用于避免在窗口未准备好时绘图）
    pub first_configure_done: bool,
}

impl<'window> XdgPopupView<'window> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ViewId,
        surface: WlSurface,
        popup: Popup,
        positioner: XdgPositioner,
        wgpu_surface: Surface<'window>,
        wgpu_surface_configuration: egui_wgpu::wgpu::SurfaceConfiguration,
        size: LogicalSize<u32>,
        scale_factor: f64,
        viewport: WpViewport,
        clipboard: Arc<Clipboard>,
        build_view: BuildViewFn,
    ) -> Self {
        let view = SurfaceView::new(
            id,
            surface,
            wgpu_surface,
            wgpu_surface_configuration,
            size,
            scale_factor,
            None,
            viewport,
            clipboard,
            build_view,
        );
        Self {
            view,
            positioner,
            popup,
            first_configure_done: false,
        }
    }
}

impl PopupView for XdgPopupView<'_> {
    fn id(&self) -> ViewId {
        self.view.id()
    }

    fn surface_id(&self) -> ObjectId {
        self.view.surface().id()
    }

    fn view(&self) -> &dyn View {
        &self.view
    }

    fn view_mut(&mut self) -> &mut dyn View {
        &mut self.view
    }

    fn first_configure_done(&self) -> bool {
        self.first_configure_done
    }

    fn set_first_configure_done(&mut self) {
        self.first_configure_done = true;
    }

    fn record_position(&mut self, pos: LogicalPosition<i32>) {
        self.view.record_position(pos);
    }

    fn popup(&self) -> &Popup {
        &self.popup
    }
}

/// xdg-popup抓取弹窗必须由真实的用户交互触发，例如鼠标点击、按键按下或触摸按下
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TriggerType {
    MousePress,
    KeyPress,
    Touch,
}
