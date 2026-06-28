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
}

struct State {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    outputs: Vec<OutputEntry>,
    surface: Option<SurfaceEntry>,
    configured: bool,
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
                ..
            } => {
                surface.ack_configure(serial);
                state.configured = true;
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

fn draw_border(mmap: &mut [u8], width: i32, height: i32, thick: i32, alpha: u8) {
    mmap.fill(0);
    // Wayland format is Argb8888 (little-endian BGRA)
    // Blue=0, Green=0, Red=alpha, Alpha=alpha
    let color = [0u8, 0u8, alpha, alpha];
    
    for y in 0..height {
        for x in 0..width {
            let is_border = x < thick || x >= width - thick || y < thick || y >= height - thick;
            if is_border {
                let offset = ((y * width + x) * 4) as usize;
                if offset + 3 < mmap.len() {
                    mmap[offset] = color[0];
                    mmap[offset + 1] = color[1];
                    mmap[offset + 2] = color[2];
                    mmap[offset + 3] = color[3];
                }
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
    };

    event_queue.roundtrip(&mut state).context("Failed to initialize Wayland globals")?;
    
    // Perform roundtrip again to ensure output metadata is received
    event_queue.roundtrip(&mut state).context("Failed to query output metadata")?;

    let compositor = state.compositor.clone().context("wl_compositor not available")?;
    let shm = state.shm.clone().context("wl_shm not available")?;
    let layer_shell = state.layer_shell.clone().context("zwlr_layer_shell_v1 not available")?;

    // Find the matching output by name or fallback to first
    let output_entry = state.outputs.iter().find(|o| o.name.as_deref() == Some(monitor))
        .or_else(|| state.outputs.first())
        .context("No outputs found")?;

    let scale_int = output_entry.scale.max(1);

    // Calculate margins relative to output top-left using passed active monitor coordinates
    let rx = x - output_x;
    let ry = y - output_y;
    let margin_left = (rx - PADDING).max(0);
    let margin_top = (ry - PADDING).max(0);

    // Overlay surface size (logical)
    let w_logical = w + PADDING * 2;
    let h_logical = h + PADDING * 2;

    // Buffer dimensions (physical)
    let w_phys = w_logical * scale_int;
    let h_phys = h_logical * scale_int;
    let stride = w_phys * 4;
    let size = (stride * h_phys) as usize;

    // Create shm buffer
    let mut tmp_file = tempfile::NamedTempFile::new().context("Failed to create temporary file for shm")?;
    tmp_file.as_file_mut().set_len(size as u64).context("Failed to resize shm file")?;
    let mmap = unsafe { memmap2::MmapMut::map_mut(&tmp_file).context("Failed to map shm")? };

    let pool = shm.create_pool(
        unsafe { BorrowedFd::borrow_raw(tmp_file.as_file().as_raw_fd()) },
        size as i32,
        &qh,
        (),
    );
    let buffer = pool.create_buffer(0, w_phys, h_phys, stride, wl_shm::Format::Argb8888, &qh, ());
    pool.destroy();

    // Create layer surface
    let surface = compositor.create_surface(&qh, ());
    let layer_surface = layer_shell.get_layer_surface(
        &surface,
        Some(&output_entry.output),
        Layer::Overlay,
        "shot-overlay".to_string(),
        &qh,
        SurfaceKey(0),
    );

    layer_surface.set_anchor(Anchor::Top | Anchor::Left);
    layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer_surface.set_exclusive_zone(-1);
    layer_surface.set_size(w_logical as u32, h_logical as u32);
    layer_surface.set_margin(margin_top, 0, 0, margin_left);

    if scale_int > 1 {
        surface.set_buffer_scale(scale_int);
    }

    // Set mouse pass-through: empty input region
    let input_region = compositor.create_region(&qh, ());
    surface.set_input_region(Some(&input_region));

    surface.commit();
    event_queue.roundtrip(&mut state).context("Failed to configure overlay surface")?;

    state.surface = Some(SurfaceEntry {
        surface,
        layer_surface,
        buffer,
        _tmp: tmp_file,
        mmap,
    });

    let surface_entry = state.surface.as_mut().unwrap();
    surface_entry.surface.attach(Some(&surface_entry.buffer), 0, 0);
    surface_entry.surface.commit();
    conn.flush().ok();

    // Event loop with breathing/flashing border
    let start_time = std::time::Instant::now();
    loop {
        let elapsed = start_time.elapsed().as_secs_f32();
        // Breathing alpha value: frequency 1.5Hz
        let alpha = (((elapsed * 2.0 * std::f32::consts::PI * 1.5).sin() + 1.0) * 0.5 * 200.0 + 55.0) as u8;
        
        let surface_entry = state.surface.as_mut().unwrap();
        draw_border(&mut surface_entry.mmap, w_phys, h_phys, BORDER_THICK * scale_int, alpha);
        
        surface_entry.surface.damage(0, 0, w_logical, h_logical);
        surface_entry.surface.commit();
        conn.flush().ok();

        event_queue.roundtrip(&mut state).ok();
        
        // Target roughly 20 FPS for smooth flashing without wasting CPU
        thread::sleep(Duration::from_millis(50));
    }
}
