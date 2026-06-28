use anyhow::{Context, Result};

#[cfg(all(target_os = "linux", feature = "freeze"))]
mod imp {
    use super::*;
    use grim_rs::Grim;
    use std::{
        os::fd::{AsRawFd, BorrowedFd},
        sync::mpsc,
        thread,
        time::Duration,
    };
    use serde_json::Value;
    use std::process::Command;
    use crate::utils::output_with_timeout;
    use wayland_client::{
        Connection, Dispatch, QueueHandle,
        protocol::{
            wl_buffer::WlBuffer,
            wl_compositor::WlCompositor,
            wl_output::Mode as WlOutputMode,
            wl_output::WlOutput,
            wl_region::WlRegion,
            wl_registry::WlRegistry,
            wl_shm::{self, WlShm},
            wl_shm_pool::WlShmPool,
            wl_surface::WlSurface,
        },
    };
    use wayland_protocols::xdg::xdg_output::zv1::client::{
        zxdg_output_manager_v1::ZxdgOutputManagerV1, zxdg_output_v1::ZxdgOutputV1,
    };
    use wayland_protocols_wlr::layer_shell::v1::client::{
        zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
        zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
    };

    pub enum FreezeGuardType {
        Native {
            stop_tx: mpsc::Sender<()>,
            join: Option<thread::JoinHandle<Result<()>>>,
        },
        Hyprpicker {
            child: std::process::Child,
        },
    }

    pub struct FreezeGuard {
        pub guard: FreezeGuardType,
    }

    impl FreezeGuard {
        pub fn stop(mut self) -> Result<()> {
            match &mut self.guard {
                FreezeGuardType::Native { stop_tx, join } => {
                    let _ = stop_tx.send(());
                    if let Some(j) = join.take() {
                        return j
                            .join()
                            .unwrap_or_else(|_| Err(anyhow::anyhow!("Freeze thread panicked")));
                    }
                    Ok(())
                }
                FreezeGuardType::Hyprpicker { child } => {
                    let _ = child.kill();
                    let _ = child.wait();
                    Ok(())
                }
            }
        }
    }

    impl Drop for FreezeGuard {
        fn drop(&mut self) {
            match &mut self.guard {
                FreezeGuardType::Native { stop_tx, join } => {
                    let _ = stop_tx.send(());
                    if let Some(j) = join.take() {
                        let _ = j.join();
                    }
                }
                FreezeGuardType::Hyprpicker { child } => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }

    #[derive(Clone)]
    struct GrimOutputMeta {
        name: String,
        geom: (i32, i32, i32, i32),
    }

    struct CaptureImage {
        data: Vec<u8>,
        width: u32,
        height: u32,
    }

    #[derive(Debug, Clone)]
    struct MonitorInfo {
        name: String,
        x: i32,
        y: i32,
        logical_w: i32,
        logical_h: i32,
        scale: f64,
    }

    fn get_all_monitors_info() -> Vec<MonitorInfo> {
        const IPC_TIMEOUT: Duration = Duration::from_secs(3);
        // Try Hyprland
        if let Ok(output) = output_with_timeout(
            {
                let mut cmd = Command::new("hyprctl");
                cmd.arg("monitors").arg("-j");
                cmd
            },
            IPC_TIMEOUT,
        ) {
            if let Ok(monitors) = serde_json::from_slice::<Value>(&output.stdout) {
                if let Some(arr) = monitors.as_array() {
                    let mut list = Vec::new();
                    for m in arr {
                        let name = m["name"].as_str().unwrap_or("").to_string();
                        let x = m["x"].as_i64().unwrap_or(0) as i32;
                        let y = m["y"].as_i64().unwrap_or(0) as i32;
                        let logical_w = m["width"].as_i64().unwrap_or(0) as i32;
                        let logical_h = m["height"].as_i64().unwrap_or(0) as i32;
                        let scale = m["scale"].as_f64().unwrap_or(1.0);
                        list.push(MonitorInfo {
                            name,
                            x,
                            y,
                            logical_w,
                            logical_h,
                            scale,
                        });
                    }
                    return list;
                }
            }
        }
        // Try Sway
        if let Ok(output) = output_with_timeout(
            {
                let mut cmd = Command::new("swaymsg");
                cmd.arg("-t").arg("get_outputs");
                cmd
            },
            IPC_TIMEOUT,
        ) {
            if let Ok(outputs) = serde_json::from_slice::<Value>(&output.stdout) {
                if let Some(arr) = outputs.as_array() {
                    let mut list = Vec::new();
                    for o in arr {
                        let name = o["name"].as_str().unwrap_or("").to_string();
                        let rect = &o["rect"];
                        let x = rect["x"].as_i64().unwrap_or(0) as i32;
                        let y = rect["y"].as_i64().unwrap_or(0) as i32;
                        let logical_w = rect["width"].as_i64().unwrap_or(0) as i32;
                        let logical_h = rect["height"].as_i64().unwrap_or(0) as i32;
                        let scale = o["scale"].as_f64().unwrap_or(1.0);
                        list.push(MonitorInfo {
                            name,
                            x,
                            y,
                            logical_w,
                            logical_h,
                            scale,
                        });
                    }
                    return list;
                }
            }
        }
        Vec::new()
    }

    pub fn start_freeze(selected_output: Option<&str>, debug: bool) -> Result<FreezeGuard> {
        // If on Hyprland, try to use hyprpicker -r -z as it natively supports Hyprland's internal scaling/oversampling layout
        if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
            if debug {
                eprintln!("Freeze: detected Hyprland session, attempting to spawn hyprpicker");
            }
            match Command::new("hyprpicker")
                .arg("-r")
                .arg("-z")
                .spawn()
            {
                Ok(child) => {
                    if debug {
                        eprintln!("Freeze: successfully spawned hyprpicker -r -z");
                    }
                    // Wait a short duration for the overlay to map
                    thread::sleep(Duration::from_millis(200));
                    return Ok(FreezeGuard {
                        guard: FreezeGuardType::Hyprpicker { child },
                    });
                }
                Err(e) => {
                    if debug {
                        eprintln!("Freeze: failed to spawn hyprpicker ({}). Falling back to native.", e);
                    }
                }
            }
        }

        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();

        let selected_output = selected_output.map(str::to_string);
        let mut join = Some(thread::spawn(move || {
            run_freeze(selected_output, stop_rx, ready_tx, debug)
        }));
        const FREEZE_READY_TIMEOUT: Duration = Duration::from_secs(5);

        match ready_rx.recv_timeout(FREEZE_READY_TIMEOUT) {
            Ok(Ok(())) => {
                if debug {
                    eprintln!("Freeze overlay initialized");
                }
                Ok(FreezeGuard {
                    guard: FreezeGuardType::Native { stop_tx, join },
                })
            }
            Ok(Err(err)) => {
                eprintln!("Freeze disabled: {}", err);
                if let Some(join) = join.take() {
                    let _ = join.join();
                }
                Ok(FreezeGuard {
                    guard: FreezeGuardType::Native {
                        stop_tx,
                        join: None,
                    },
                })
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = stop_tx.send(());
                if debug {
                    eprintln!(
                        "Freeze startup timed out after {:?}; proceeding with explicit error",
                        FREEZE_READY_TIMEOUT
                    );
                }
                Err(anyhow::anyhow!(
                    "Freeze initialization timed out after {:?}",
                    FREEZE_READY_TIMEOUT
                ))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(join) = join.take() {
                    match join.join() {
                        Ok(Ok(())) => {}
                        Ok(Err(err)) => return Err(err),
                        Err(_) => return Err(anyhow::anyhow!("Freeze thread panicked")),
                    }
                }
                Err(anyhow::anyhow!(
                    "Freeze initialization channel disconnected"
                ))
            }
        }
    }

    #[derive(Debug)]
    struct OutputKey(usize);

    #[derive(Debug)]
    struct SurfaceKey(usize);

    struct OutputEntry {
        output: WlOutput,
        name: Option<String>,
        xdg_output: Option<ZxdgOutputV1>,
        pos_x: Option<i32>,
        pos_y: Option<i32>,
        mode_width: Option<i32>,
        mode_height: Option<i32>,
        scale: i32,
        logical_x: Option<i32>,
        logical_y: Option<i32>,
        logical_width: Option<i32>,
        logical_height: Option<i32>,
    }

    struct SurfaceEntry {
        surface: WlSurface,
        layer_surface: ZwlrLayerSurfaceV1,
        buffer: Option<WlBuffer>,
        _input_region: WlRegion,
        _tmp: Option<tempfile::NamedTempFile>,
        _mmap: Option<memmap2::MmapMut>,
        configured: bool,
        configured_w: Option<u32>,
        configured_h: Option<u32>,
        output_idx: usize,
    }

    struct State {
        compositor: Option<WlCompositor>,
        shm: Option<WlShm>,
        layer_shell: Option<ZwlrLayerShellV1>,
        xdg_output_manager: Option<ZxdgOutputManagerV1>,
        outputs: Vec<OutputEntry>,
        surfaces: Vec<SurfaceEntry>,
        monitors_info: Vec<MonitorInfo>,
    }

    impl Dispatch<WlRegistry, ()> for State {
        fn event(
            state: &mut Self,
            registry: &WlRegistry,
            event: wayland_client::protocol::wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            if let wayland_client::protocol::wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            {
                match interface.as_str() {
                    "wl_compositor" => {
                        state.compositor = Some(registry.bind(name, version.min(5), qh, ()));
                    }
                    "wl_shm" => {
                        state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                    }
                    "zwlr_layer_shell_v1" => {
                        state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
                    }
                    "wl_output" => {
                        let idx = state.outputs.len();
                        let output = registry.bind::<WlOutput, _, _>(
                            name,
                            version.min(4),
                            qh,
                            OutputKey(idx),
                        );
                        state.outputs.push(OutputEntry {
                            output,
                            name: None,
                            xdg_output: None,
                            pos_x: None,
                            pos_y: None,
                            mode_width: None,
                            mode_height: None,
                            scale: 1,
                            logical_x: None,
                            logical_y: None,
                            logical_width: None,
                            logical_height: None,
                        });
                    }
                    "zxdg_output_manager_v1" => {
                        state.xdg_output_manager =
                            Some(registry.bind(name, version.min(3), qh, ()));
                    }
                    _ => {}
                }
            }
        }
    }

    impl Dispatch<WlOutput, OutputKey> for State {
        fn event(
            state: &mut Self,
            _: &WlOutput,
            event: wayland_client::protocol::wl_output::Event,
            data: &OutputKey,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let Some(entry) = state.outputs.get_mut(data.0) else {
                return;
            };
            match event {
                wayland_client::protocol::wl_output::Event::Geometry { x, y, .. } => {
                    entry.pos_x = Some(x);
                    entry.pos_y = Some(y);
                }
                wayland_client::protocol::wl_output::Event::Mode {
                    flags,
                    width,
                    height,
                    ..
                } => {
                    let is_current = match flags {
                        wayland_client::WEnum::Value(f) => f.contains(WlOutputMode::Current),
                        wayland_client::WEnum::Unknown(_) => false,
                    };
                    if is_current {
                        entry.mode_width = Some(width);
                        entry.mode_height = Some(height);
                    }
                }
                wayland_client::protocol::wl_output::Event::Scale { factor } => {
                    entry.scale = factor.max(1);
                }
                wayland_client::protocol::wl_output::Event::Name { name } => {
                    entry.name = Some(name.clone());
                    if let Some(info) = state.monitors_info.iter().find(|info| info.name == name) {
                        entry.logical_x = Some(info.x);
                        entry.logical_y = Some(info.y);
                        entry.logical_width = Some(info.logical_w);
                        entry.logical_height = Some(info.logical_h);
                        let int_scale = if (info.scale - info.scale.round()).abs() < 0.01 {
                            info.scale.round() as i32
                        } else {
                            1
                        };
                        entry.scale = int_scale;
                    }
                }
                _ => {}
            }
        }
    }

    impl Dispatch<ZxdgOutputV1, OutputKey> for State {
        fn event(
            state: &mut Self,
            _: &ZxdgOutputV1,
            event: wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::Event,
            data: &OutputKey,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let Some(entry) = state.outputs.get_mut(data.0) else {
                return;
            };
            match event {
                wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::Event::LogicalPosition { x, y } => {
                    entry.logical_x = Some(x);
                    entry.logical_y = Some(y);
                }
                wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::Event::LogicalSize { width, height } => {
                    entry.logical_width = Some(width);
                    entry.logical_height = Some(height);
                }
                wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::Event::Name {
                    name,
                } => {
                    entry.name = Some(name);
                }
                _ => {}
            }
        }
    }

    impl Dispatch<ZwlrLayerSurfaceV1, SurfaceKey> for State {
        fn event(
            state: &mut Self,
            surface: &ZwlrLayerSurfaceV1,
            event: wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event,
            data: &SurfaceKey,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let Some(entry) = state.surfaces.get_mut(data.0) {
                match event {
                    wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event::Configure {
                        serial,
                        width,
                        height,
                    } => {
                        surface.ack_configure(serial);
                        entry.configured = true;
                        entry.configured_w = Some(width);
                        entry.configured_h = Some(height);
                    }
                    wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event::Closed => {
                        entry.configured = false;
                    }
                    _ => {}
                }
            }
        }
    }

    impl Dispatch<WlCompositor, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlCompositor,
            _: wayland_client::protocol::wl_compositor::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlShm, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlShm,
            _: wayland_client::protocol::wl_shm::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlShmPool, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlShmPool,
            _: wayland_client::protocol::wl_shm_pool::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlSurface, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlSurface,
            _: wayland_client::protocol::wl_surface::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlBuffer, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlBuffer,
            _: wayland_client::protocol::wl_buffer::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<WlRegion, ()> for State {
        fn event(
            _: &mut Self,
            _: &WlRegion,
            _: wayland_client::protocol::wl_region::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwlrLayerShellV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &ZwlrLayerShellV1,
            _: wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZxdgOutputManagerV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &ZxdgOutputManagerV1,
            _: wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    fn run_freeze(
        selected_output: Option<String>,
        stop_rx: mpsc::Receiver<()>,
        ready_tx: mpsc::Sender<Result<()>>,
        debug: bool,
    ) -> Result<()> {
        if debug {
            eprintln!("Freeze: connect to Wayland");
        }
        let conn = Connection::connect_to_env().context("Failed to connect to Wayland")?;
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();

        let registry = conn.display().get_registry(&qh, ());

        let mut state = State {
            compositor: None,
            shm: None,
            layer_shell: None,
            xdg_output_manager: None,
            outputs: Vec::new(),
            surfaces: Vec::new(),
            monitors_info: get_all_monitors_info(),
        };

        event_queue
            .roundtrip(&mut state)
            .context("Failed to initialize Wayland globals")?;
        if debug {
            eprintln!("Freeze: Wayland globals initialized");
        }

        if let Some(manager) = &state.xdg_output_manager {
            for (idx, entry) in state.outputs.iter_mut().enumerate() {
                let xdg_output = manager.get_xdg_output(&entry.output, &qh, OutputKey(idx));
                entry.xdg_output = Some(xdg_output);
            }
        }

        event_queue
            .roundtrip(&mut state)
            .context("Failed to receive output configuration events")?;
        if debug {
            eprintln!("Freeze: received output configuration events");
        }

        // Match Wayland outputs with compositor monitor info
        for entry in &mut state.outputs {
            // 1. Try matching by name (most reliable, works if Name event was received)
            let mut matched = false;
            if let Some(name) = &entry.name {
                if let Some(info) = state.monitors_info.iter().find(|info| info.name == *name) {
                    if debug {
                        eprintln!("Freeze: matched output '{}' via name", info.name);
                    }
                    entry.logical_x = Some(info.x);
                    entry.logical_y = Some(info.y);
                    entry.logical_width = Some(info.logical_w);
                    entry.logical_height = Some(info.logical_h);
                    matched = true;
                }
            }

            // 2. Fallback to match by position and physical size
            if !matched {
                if let (Some(pos_x), Some(pos_y), Some(mode_w), Some(mode_h)) = (entry.pos_x, entry.pos_y, entry.mode_width, entry.mode_height) {
                    if let Some(info) = state.monitors_info.iter().find(|info| {
                        let phys_w = (info.logical_w as f64 * info.scale).round() as i32;
                        let phys_h = (info.logical_h as f64 * info.scale).round() as i32;
                        (info.x - pos_x).abs() < 5
                            && (info.y - pos_y).abs() < 5
                            && (phys_w - mode_w).abs() < 5
                            && (phys_h - mode_h).abs() < 5
                    }) {
                        if debug {
                            eprintln!("Freeze: matched output '{}' via position & size", info.name);
                        }
                        entry.name = Some(info.name.clone());
                        entry.logical_x = Some(info.x);
                        entry.logical_y = Some(info.y);
                        entry.logical_width = Some(info.logical_w);
                        entry.logical_height = Some(info.logical_h);
                    }
                }
            }
        }

        if debug {
            eprintln!("Freeze: checking required globals");
        }
        let compositor = state
            .compositor
            .as_ref()
            .context("wl_compositor not available")?
            .clone();
        let shm = state.shm.as_ref().context("wl_shm not available")?.clone();
        let layer_shell = match state.layer_shell.as_ref() {
            Some(shell) => shell.clone(),
            None => {
                // FIXME: нужно проверить поддержку wlr-layer-shell на Hyprland/Sway/River/Wayfire.
                eprintln!(
                    "Freeze is disabled: compositor does not support wlr-layer-shell. \
Check the support for this protocol on Hyprland/Sway/River/Wayfire."
                );
                let _ = ready_tx.send(Ok(()));
                return Ok(());
            }
        };
        if debug {
            eprintln!("Freeze: required globals are available");
        }

        // Some compositors may not report frame callbacks for a temporary surface
        // in this context. Skip pre-sync to avoid blocking freeze startup.
        if debug {
            eprintln!("Freeze: pre-sync skipped");
        }

        let mut grim = match Grim::new() {
            Ok(grim) => grim,
            Err(err) if is_missing_screencopy_msg(&err.to_string()) => {
                // FIXME: нужно проверить поддержку wlr-screencopy на Hyprland/Sway/River/Wayfire.
                eprintln!(
                    "Freeze is disabled: compositor does not support wlr-screencopy. \
        Check the support for this protocol on Hyprland/Sway/River/Wayfire."
                );
                let _ = ready_tx.send(Ok(()));
                return Ok(());
            }
            Err(err) => {
                let _ = ready_tx.send(Err(err.into()));
                return Ok(());
            }
        };

        if stop_rx.try_recv().is_ok() {
            let _ = ready_tx.send(Ok(()));
            return Ok(());
        }

        if debug {
            eprintln!("Freeze: querying outputs via grim-rs");
        }
        let grim_outputs = grim
            .get_outputs()
            .context("Failed to list outputs via grim-rs")?;
        let mut metas = Vec::new();
        for output in grim_outputs {
            metas.push(GrimOutputMeta {
                name: output.name().to_string(),
                geom: (
                    output.geometry().x(),
                    output.geometry().y(),
                    output.geometry().width(),
                    output.geometry().height(),
                ),
            });
        }

        let mapping = match_outputs(&state.outputs, &metas, selected_output.as_deref())?;
        if mapping.iter().all(|m| m.is_none()) {
            let _ = ready_tx.send(Err(anyhow::anyhow!(
                "No matching outputs found for freeze overlay"
            )));
            return Ok(());
        }
        if debug {
            eprintln!("Freeze: output mapping prepared");
        }

        let mut captures = Vec::new();

        for (idx, meta_index) in mapping.into_iter().enumerate() {
            if stop_rx.try_recv().is_ok() {
                let _ = ready_tx.send(Ok(()));
                return Ok(());
            }
            let Some(meta_index) = meta_index else {
                continue;
            };
            let output = &state.outputs[idx];
            let meta = &metas[meta_index];

            let capture = grim
                .capture_output(&meta.name)
                .with_context(|| format!("Failed to capture output '{}'", meta.name))?;

            if debug {
                eprintln!(
                    "Freeze capture: {} ({}x{})",
                    meta.name,
                    capture.width(),
                    capture.height()
                );
            }

            let width = capture.width();
            let height = capture.height();
            let capture_img = CaptureImage {
                data: capture.into_data(),
                width,
                height,
            };
            captures.push(capture_img);

            let surface_idx = state.surfaces.len();
            let surface = compositor.create_surface(&qh, ());
            let layer_surface = layer_shell.get_layer_surface(
                &surface,
                Some(&output.output),
                Layer::Overlay,
                "hyprshot-freeze".to_string(),
                &qh,
                SurfaceKey(surface_idx),
            );

            layer_surface.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
            layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer_surface.set_exclusive_zone(-1);

            let input_region = compositor.create_region(&qh, ());
            surface.set_input_region(Some(&input_region));

            surface.commit();

            state.surfaces.push(SurfaceEntry {
                surface,
                layer_surface,
                buffer: None,
                _input_region: input_region,
                _tmp: None,
                _mmap: None,
                configured: false,
                configured_w: None,
                configured_h: None,
                output_idx: idx,
            });
        }

        if state.surfaces.is_empty() {
            let _ = ready_tx.send(Err(anyhow::anyhow!(
                "No matching outputs found for freeze overlay"
            )));
            return Ok(());
        }

        if debug {
            eprintln!("Freeze: waiting for layer-surface configure");
        }
        event_queue
            .roundtrip(&mut state)
            .context("Failed to configure freeze surfaces")?;

        for (surface_idx, entry) in state.surfaces.iter_mut().enumerate() {
            let output = &state.outputs[entry.output_idx];
            let capture = &captures[surface_idx];

            let logical_w = entry.configured_w.map(|w| w as i32)
                .or_else(|| output.logical_width)
                .unwrap_or(0);
            let logical_h = entry.configured_h.map(|h| h as i32)
                .or_else(|| output.logical_height)
                .unwrap_or(0);

            let buffer_scale = output_buffer_scale(output);

            let width = capture.width;
            let height = capture.height;
            let mut raw_data = capture.data.clone();

            // Calculate target physical size based on logical size and compositor scale
            let (target_w, target_h) = if logical_w > 0 && logical_h > 0 {
                ((logical_w * buffer_scale) as u32, (logical_h * buffer_scale) as u32)
            } else {
                (width, height)
            };

            if width != target_w || height != target_h {
                if debug {
                    eprintln!(
                        "Resizing captured image for output {} from {}x{} to target buffer size {}x{} (scale={})",
                        output.name.as_deref().unwrap_or(""), width, height, target_w, target_h, buffer_scale
                    );
                }
                if let Some(img_buf) = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(width, height, raw_data.clone()) {
                    let resized = image::imageops::resize(
                        &img_buf,
                        target_w,
                        target_h,
                        image::imageops::FilterType::Nearest,
                    );
                    raw_data = resized.into_raw();
                }
            }

            let capture_img = CaptureImage {
                data: raw_data,
                width: target_w,
                height: target_h,
            };

            let (buffer, tmp, mmap) = create_buffer(&shm, &qh, &capture_img).with_context(|| {
                format!(
                    "Failed to create buffer for output '{}'",
                    output.name.as_deref().unwrap_or("")
                )
            })?;

            if buffer_scale > 1 {
                entry.surface.set_buffer_scale(buffer_scale);
            }

            entry.surface.attach(Some(&buffer), 0, 0);
            entry.surface.commit();

            entry.buffer = Some(buffer);
            entry._tmp = Some(tmp);
            entry._mmap = Some(mmap);
        }

        conn.flush().ok();
        if debug {
            eprintln!("Freeze: overlay committed");
        }

        let _ = ready_tx.send(Ok(()));

        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }
            event_queue.roundtrip(&mut state).ok();
        }

        if debug {
            eprintln!("Freeze overlay stopped");
        }

        for entry in state.surfaces {
            entry.layer_surface.destroy();
            entry.surface.destroy();
            if let Some(buffer) = entry.buffer {
                buffer.destroy();
            }
        }
        drop(registry);

        Ok(())
    }

    fn create_buffer(
        shm: &WlShm,
        qh: &QueueHandle<State>,
        capture: &CaptureImage,
    ) -> Result<(WlBuffer, tempfile::NamedTempFile, memmap2::MmapMut)> {
        let width = capture.width as i32;
        let height = capture.height as i32;
        let stride = width * 4;
        let size = (stride * height) as usize;

        let mut tmp_file = tempfile::NamedTempFile::new()
            .context("Failed to create temporary file for shm buffer")?;
        tmp_file
            .as_file_mut()
            .set_len(size as u64)
            .context("Failed to resize shm buffer file")?;

        let mut mmap = unsafe {
            memmap2::MmapMut::map_mut(&tmp_file).context("Failed to memory-map shm buffer")?
        };

        let src = &capture.data;
        let dst = &mut mmap[..];
        for (i, px) in src.chunks_exact(4).enumerate() {
            let offset = i * 4;
            dst[offset] = px[2];
            dst[offset + 1] = px[1];
            dst[offset + 2] = px[0];
            dst[offset + 3] = px[3];
        }

        let pool = shm.create_pool(
            unsafe { BorrowedFd::borrow_raw(tmp_file.as_file().as_raw_fd()) },
            size as i32,
            qh,
            (),
        );
        let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());
        pool.destroy();

        Ok((buffer, tmp_file, mmap))
    }

    fn output_logical_size(output: &OutputEntry) -> Option<(i32, i32)> {
        if let (Some(width), Some(height)) = (output.logical_width, output.logical_height) {
            return Some((width, height));
        }

        let mode_width = output.mode_width?;
        let mode_height = output.mode_height?;
        let scale = output.scale.max(1);
        Some((
            ((mode_width as f64) / (scale as f64)).round() as i32,
            ((mode_height as f64) / (scale as f64)).round() as i32,
        ))
    }

    fn output_geometry(output: &OutputEntry) -> Option<(i32, i32, i32, i32)> {
        let x = output.logical_x.or(output.pos_x)?;
        let y = output.logical_y.or(output.pos_y)?;
        let (width, height) = output_logical_size(output)?;
        Some((x, y, width, height))
    }

    fn geometry_close(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
        fn close(a: i32, b: i32) -> bool {
            (a - b).abs() <= 1
        }

        close(a.0, b.0) && close(a.1, b.1) && close(a.2, b.2) && close(a.3, b.3)
    }

    fn output_buffer_scale(output: &OutputEntry) -> i32 {
        output.scale.max(1)
    }

    fn match_outputs(
        outputs: &[OutputEntry],
        metas: &[GrimOutputMeta],
        selected_output: Option<&str>,
    ) -> Result<Vec<Option<usize>>> {
        let mut mapping = vec![None; outputs.len()];
        let mut used = vec![false; metas.len()];

        if let Some(selected) = selected_output {
            let meta_index = metas
                .iter()
                .position(|meta| meta.name == selected)
                .context(format!("Output '{}' not found", selected))?;

            if let Some((idx, _)) = outputs
                .iter()
                .enumerate()
                .find(|(_, o)| o.name.as_deref() == Some(selected))
            {
                mapping[idx] = Some(meta_index);
                used[meta_index] = true;
                return Ok(mapping);
            }

            let target_geom = metas[meta_index].geom;
            if let Some((idx, _)) = outputs.iter().enumerate().find(|(_, o)| {
                output_geometry(o)
                    .map(|geom| geometry_close(geom, target_geom))
                    .unwrap_or(false)
            }) {
                mapping[idx] = Some(meta_index);
                used[meta_index] = true;
                return Ok(mapping);
            }

            if !outputs.is_empty() {
                mapping[0] = Some(meta_index);
                used[meta_index] = true;
            }

            return Ok(mapping);
        }

        for (idx, output) in outputs.iter().enumerate() {
            let Some(name) = output.name.as_deref() else {
                continue;
            };
            if let Some((meta_idx, _)) = metas
                .iter()
                .enumerate()
                .find(|(m_idx, meta)| !used[*m_idx] && meta.name == name)
            {
                mapping[idx] = Some(meta_idx);
                used[meta_idx] = true;
            }
        }

        for (idx, output) in outputs.iter().enumerate() {
            if mapping[idx].is_some() {
                continue;
            }
            let Some(geom) = output_geometry(output) else {
                continue;
            };
            if let Some((meta_idx, _)) = metas
                .iter()
                .enumerate()
                .find(|(m_idx, meta)| !used[*m_idx] && geometry_close(meta.geom, geom))
            {
                mapping[idx] = Some(meta_idx);
                used[meta_idx] = true;
            }
        }

        let mut unused = metas
            .iter()
            .enumerate()
            .filter(|(idx, _)| !used[*idx])
            .map(|(idx, _)| idx);

        for slot in mapping.iter_mut().take(outputs.len()) {
            if slot.is_none()
                && let Some(meta_idx) = unused.next()
            {
                *slot = Some(meta_idx);
            }
        }

        Ok(mapping)
    }

    fn is_missing_screencopy_msg(msg: &str) -> bool {
        let msg = msg.to_ascii_lowercase();
        msg.contains("screencopy") || msg.contains("wlr-screencopy")
    }
}

#[cfg(all(target_os = "linux", feature = "freeze"))]
pub use imp::FreezeGuard;
#[cfg(all(target_os = "linux", feature = "freeze"))]
pub use imp::start_freeze;

#[cfg(not(all(target_os = "linux", feature = "freeze")))]
mod imp_stub {
    use super::*;

    pub struct FreezeGuard;

    impl FreezeGuard {
        pub fn stop(self) -> Result<()> {
            Ok(())
        }
    }

    pub fn start_freeze(_selected_output: Option<&str>, _debug: bool) -> Result<FreezeGuard> {
        Ok(FreezeGuard)
    }
}

#[cfg(not(all(target_os = "linux", feature = "freeze")))]
pub use imp_stub::FreezeGuard;
#[cfg(not(all(target_os = "linux", feature = "freeze")))]
pub use imp_stub::start_freeze;
