use crate::dpi::{LogicalPosition, LogicalSize};
use crate::view::ViewId;
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::oneshot::Receiver;

#[derive(Default)]
pub struct WindowContext {
    /// 按类型存储全局变量
    pub globals_by_type: FxHashMap<TypeId, Box<dyn Any>>,

    /// 提供给BuildViewFn使用
    pub current_view_id: Option<ViewId>,

    /// BuildViewFn中会添加一些命令，这些命令会在BuildViewFn执行完成后被执行
    pub commands: VecDeque<Command>,
}

pub enum Command {
    HideView(ViewId),
    ResizeView(ViewId, LogicalSize<u32>),
    DropView(ViewId),
    RepositionSubView(ViewId, LogicalPosition<i32>),
    CopyImage(Receiver<Arc<RgbaImage>>),
    SaveImage(Receiver<Arc<RgbaImage>>, bool),
    StartMovingWindow,
    CloseWindow,
}

impl WindowContext {
    pub fn new() -> Self {
        Default::default()
    }
}
