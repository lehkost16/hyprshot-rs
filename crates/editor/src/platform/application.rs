use crate::platform::gpu::GpuContext;
use crate::platform::view::BuildViewFn;
use crate::platform::window::{AppWindow, WindowConfiguration, WindowId};
use crate::platform::wp_fractional_scaling::FractionalScalingManager;
use crate::platform::wp_viewporter::ViewporterState;
use egui::ImeEvent;
use log::{info, warn};
use sctk::compositor::{CompositorHandler, CompositorState};
use sctk::globals::GlobalData;
use sctk::output::{OutputHandler, OutputState};
use sctk::reexports::calloop::{EventLoop, LoopHandle};
use sctk::reexports::calloop_wayland_source::WaylandSource;
use sctk::registry::{ProvidesRegistryState, RegistryState};
use sctk::seat::pointer::{CursorIcon, PointerData, ThemeSpec, ThemedPointer};
use sctk::seat::{
    Capability, SeatHandler, SeatState,
    keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
    pointer::{PointerEvent, PointerEventKind, PointerHandler},
};
use sctk::shell::xdg::XdgShell;
use sctk::shell::xdg::popup::{Popup, PopupConfigure, PopupHandler};
use sctk::shell::xdg::window::{Window, WindowConfigure, WindowHandler};
use sctk::shm::{Shm, ShmHandler};
use sctk::subcompositor::SubcompositorState;
use sctk::{delegate_registry, registry_handlers};
use smithay_clipboard::Clipboard;
use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::wl_keyboard::WlKeyboard;
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::protocol::{wl_output, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::WpCursorShapeDeviceV1;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    ContentHint, ContentPurpose,
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3, zwp_text_input_v3,
};
use wayland_protocols::xdg::shell::client::xdg_positioner::XdgPositioner;

/// GlobalState 存储了 Wayland 的全局状态
pub struct GlobalState {
    pub connection: Connection,
    /// 注册表状态，用于管理 Wayland 全局对象
    pub registry_state: RegistryState,
    /// 输出设备状态（显示器信息）
    pub output_state: OutputState,
    /// 合成器状态
    pub compositor_state: CompositorState,
    /// 子合成器状态，用于管理 SubSurface
    pub sub_compositor_state: SubcompositorState,
    /// 视口管理器（用于调整 Surface 显示尺寸）
    pub viewporter_state: Option<ViewporterState>,
    /// 分数缩放管理器
    pub fractional_scaling_manager: Option<FractionalScalingManager>,
    /// XDG Shell 状态，用于管理窗口
    pub xdg_shell_state: XdgShell,
    /// 队列句柄
    pub queue_handle: QueueHandle<Application>,
    /// GPU 上下文（wgpu相关的全局对象）
    pub gpu: RefCell<Option<GpuContext>>,
    /// 座位状态（管理输入设备）
    pub seat_state: SeatState,
    pub seat: Option<WlSeat>,
    /// 键盘实例
    keyboard: Option<WlKeyboard>,

    /// 指针实例
    pub themed_pointer: Option<ThemedPointer<PointerData<()>>>,
    /// 共享内存（给ThemePointer使用）
    shm_state: Shm,

    /// 事件循环句柄
    loop_handle: LoopHandle<'static, Application>,
    pub text_input_manager: Option<zwp_text_input_manager_v3::ZwpTextInputManagerV3>,
    pub text_input: Option<zwp_text_input_v3::ZwpTextInputV3>,
    /// 记录上一个ImeEvent，以解决在某些平台（比如Niri）下，输入中文标点符号时，没有收到ImeEvent::Preedit就直接收到了ImeEvent::Commit事件的问题
    /// egui看起来不支持“没有ImeEvent::Preedit而直接ImeEvent::Commit的情况”
    previous_ime_event: Option<ImeEvent>,
    pub clipboard: Arc<Clipboard>,
}

/// Application 是应用的核心结构，管理全局状态和窗口列表
pub struct Application {
    pub session: crate::config::EditorSession,
    pub output_directory: std::path::PathBuf,
    pub notifications: bool,
    pub notification_timeout: u32,
    /// 全局状态
    pub global_state: GlobalState,
    /// 应用 ID
    pub app_id: &'static str,
    /// 窗口列表（需要注意，在窗口的绘制过程中，对应窗口会暂时从窗口列表中移除，以便在绘制过程中可变地借用Application的引用）
    windows: Vec<AppWindow>,
    /// 如果为true，那么当所有窗口都关闭的时候退出程序
    exit_when_all_windows_closed: bool,
    event_loop: Option<EventLoop<'static, Application>>,
    should_exit: bool,
}

impl Application {
    /// 初始化 Application，建立 Wayland 连接
    pub fn new(
        app_id: &'static str,
        exit_when_all_windows_closed: bool,
        session: crate::config::EditorSession,
        output_directory: std::path::PathBuf,
        notifications: bool,
        notification_timeout: u32,
    ) -> anyhow::Result<Application> {
        use anyhow::Context;
        let conn = Connection::connect_to_env().context("Can't connect to the Wayland server")?;

        let (globals, event_queue) =
            registry_queue_init(&conn).context("Failed to initialize Wayland registry")?;
        let qh = event_queue.handle();

        let compositor_state =
            CompositorState::bind(&globals, &qh).context("wl_compositor not available")?;
        let sub_compositor_state =
            SubcompositorState::bind(compositor_state.wl_compositor().clone(), &globals, &qh)
                .context("wl_subcompositor not available")?;

        let (viewporter_state, fractional_scaling_manager) =
            if let Ok(fsm) = FractionalScalingManager::new(&globals, &qh) {
                (ViewporterState::new(&globals, &qh).ok(), Some(fsm))
            } else {
                (None, None)
            };
        let event_loop: EventLoop<Application> =
            EventLoop::try_new().context("Failed to initialize the event loop")?;
        let loop_handle = event_loop.handle();
        WaylandSource::new(conn.clone(), event_queue)
            .insert(loop_handle.clone())
            .map_err(|err| anyhow::anyhow!("Failed to attach Wayland event source: {err}"))?;

        let seat_state = SeatState::new(&globals, &qh);
        let shm_state = Shm::bind(&globals, &qh).context("wl_shm not available")?;
        let text_input_manager = globals.bind(&qh, 1..=1, ()).ok();
        let clipboard = unsafe { Clipboard::new(conn.backend().display_ptr() as *mut _) };

        Ok(Self {
            session,
            output_directory,
            notifications,
            notification_timeout,
            global_state: GlobalState {
                connection: conn,
                registry_state: RegistryState::new(&globals),
                output_state: OutputState::new(&globals, &qh),
                compositor_state,
                sub_compositor_state,
                viewporter_state,
                fractional_scaling_manager,
                xdg_shell_state: XdgShell::bind(&globals, &qh)
                    .context("xdg shell not available")?,
                queue_handle: qh,
                gpu: RefCell::new(None),
                seat_state,
                seat: None,
                keyboard: None,
                themed_pointer: None,
                shm_state,
                loop_handle,
                text_input_manager,
                text_input: None,
                previous_ime_event: None,
                #[allow(clippy::arc_with_non_send_sync)]
                clipboard: Arc::new(clipboard),
            },
            app_id,
            windows: vec![],
            exit_when_all_windows_closed,
            should_exit: false,
            event_loop: Some(event_loop),
        })
    }

    /// 新建一个窗口
    pub fn open_window(
        &mut self,
        window_config: WindowConfiguration,
        build_root_view: BuildViewFn,
    ) -> WindowId {
        let global_state = &self.global_state;
        let window = AppWindow::new(global_state, window_config, build_root_view);
        let window_id = window.window_id();
        self.windows.push(window);
        window_id
    }

    /// 根据窗口Id获取窗口实例并使用获取到的窗口实例执行指定的函数
    pub fn with_window_mut<F>(&mut self, window_id: WindowId, func: F)
    where
        F: FnOnce(&GlobalState, &mut Option<&mut AppWindow>),
    {
        let mut target_window_idx = None;
        for (idx, w) in self.windows.iter().enumerate() {
            if w.window_id() == window_id {
                target_window_idx = Some(idx);
                break;
            }
        }
        let mut window = if let Some(idx) = target_window_idx {
            Some(&mut self.windows[idx])
        } else {
            None
        };
        let global_state = &self.global_state;
        func(global_state, &mut window);
    }

    fn find_window_index<F>(&self, predicate: F) -> Option<usize>
    where
        F: Fn(&AppWindow) -> bool,
    {
        self.windows.iter().position(predicate)
    }

    pub fn scale_factor_changed(
        &mut self,
        surface: &WlSurface,
        scale_factor: f64,
        is_legacy: bool,
    ) {
        let supports_fractional_scaling = self.global_state.fractional_scaling_manager.is_some();
        if is_legacy && supports_fractional_scaling {
            // 使用分数缩放的情况下忽略整数缩放倍数
            return;
        }
        info!(
            "scale factor of {}  changed to {}",
            surface.id(),
            scale_factor
        );

        let window_index = self.find_window_index(|window| window.contains_surface(surface));
        let Some(window_index) = window_index else {
            return;
        };

        // scale 变化会影响逻辑尺寸和 egui pixels_per_point，需要触发一次重绘/重布局。
        let needs_draw;

        {
            let window = &mut self.windows[window_index];

            // 如果窗口的scale_factor不存在，意味着窗口尚未开始绘制
            let is_first_draw_pending = window.scale_factor().is_none();

            let gpu_context = self.global_state.gpu.borrow();
            let gpu_context = gpu_context.as_ref().unwrap();
            window.set_scale_factor(scale_factor, gpu_context);

            needs_draw = window.configured || is_first_draw_pending;
        }

        if needs_draw {
            self.draw_window(window_index);
        }
    }

    fn draw_window(&mut self, window_index: usize) {
        let mut window = self.windows.remove(window_index);

        window.draw(self);

        if !window.should_remove {
            self.windows.push(window);
        }
        if self.windows.is_empty() && self.exit_when_all_windows_closed {
            self.should_exit = true;
        }
    }

    pub fn screen_size(&self) -> (u32, u32) {
        let mut max_width = 0;
        let mut max_height = 0;
        for output in self.global_state.output_state.outputs() {
            if let Some(info) = self.global_state.output_state.info(&output) {
                if let Some((w, h)) = info.logical_size {
                    if w > 0 && h > 0 {
                        max_width = max_width.max(w as u32);
                        max_height = max_height.max(h as u32);
                    }
                } else if let Some(mode) = info.modes.first() {
                    let scale = info.scale_factor.max(1) as u32;
                    let w = mode.dimensions.0 as u32 / scale;
                    let h = mode.dimensions.1 as u32 / scale;
                    max_width = max_width.max(w);
                    max_height = max_height.max(h);
                }
            }
        }
        if max_width > 0 && max_height > 0 {
            (max_width, max_height)
        } else {
            (1920, 1080)
        }
    }

    pub fn screen_scale_factor(&self) -> f64 {
        let mut max_scale = 1.0f64;
        for output in self.global_state.output_state.outputs() {
            if let Some(info) = self.global_state.output_state.info(&output) {
                max_scale = max_scale.max(info.scale_factor as f64);
            }
        }
        if max_scale > 0.0 { max_scale } else { 1.0 }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        use anyhow::Context;
        let mut event_loop = self
            .event_loop
            .take()
            .context("Editor event loop already consumed")?;
        loop {
            event_loop
                .dispatch(Duration::from_millis(16), self)
                .context("Editor event loop failed")?;
            if self.should_exit || (self.exit_when_all_windows_closed && self.windows.is_empty()) {
                break;
            }
        }
        Ok(())
    }
}

delegate_registry!(Application);
sctk::delegate_dispatch2!(Application);
delegate_noop!(Application: ignore  WlRegion);

delegate_noop!(Application: ignore zwp_text_input_manager_v3::ZwpTextInputManagerV3);

impl CompositorHandler for Application {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &WlSurface,
        new_factor: i32,
    ) {
        self.scale_factor_changed(surface, new_factor as f64, true);
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_transform: wl_output::Transform,
    ) {
        info!("transform changed");
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &WlSurface,
        _time: u32,
    ) {
        let window_index = self.find_window_index(|window| window.contains_surface(surface));
        let Some(window_index) = window_index else {
            return;
        };

        let needs_draw;
        {
            let window = &self.windows[window_index];

            // 只在主 Surface (root_surface) 的帧回调到达时触发重绘
            // 这样可以保证渲染频率与显示刷新率同步，避免过度提交
            needs_draw = window.root_surface() == surface;
        }

        if needs_draw {
            self.draw_window(window_index);
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        info!("Surface entered");
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        info!("Surface leaved");
    }
}

impl OutputHandler for Application {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.global_state.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl ProvidesRegistryState for Application {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.global_state.registry_state
    }

    registry_handlers!(OutputState);
}

impl WindowHandler for Application {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        self.windows.retain(|v| v.xdg_window() != window);
        if self.windows.is_empty() && self.exit_when_all_windows_closed {
            self.should_exit = true;
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        info!("Window configured to: {:?}", configure);

        let window_index = self.find_window_index(|app_window| app_window.xdg_window() == window);
        let Some(window_index) = window_index else {
            return;
        };

        let fallback_scale = self.screen_scale_factor();
        let needs_draw;
        {
            let window = &mut self.windows[window_index];
            if window.scale_factor().is_none() {
                let gpu = self.global_state.gpu.borrow();
                if let Some(gpu_context) = gpu.as_ref() {
                    window.set_scale_factor(fallback_scale, gpu_context);
                }
            }
            needs_draw = !window.configured;
            window.configured = true;
        }

        if needs_draw {
            self.draw_window(window_index);
        }
    }
}

impl PopupHandler for Application {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        popup: &Popup,
        config: PopupConfigure,
    ) {
        // config 中包含了合成器确定的最终位置和尺寸
        info!(
            "Popup {:?} configured to: {:?}",
            popup.xdg_surface().id(),
            config
        );
        let window_index = self
            .windows
            .iter()
            .position(|w| w.contains_surface(popup.wl_surface()));
        if let Some(window_index) = window_index {
            let window = &mut self.windows[window_index];
            let gpu = self.global_state.gpu.borrow();
            let gpu = gpu.as_ref().unwrap();
            window.configure_popup(popup, &config, gpu);
        }
    }

    fn done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, popup: &Popup) {
        info!("Popup done: {:?}", popup.xdg_surface().id());
        // 弹出框被合成器关闭（例如用户点击外部）
        let window_index = self
            .windows
            .iter()
            .position(|w| w.contains_surface(popup.wl_surface()));
        if let Some(window_index) = window_index {
            let window = &mut self.windows[window_index];
            window.remove_popup(popup);
        }
    }
}

impl Dispatch<XdgPositioner, ()> for Application {
    fn event(
        _state: &mut Self,
        _proxy: &XdgPositioner,
        event: <XdgPositioner as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        info!("XdgPositioner event {:?}", event);
    }
}

impl SeatHandler for Application {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.global_state.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.global_state.keyboard.is_none() {
            info!("Set keyboard capability");
            let keyboard = self
                .global_state
                .seat_state
                .get_keyboard_with_repeat(
                    qh,
                    &seat,
                    None,
                    self.global_state.loop_handle.clone(),
                    Box::new(|_state, _wl_kbd, event| {
                        info!("Repeat: {:?} ", event);
                    }),
                )
                .expect("Failed to create keyboard");

            self.global_state.keyboard = Some(keyboard);

            self.global_state.text_input = self
                .global_state
                .text_input_manager
                .as_ref()
                .map(|text_input_manager| text_input_manager.get_text_input(&seat, qh, ()));
        }

        if capability == Capability::Pointer && self.global_state.themed_pointer.is_none() {
            info!("Set pointer capability");
            let surface = self.global_state.compositor_state.create_surface(qh);
            let pointer_data = PointerData::new(seat.clone(), ());
            let themed_pointer = self
                .global_state
                .seat_state
                .get_pointer_with_theme_and_data(
                    qh,
                    &seat,
                    self.global_state.shm_state.wl_shm(),
                    surface,
                    ThemeSpec::default(),
                    pointer_data,
                )
                .expect("Failed to create pointer");
            self.global_state.themed_pointer.replace(themed_pointer);
        }
        if self.global_state.seat.is_none() {
            self.global_state.seat = Some(seat);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.global_state.keyboard.is_some() {
            info!("Unset keyboard capability");
            self.global_state.keyboard.take().unwrap().release();
        }

        if capability == Capability::Pointer && self.global_state.themed_pointer.is_some() {
            info!("Unset pointer capability");
            self.global_state
                .themed_pointer
                .take()
                .unwrap()
                .pointer()
                .release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl KeyboardHandler for Application {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        surface: &WlSurface,
        _: u32,
        _: &[u32],
        _keysyms: &[Keysym],
    ) {
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.contains_surface(surface));

        if let Some(window) = window {
            window.set_keyboard_focus(true);
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        let window = self
            .windows
            .iter_mut()
            .find(|w| w.contains_surface(surface));

        if let Some(window) = window {
            window.set_keyboard_focus(false);
        }
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        let single_window = self.windows.len() == 1;
        for window in &mut self.windows {
            if window.keyboard_focus() || single_window {
                window.handle_keyboard_event(event.clone(), serial, true, false);
            }
        }
    }

    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        let single_window = self.windows.len() == 1;
        for window in &mut self.windows {
            if window.keyboard_focus() || single_window {
                window.handle_keyboard_event(event.clone(), serial, true, true);
            }
        }
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        let single_window = self.windows.len() == 1;
        for window in &mut self.windows {
            if window.keyboard_focus() || single_window {
                window.handle_keyboard_event(event.clone(), serial, false, false);
            }
        }
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        raw_modifiers: RawModifiers,
        _layout: u32,
    ) {
        log::info!(
            "[DEBUG] Wayland update_modifiers: modifiers={:?}, raw_modifiers={:?}",
            modifiers,
            raw_modifiers
        );
        let single_window = self.windows.len() == 1;
        for window in &mut self.windows {
            if window.keyboard_focus() || single_window {
                window.update_modifiers(modifiers);
            }
        }
    }
}

impl PointerHandler for Application {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &WlPointer,
        events: &[PointerEvent], // 指针事件使用的是逻辑坐标
    ) {
        for event in events {
            // 找到事件对应的窗口
            let mut target_window_idx = None;
            for (idx, w) in self.windows.iter().enumerate() {
                if w.contains_surface(&event.surface) {
                    target_window_idx = Some(idx);
                    break;
                }
            }

            let enter_event = matches!(event.kind, PointerEventKind::Enter { .. });
            if let Some(themed_cursor) = self.global_state.themed_pointer.as_ref()
                && enter_event
            {
                let connection = &self.global_state.connection;
                if let Err(e) = themed_cursor.set_cursor(connection, CursorIcon::Default) {
                    warn!("Failed tp set cursor: {:?}", e);
                }
            }

            if let Some(idx) = target_window_idx {
                self.windows[idx].handle_pointer_event(event, &self.global_state);
            }
        }
    }
}

impl Dispatch<zwp_text_input_v3::ZwpTextInputV3, ()> for Application {
    fn event(
        this: &mut Self,
        _text_input: &zwp_text_input_v3::ZwpTextInputV3,
        event: <zwp_text_input_v3::ZwpTextInputV3 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_text_input_v3::Event::Enter { .. } => {
                let Some(text_input) = this.global_state.text_input.take() else {
                    return;
                };

                text_input.enable();
                text_input.set_content_type(ContentHint::None, ContentPurpose::Normal);

                for window in &mut this.windows {
                    if window.keyboard_focus() {}
                }
                text_input.commit();

                this.global_state.text_input = Some(text_input);
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                if let Some(text_input) = &this.global_state.text_input {
                    text_input.disable();
                    text_input.commit();
                }

                for window in &mut this.windows {
                    if window.keyboard_focus() {}
                }
                this.global_state.previous_ime_event = None;
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                let Some(text) = text else {
                    return;
                };
                if !matches!(
                    this.global_state.previous_ime_event,
                    Some(ImeEvent::Preedit { .. })
                ) {
                    for window in &mut this.windows {
                        if window.keyboard_focus() {
                            this.global_state.previous_ime_event = Some(ImeEvent::Preedit {
                                text: "".to_string(),
                                active_range_chars: None,
                            });
                            window.handle_ime_event(&ImeEvent::Preedit {
                                text: "".to_string(),
                                active_range_chars: None,
                            });
                        }
                    }
                }

                for window in &mut this.windows {
                    if window.keyboard_focus() {
                        this.global_state.previous_ime_event = Some(ImeEvent::Commit(text.clone()));
                        window.handle_ime_event(&ImeEvent::Commit(text.clone()));
                    }
                }
            }
            zwp_text_input_v3::Event::PreeditString { text, .. } => {
                // egui看起来不支持“没有ImeEvent::Preedit而直接ImeEvent::Commit的情况”，
                // 然而在输入中文标点符号的时候，Event::PreeditString的text为None，
                // 为了避免无法输入中文标点符号，这里替换成空字符串
                let text = text.unwrap_or("".to_string());

                for window in &mut this.windows {
                    if window.keyboard_focus() {
                        this.global_state.previous_ime_event = Some(ImeEvent::Preedit {
                            text: text.clone(),
                            active_range_chars: None,
                        });
                        window.handle_ime_event(&ImeEvent::Preedit {
                            text: text.clone(),
                            active_range_chars: None,
                        });
                    }
                }
            }
            zwp_text_input_v3::Event::Done { .. } => {}
            _ => {}
        }
    }
}

impl ShmHandler for Application {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.global_state.shm_state
    }
}

impl Dispatch<WpCursorShapeDeviceV1, GlobalData, Application> for SeatState {
    fn event(
        _: &mut Application,
        _: &WpCursorShapeDeviceV1,
        _: <WpCursorShapeDeviceV1 as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Application>,
    ) {
        unreachable!("wp_cursor_shape_manager has no events")
    }
}

impl Dispatch<WpCursorShapeManagerV1, GlobalData, Application> for SeatState {
    fn event(
        _: &mut Application,
        _: &WpCursorShapeManagerV1,
        _: <WpCursorShapeManagerV1 as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<Application>,
    ) {
        unreachable!("wp_cursor_device_manager has no events")
    }
}
