use crate::dpi::{LogicalPosition, LogicalSize};
use crate::view::surface_view::SurfaceView;
use crate::view::{BuildViewFn, SubView, View, ViewId};
use egui_wgpu::wgpu::Surface;
use smithay_clipboard::Clipboard;
use std::sync::Arc;
use wayland_backend::client::ObjectId;
use wayland_client::Proxy;
use wayland_client::protocol::wl_subsurface::WlSubsurface;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;

pub struct SubSurfaceView<'window> {
    view: SurfaceView<'window>,
    subsurface: WlSubsurface,
    position_calculator: Option<Arc<crate::view::RelativePositionCalculator>>,
}

impl<'window> SubSurfaceView<'window> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ViewId,
        surface: WlSurface,
        wgpu_surface: Surface<'window>,
        wgpu_surface_configuration: egui_wgpu::wgpu::SurfaceConfiguration,
        subsurface: WlSubsurface,
        size: LogicalSize<u32>,
        scale_factor: f64,
        position: Option<LogicalPosition<i32>>,
        viewport: WpViewport,
        clipboard: Arc<Clipboard>,
        build_view: BuildViewFn,
        position_calculator: Option<Arc<crate::view::RelativePositionCalculator>>,
    ) -> Self {
        if let Some(position) = position {
            subsurface.set_position(position.x, position.y);
        }
        let view = SurfaceView::new(
            id,
            surface,
            wgpu_surface,
            wgpu_surface_configuration,
            size,
            scale_factor,
            position,
            viewport,
            clipboard,
            build_view,
        );
        view.surface().commit(); // Initial commit

        Self {
            view,
            subsurface,
            position_calculator,
        }
    }

    pub fn surface_id(&self) -> ObjectId {
        self.view.surface().id()
    }

    pub fn view_id(&self) -> ViewId {
        self.view.id()
    }
}

impl<'window> SubView for SubSurfaceView<'window> {
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

    fn set_position(&mut self, pos: LogicalPosition<i32>) {
        self.subsurface.set_position(pos.x, pos.y);
        self.view.record_position(pos);
    }

    fn position_calculator(&mut self) -> Option<Arc<crate::view::RelativePositionCalculator>> {
        if let Some(relocation_fn) = &self.position_calculator {
            Some(relocation_fn.clone())
        } else {
            None
        }
    }
}
