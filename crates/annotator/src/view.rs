use crate::application::{Application, GlobalState};
use crate::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};
use crate::gpu::GpuContext;
use crate::view::AppView::{Child, Pop, Root};
use crate::window::AppWindow;
use egui::{FullOutput, ImeEvent, PlatformOutput, RawInput};
use sctk::shell::xdg::popup::Popup;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use wayland_backend::client::ObjectId;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;

pub mod sub_surface_view;
pub mod surface_view;
pub mod xdg_popup_view;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ViewId(pub Arc<str>);

impl From<String> for ViewId {
    fn from(s: String) -> Self {
        Self(s.into())
    }
}

impl From<&str> for ViewId {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

pub trait View {
    fn id(&self) -> ViewId;
    fn scale_factor(&self) -> f64;
    fn set_scale_factor(&mut self, scale_factor: f64, gpu: &GpuContext);
    /// Surface在窗口中的位置
    /// 并非所有类型的View都可以获知其自身的位置，只有SubView和PopupView可能会有自身位置的信息
    fn position(&self) -> Option<LogicalPosition<i32>>;
    fn size(&self) -> LogicalSize<u32>;
    fn viewport_size(&self) -> PhysicalSize<u32>;
    fn viewport(&self) -> &WpViewport;
    fn resize(&mut self, new_size: LogicalSize<u32>, gpu: &GpuContext);
    fn surface(&self) -> &WlSurface;
    fn surface_id(&self) -> ObjectId;

    fn handle_keyboard_event(
        &mut self,
        event: sctk::seat::keyboard::KeyEvent,
        pressed: bool,
        repeat: bool,
    );
    fn update_modifiers(&mut self, modifiers: sctk::seat::keyboard::Modifiers);
    fn handle_ime_event(&mut self, event: &ImeEvent);
    fn handle_pointer_event(
        &mut self,
        event: &sctk::seat::pointer::PointerEvent,
        global_state: &GlobalState,
    );
    fn visible(&self) -> bool;

    fn set_visible(&mut self, visible: bool);

    fn should_remove(&self) -> bool;

    fn close_later(&mut self);

    /// 使用 GPU 上下文进行重绘。
    fn draw(&mut self, app: &mut Application, window: &mut AppWindow) -> Option<PlatformOutput>;
}

/// 一个函数，可以根据父表面的尺寸和子表面自身的尺寸重新计算子表面的位置
pub(crate) type RelativePositionCalculator =
    dyn Fn(&PhysicalSize<u32>, &PhysicalSize<u32>) -> PhysicalPosition<u32>;

pub trait SubView {
    fn id(&self) -> ViewId;
    fn surface_id(&self) -> ObjectId;

    fn view(&self) -> &dyn View;

    fn view_mut(&mut self) -> &mut dyn View;

    fn set_position(&mut self, pos: LogicalPosition<i32>);

    fn position_calculator(&mut self) -> Option<Arc<RelativePositionCalculator>>;
}

pub trait PopupView {
    fn id(&self) -> ViewId;
    fn surface_id(&self) -> ObjectId;

    fn view(&self) -> &dyn View;

    fn view_mut(&mut self) -> &mut dyn View;

    fn first_configure_done(&self) -> bool;

    fn set_first_configure_done(&mut self);
    /// 这个方法仅记录位置，不会更改Surface在窗口中的实际位置
    fn record_position(&mut self, pos: LogicalPosition<i32>);

    fn popup(&self) -> &Popup;
}

pub type BuildViewFn = Box<
    dyn Fn(
        RawInput,
        &mut egui::Context,
        &mut Application,
        &mut AppWindow,
        &mut dyn View,
    ) -> FullOutput,
>;

pub enum AppView {
    Root(Box<dyn View>),
    Child(Box<dyn SubView>),
    Pop(Box<dyn PopupView>),
}

impl AppView {
    pub fn get_view_ref(self: &AppView) -> &dyn View {
        match self {
            Root(view) => view.deref(),
            Child(sub_view) => sub_view.view(),
            Pop(popup_view) => popup_view.view(),
        }
    }

    pub fn get_view_ref_mut(self: &mut AppView) -> &mut dyn View {
        match self {
            Root(view) => view.deref_mut(),
            Child(sub_view) => sub_view.view_mut(),
            Pop(popup_view) => popup_view.view_mut(),
        }
    }

    pub fn id(&self) -> ViewId {
        match self {
            Root(view) => view.id(),
            Child(sub_view) => sub_view.id(),
            Pop(popup_view) => popup_view.id(),
        }
    }

    pub fn surface_id(&self) -> ObjectId {
        match self {
            Root(view) => view.surface_id(),
            Child(sub_view) => sub_view.surface_id(),
            Pop(popup_view) => popup_view.surface_id(),
        }
    }
}
