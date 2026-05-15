use std::fs::File;
use std::io::Write;
use std::os::unix::io::AsFd;

use tracing::{debug, warn};
use wayland_client::protocol::wl_pointer::{Axis, ButtonState};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

use crate::error::AbarError;
use crate::input::{self, PointerAction};
use crate::layout::ComputedBar;
use crate::model::BarSpec;
use crate::render::paint_bar;
use crate::spawn;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

/// Blocks until the layer surface is closed or dispatch fails.
pub fn run_bar(spec: BarSpec) -> Result<(), AbarError> {
    spawn::ensure_runtime()?;
    let conn = Connection::connect_to_env()?;
    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    display.get_registry(&qh, ());

    let mut state = AppState {
        running: true,
        spec,
        compositor: None,
        shm: None,
        layer_shell: None,
        seat: None,
        surface: None,
        layer_surface: None,
        pending_configure: None,
        buffer: None,
        pool: None,
        pool_file: None,
        bar_height: 1,
        computed: None,
        pointer: PointerState::default(),
    };

    while state.running {
        event_queue
            .blocking_dispatch(&mut state)
            .map_err(|e| AbarError::WaylandProtocol(format!("dispatch failed: {e}")))?;
    }

    Ok(())
}

#[derive(Default)]
struct PointerState {
    pointer: Option<wl_pointer::WlPointer>,
    on_surface: bool,
    x: f64,
    y: f64,
}

struct AppState {
    running: bool,
    spec: BarSpec,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    seat: Option<wl_seat::WlSeat>,
    surface: Option<wl_surface::WlSurface>,
    layer_surface: Option<ZwlrLayerSurfaceV1>,
    pending_configure: Option<(u32, u32, u32)>,
    buffer: Option<wl_buffer::WlBuffer>,
    pool: Option<wl_shm_pool::WlShmPool>,
    pool_file: Option<File>,
    bar_height: u32,
    computed: Option<ComputedBar>,
    pointer: PointerState,
}

impl AppState {
    fn try_init_layer_shell(&mut self, qh: &QueueHandle<Self>) {
        if self.layer_surface.is_some() {
            return;
        }
        let Some(compositor) = self.compositor.as_ref() else {
            return;
        };
        let Some(layer_shell) = self.layer_shell.as_ref() else {
            return;
        };

        let surface = compositor.create_surface(qh, ());
        let layer_surface =
            layer_shell.get_layer_surface(&surface, None, Layer::Top, "abar".into(), qh, ());

        layer_surface.set_anchor(Anchor::Top | Anchor::Left | Anchor::Right);
        layer_surface.set_exclusive_zone(self.bar_height as i32);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.set_size(0, self.bar_height);

        surface.commit();

        self.surface = Some(surface);
        self.layer_surface = Some(layer_surface);
        debug!("layer surface created (initial commit without buffer)");
    }

    fn bind_pointer(&mut self, seat: &wl_seat::WlSeat, qh: &QueueHandle<Self>) {
        if self.pointer.pointer.is_some() {
            return;
        }
        let pointer = seat.get_pointer(qh, ());
        self.pointer.pointer = Some(pointer);
        debug!("pointer bound");
    }

    fn dispatch_pointer_action(&self, action: PointerAction) {
        let Some(computed) = self.computed.as_ref() else {
            return;
        };
        if !self.pointer.on_surface {
            return;
        }
        input::dispatch_pointer_action(computed, self.pointer.x, self.pointer.y, action);
    }

    fn on_configure(
        &mut self,
        layer_surface: &ZwlrLayerSurfaceV1,
        serial: u32,
        width: u32,
        height: u32,
        qh: &QueueHandle<Self>,
    ) -> Result<(), AbarError> {
        let width = width.max(1);
        let height = height.max(1);

        let Some(shm) = self.shm.clone() else {
            self.pending_configure = Some((width, height, serial));
            return Ok(());
        };

        layer_surface.ack_configure(serial);
        self.resize_and_paint(&shm, qh, width, height)
    }

    fn try_flush_pending_configure(&mut self, qh: &QueueHandle<Self>) -> Result<(), AbarError> {
        if self.pending_configure.is_none() {
            return Ok(());
        }
        let Some(shm) = self.shm.clone() else {
            return Ok(());
        };
        let Some(ls) = self.layer_surface.as_ref() else {
            return Ok(());
        };
        let Some((w, h, serial)) = self.pending_configure.take() else {
            return Ok(());
        };
        ls.ack_configure(serial);
        self.resize_and_paint(&shm, qh, w, h)
    }

    fn resize_and_paint(
        &mut self,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<Self>,
        width: u32,
        height: u32,
    ) -> Result<(), AbarError> {
        let painted = paint_bar(&self.spec, width)?;
        self.bar_height = painted.frame.height;
        self.computed = Some(painted.computed);

        if let Some(ls) = self.layer_surface.as_ref() {
            ls.set_exclusive_zone(painted.frame.height as i32);
            ls.set_size(0, painted.frame.height);
        }

        let stride = painted.frame.stride;
        let buf_h = painted.frame.height;
        let size = (stride as u64)
            .checked_mul(buf_h as u64)
            .ok_or_else(|| AbarError::WaylandProtocol("buffer size overflow".into()))?;

        self.buffer.take();
        self.pool.take();
        self.pool_file.take();

        let mut file = tempfile::tempfile_in("/dev/shm").map_err(|source| AbarError::Io {
            path: std::path::PathBuf::from("/dev/shm"),
            source,
        })?;

        file.write_all(&painted.frame.data)
            .map_err(|source| AbarError::Io {
                path: std::path::PathBuf::from("/dev/shm"),
                source,
            })?;
        file.flush().map_err(|source| AbarError::Io {
            path: std::path::PathBuf::from("/dev/shm"),
            source,
        })?;

        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            width as i32,
            buf_h as i32,
            stride,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );

        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| AbarError::WaylandProtocol("missing wl_surface during paint".into()))?;
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width as i32, buf_h as i32);
        surface.commit();

        self.pool_file = Some(file);
        self.pool = Some(pool);
        self.buffer = Some(buffer);

        let _ = height;
        Ok(())
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for AppState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(
                        name,
                        5.min(version),
                        qh,
                        (),
                    );
                    state.compositor = Some(compositor);
                    state.try_init_layer_shell(qh);
                }
                "wl_shm" => {
                    let shm = registry.bind::<wl_shm::WlShm, _, _>(name, 1.min(version), qh, ());
                    state.shm = Some(shm);
                    if let Err(e) = state.try_flush_pending_configure(qh) {
                        warn!(error = %e, "failed to apply pending layer configure");
                        state.running = false;
                    }
                }
                "zwlr_layer_shell_v1" => {
                    let shell =
                        registry.bind::<ZwlrLayerShellV1, _, _>(name, 4.min(version), qh, ());
                    state.layer_shell = Some(shell);
                    state.try_init_layer_shell(qh);
                }
                "wl_seat" => {
                    let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, 7.min(version), qh, ());
                    state.seat = Some(seat);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for AppState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
            && caps.contains(wl_seat::Capability::Pointer)
        {
            state.bind_pointer(seat, qh);
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { surface, .. } => {
                state.pointer.on_surface =
                    state.surface.as_ref().is_some_and(|ours| &surface == ours);
            }
            wl_pointer::Event::Leave { surface, .. }
                if state.surface.as_ref().is_some_and(|ours| &surface == ours) =>
            {
                state.pointer.on_surface = false;
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer.x = surface_x;
                state.pointer.y = surface_y;
            }
            wl_pointer::Event::Button {
                button,
                state: btn_state,
                ..
            } => {
                if btn_state != WEnum::Value(ButtonState::Pressed) {
                    return;
                }
                let action = match button {
                    BTN_LEFT => Some(PointerAction::LeftClick),
                    BTN_RIGHT => Some(PointerAction::RightClick),
                    BTN_MIDDLE => Some(PointerAction::MiddleClick),
                    _ => None,
                };
                if let Some(action) = action {
                    state.dispatch_pointer_action(action);
                }
            }
            wl_pointer::Event::AxisDiscrete { axis, discrete, .. } => {
                if axis != WEnum::Value(Axis::VerticalScroll) || discrete == 0 {
                    return;
                }
                let action = if discrete < 0 {
                    PointerAction::ScrollUp
                } else {
                    PointerAction::ScrollDown
                };
                state.dispatch_pointer_action(action);
            }
            wl_pointer::Event::Axis { axis, value, .. } => {
                if axis != WEnum::Value(Axis::VerticalScroll) || value == 0.0 {
                    return;
                }
                let action = if value < 0.0 {
                    PointerAction::ScrollUp
                } else {
                    PointerAction::ScrollDown
                };
                state.dispatch_pointer_action(action);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, ()> for AppState {
    fn event(
        state: &mut Self,
        layer_surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                if let Err(e) = state.on_configure(layer_surface, serial, width, height, qh) {
                    warn!(error = %e, "configure handling failed");
                    state.running = false;
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                debug!("layer surface closed");
                state.running = false;
            }
            _ => {}
        }
    }
}

wayland_client::delegate_noop!(AppState: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(AppState: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(AppState: ignore wl_shm::WlShm);
wayland_client::delegate_noop!(AppState: ignore wl_shm_pool::WlShmPool);
wayland_client::delegate_noop!(AppState: ignore wl_buffer::WlBuffer);
wayland_client::delegate_noop!(AppState: ignore ZwlrLayerShellV1);

#[cfg(test)]
mod tests;
