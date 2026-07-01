use anyhow::{Context, Result};
use std::{
    os::fd::{AsRawFd, BorrowedFd},
    sync::mpsc,
    thread,
    time::Duration,
};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_output::WlOutput,
        wl_region::WlRegion,
        wl_registry::WlRegistry,
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

const PADDING: i32 = 10;
const BORDER_THICK: i32 = 6;

#[derive(Debug)]
struct OutputKey(usize);

#[derive(Debug)]
struct SurfaceKey(usize);

struct OutputEntry {
    output: WlOutput,
    name: Option<String>,
    pos_x: Option<i32>,
    pos_y: Option<i32>,
    scale: i32,
    mode_width: Option<i32>,
    mode_height: Option<i32>,
}

struct State {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    outputs: Vec<OutputEntry>,
    surface: Option<SurfaceEntry>,
    configured: bool,
    configured_w: Option<u32>,
    configured_h: Option<u32>,
}

struct SurfaceEntry {
    surface: WlSurface,
    layer_surface: ZwlrLayerSurfaceV1,
    buffer: WlBuffer,
    _tmp: tempfile::NamedTempFile,
    mmap: memmap2::MmapMut,
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
                        pos_x: None,
                        pos_y: None,
                        scale: 1,
                        mode_width: None,
                        mode_height: None,
                    });
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
        if let Some(entry) = state.outputs.get_mut(data.0) {
            match event {
                wayland_client::protocol::wl_output::Event::Geometry { x, y, .. } => {
                    entry.pos_x = Some(x);
                    entry.pos_y = Some(y);
                }
                wayland_client::protocol::wl_output::Event::Scale { factor } => {
                    entry.scale = factor.max(1);
                }
                wayland_client::protocol::wl_output::Event::Name { name } => {
                    entry.name = Some(name);
                }
                wayland_client::protocol::wl_output::Event::Mode { flags, width, height, .. } => {
                    let is_current = match flags {
                        wayland_client::WEnum::Value(f) => f.contains(wayland_client::protocol::wl_output::Mode::Current),
                        wayland_client::WEnum::Unknown(_) => false,
                    };
                    if is_current {
                        entry.mode_width = Some(width);
                        entry.mode_height = Some(height);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, SurfaceKey> for State {
    fn event(
        state: &mut Self,
        surface: &ZwlrLayerSurfaceV1,
        event: wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event,
        _: &SurfaceKey,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                surface.ack_configure(serial);
                state.configured = true;
                state.configured_w = Some(width);
                state.configured_h = Some(height);
            }
            wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Event::Closed => {
                state.configured = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<WlCompositor, ()> for State {
    fn event(_: &mut Self, _: &WlCompositor, _: wayland_client::protocol::wl_compositor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlShm, ()> for State {
    fn event(_: &mut Self, _: &WlShm, _: wayland_client::protocol::wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlShmPool, ()> for State {
    fn event(_: &mut Self, _: &WlShmPool, _: wayland_client::protocol::wl_shm_pool::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlSurface, ()> for State {
    fn event(_: &mut Self, _: &WlSurface, _: wayland_client::protocol::wl_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlBuffer, ()> for State {
    fn event(_: &mut Self, _: &WlBuffer, _: wayland_client::protocol::wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<WlRegion, ()> for State {
    fn event(_: &mut Self, _: &WlRegion, _: wayland_client::protocol::wl_region::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ZwlrLayerShellV1, ()> for State {
    fn event(_: &mut Self, _: &ZwlrLayerShellV1, _: wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

fn draw_selection_border(
    mmap: &mut [u8],
    width: i32,  // buffer width in physical pixels
    height: i32, // buffer height in physical pixels
    rx: i32,     // selection x relative to output, in logical pixels
    ry: i32,     // selection y relative to output, in logical pixels
    rw: i32,     // selection width in logical pixels
    rh: i32,     // selection height in logical pixels
    true_scale: f64, // real fractional scale factor (e.g. 1.25)
    alpha: u8,
) {
    mmap.fill(0);
    let color = [0u8, 0u8, alpha, alpha];
    
    let thick = ((BORDER_THICK as f64) * true_scale).round() as i32;
    let padding = ((PADDING as f64) * true_scale).round() as i32;
    
    // Convert logical coords to physical
    let border_l = ((rx as f64 * true_scale) as i32 - padding).max(0);
    let border_r = (((rx + rw) as f64 * true_scale) as i32 + padding).min(width);
    let border_t = ((ry as f64 * true_scale) as i32 - padding).max(0);
    let border_b = (((ry + rh) as f64 * true_scale) as i32 + padding).min(height);
    
    if border_l >= border_r || border_t >= border_b {
        return;
    }

    // Draw top line
    for y in border_t..std::cmp::min(border_t + thick, border_b) {
        for x in border_l..border_r {
            let offset = ((y * width + x) * 4) as usize;
            if offset + 3 < mmap.len() {
                mmap[offset..offset+4].copy_from_slice(&color);
            }
        }
    }

    // Draw bottom line
    for y in std::cmp::max(border_t, border_b - thick)..border_b {
        for x in border_l..border_r {
            let offset = ((y * width + x) * 4) as usize;
            if offset + 3 < mmap.len() {
                mmap[offset..offset+4].copy_from_slice(&color);
            }
        }
    }

    // Draw left line
    for y in border_t..border_b {
        for x in border_l..std::cmp::min(border_l + thick, border_r) {
            let offset = ((y * width + x) * 4) as usize;
            if offset + 3 < mmap.len() {
                mmap[offset..offset+4].copy_from_slice(&color);
            }
        }
    }

    // Draw right line
    for y in border_t..border_b {
        for x in std::cmp::max(border_l, border_r - thick)..border_r {
            let offset = ((y * width + x) * 4) as usize;
            if offset + 3 < mmap.len() {
                mmap[offset..offset+4].copy_from_slice(&color);
            }
        }
    }
}

pub fn run_overlay(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    _scale: f64,
    monitor: &str,
    output_x: i32,
    output_y: i32,
) -> Result<()> {
    let conn = Connection::connect_to_env().context("Failed to connect to Wayland")?;
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let registry = conn.display().get_registry(&qh, ());

    let mut state = State {
        compositor: None,
        shm: None,
        layer_shell: None,
        outputs: Vec::new(),
        surface: None,
        configured: false,
        configured_w: None,
        configured_h: None,
    };

    event_queue.roundtrip(&mut state).context("Failed to initialize Wayland globals")?;
    
    // Perform roundtrip again to ensure output metadata is received
    event_queue.roundtrip(&mut state).context("Failed to query output metadata")?;

    let compositor = state.compositor.clone().context("wl_compositor not available")?;
    let shm = state.shm.clone().context("wl_shm not available")?;
    let layer_shell = state.layer_shell.clone().context("zwlr_layer_shell_v1 not available")?;

    // Find the matching output by name or fallback to first
    let (output_wl, scale_int, mode_width, mode_height) = {
        let output_entry = state.outputs.iter().find(|o| o.name.as_deref() == Some(monitor))
            .or_else(|| state.outputs.first())
            .context("No outputs found")?;
        (
            output_entry.output.clone(),
            output_entry.scale.max(1),
            output_entry.mode_width,
            output_entry.mode_height,
        )
    };

    // Calculate margins relative to output top-left using passed active monitor coordinates
    let rx = x - output_x;
    let ry = y - output_y;

    // Create layer surface
    let surface = compositor.create_surface(&qh, ());
    let layer_surface = layer_shell.get_layer_surface(
        &surface,
        Some(&output_wl),
        Layer::Overlay,
        "shot-overlay".to_string(),
        &qh,
        SurfaceKey(0),
    );

    // Full screen overlay
    layer_surface.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
    layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer_surface.set_exclusive_zone(-1);

    // Set mouse pass-through: empty input region
    let input_region = compositor.create_region(&qh, ());
    surface.set_input_region(Some(&input_region));

    surface.commit();
    event_queue.roundtrip(&mut state).context("Failed to configure overlay surface")?;

    // The compositor configures the layer surface at logical pixel dimensions (e.g. 2048x1280).
    // The buffer MUST match these logical dimensions exactly — if we provide a larger buffer
    // (e.g. 2560x1600 physical) with buffer_scale=1, the compositor sees a 2560x1600 logical
    // surface on a 2048x1280 configured surface and ignores the content.
    //
    // Correct approach: use logical dimensions as buffer size, draw at logical coords.
    // The compositor handles upscaling to physical pixels internally (fractional scale).
    let w_buf = state.configured_w.filter(|&w| w > 0).map(|w| w as i32)
        .or_else(|| mode_width.map(|mw| if scale_int > 0 { mw / scale_int } else { mw }))
        .unwrap_or(1920);
    let h_buf = state.configured_h.filter(|&h| h > 0).map(|h| h as i32)
        .or_else(|| mode_height.map(|mh| if scale_int > 0 { mh / scale_int } else { mh }))
        .unwrap_or(1080);

    let _ = std::fs::write("/tmp/overlay_debug.log", format!(
        "buf={w_buf}x{h_buf} mode={:?}x{:?} scale_int={scale_int} rx={rx} ry={ry} w={w} h={h}\n",
        mode_width, mode_height
    ));

    let stride = w_buf * 4;
    let size = (stride * h_buf) as usize;

    // Create shm buffer at logical dimensions
    let mut tmp_file = tempfile::NamedTempFile::new().context("Failed to create temporary file for shm")?;
    tmp_file.as_file_mut().set_len(size as u64).context("Failed to resize shm file")?;
    let mmap = unsafe { memmap2::MmapMut::map_mut(&tmp_file).context("Failed to map shm")? };

    let pool = shm.create_pool(
        unsafe { BorrowedFd::borrow_raw(tmp_file.as_file().as_raw_fd()) },
        size as i32,
        &qh,
        (),
    );
    let buffer = pool.create_buffer(0, w_buf, h_buf, stride, wl_shm::Format::Argb8888, &qh, ());
    pool.destroy();

    // buffer_scale stays at 1 (default) — buffer is in logical pixels
    // compositor upscales to physical internally

    state.surface = Some(SurfaceEntry {
        surface,
        layer_surface,
        buffer,
        _tmp: tmp_file,
        mmap,
    });

    let surface_entry = state.surface.as_mut().unwrap();
    surface_entry.surface.attach(Some(&surface_entry.buffer), 0, 0);
    surface_entry.surface.damage_buffer(0, 0, w_buf, h_buf);
    surface_entry.surface.commit();
    conn.flush().ok();

    // Event loop with breathing/flashing border
    // Draw at logical pixel coords — no scale conversion needed
    let start_time = std::time::Instant::now();
    loop {
        let elapsed = start_time.elapsed().as_secs_f32();
        // Breathing alpha: 1.5 Hz oscillation between ~55 and ~255
        let alpha = (((elapsed * 2.0 * std::f32::consts::PI * 1.5).sin() + 1.0) * 0.5 * 200.0 + 55.0) as u8;

        let surface_entry = state.surface.as_mut().unwrap();
        // Draw at logical coords: rx, ry, w, h are already in logical pixels;
        // true_scale = 1.0 since buffer is logical
        draw_selection_border(
            &mut surface_entry.mmap,
            w_buf,
            h_buf,
            rx,
            ry,
            w,
            h,
            1.0,   // draw at logical 1:1
            alpha,
        );

        surface_entry.surface.damage_buffer(0, 0, w_buf, h_buf);
        surface_entry.surface.commit();
        conn.flush().ok();

        event_queue.roundtrip(&mut state).ok();

        // ~20 FPS — smooth animation, near-zero CPU usage
        thread::sleep(Duration::from_millis(50));
    }
}
