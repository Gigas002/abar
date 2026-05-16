use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;

use rustix::event::{PollFd, PollFlags, poll};
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
use crate::icon::IconCache;
use crate::input::{self, PointerAction};
use crate::layout::{ComputedBar, compute_bar};
use crate::model::BarSpec;
use crate::modules::{ModuleConfigs, ModuleUpdate};
use crate::render::{FontContext, paint_computed};
use crate::spawn;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

/// Blocks until the layer surface is closed or an unrecoverable error occurs.
pub fn run_bar(spec: BarSpec, modules: ModuleConfigs) -> Result<(), AbarError> {
    spawn::ensure_runtime()?;

    let (updates_tx, updates_rx) = mpsc::sync_channel::<ModuleUpdate>(64);
    let (wakeup_rx, wakeup_tx) = UnixStream::pair().map_err(|source| AbarError::Io {
        path: "/dev/null".into(),
        source,
    })?;
    wakeup_rx
        .set_nonblocking(true)
        .map_err(|source| AbarError::Io {
            path: "/dev/null".into(),
            source,
        })?;

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
        bar_width: 1,
        bar_height: 1,
        computed: None,
        pointer: PointerState::default(),
        icon_cache: IconCache::new(),
        font: None,
        updates_tx: updates_tx.clone(),
        updates_rx,
        wakeup_rx,
    };

    // Spawn clock background task.
    #[cfg(feature = "clock")]
    if let Some(clock_cfg) = modules.clock {
        let tx = updates_tx.clone();
        let wakeup = wakeup_tx.try_clone().map_err(|source| AbarError::Io {
            path: "/dev/null".into(),
            source,
        })?;
        spawn::ensure_runtime()?.spawn(clock_task(tx, wakeup, clock_cfg));
    }

    // Suppress unused-variable warning when no module features are active.
    let _ = modules;
    // All tasks that need the wakeup sender now hold their own clones; drop ours.
    drop(wakeup_tx);

    loop {
        event_queue
            .flush()
            .map_err(|e| AbarError::WaylandProtocol(format!("flush failed: {e}")))?;

        event_queue
            .dispatch_pending(&mut state)
            .map_err(|e| AbarError::WaylandProtocol(format!("dispatch failed: {e}")))?;

        // Apply any module label updates that arrived since the last loop iteration.
        while let Ok(update) = state.updates_rx.try_recv() {
            if let Err(e) = state.apply_update(update, &qh) {
                warn!(error = %e, "module update repaint failed");
            }
        }

        if !state.running {
            break;
        }

        // Flush any surface commits queued by apply_update before blocking in poll;
        // without this the compositor would not see the updated buffer until the next
        // Wayland event (e.g. pointer motion) triggered another flush.
        event_queue
            .flush()
            .map_err(|e| AbarError::WaylandProtocol(format!("flush failed: {e}")))?;

        // Acquire the Wayland read lock; if events are already pending just loop.
        let Some(read_guard) = event_queue.prepare_read() else {
            continue;
        };

        let wayland_fd = read_guard.connection_fd();
        // Copy the raw fd so we can borrow it independently of `state`.
        let wakeup_raw = state.wakeup_rx.as_raw_fd();

        // Poll the Wayland connection and the wakeup pipe; None = wait indefinitely.
        let mut pollfds = [
            PollFd::from_borrowed_fd(wayland_fd, PollFlags::IN),
            // SAFETY: wakeup_rx is alive and valid for the duration of this poll call.
            PollFd::from_borrowed_fd(unsafe { BorrowedFd::borrow_raw(wakeup_raw) }, PollFlags::IN),
        ];

        match poll(&mut pollfds, None) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => {
                drop(read_guard);
                continue;
            }
            Err(e) => {
                return Err(AbarError::WaylandProtocol(format!("poll: {e}")));
            }
        }

        // Drain the wakeup pipe so it doesn't stay readable forever.
        if pollfds[1].revents().contains(PollFlags::IN) {
            let mut buf = [0u8; 64];
            let _ = state.wakeup_rx.read(&mut buf);
        }

        if pollfds[0].revents().contains(PollFlags::IN) {
            if let Err(e) = read_guard.read() {
                return Err(AbarError::WaylandProtocol(format!(
                    "read events failed: {e}"
                )));
            }
        } else {
            drop(read_guard);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Clock background task
// ---------------------------------------------------------------------------

#[cfg(feature = "clock")]
async fn clock_task(
    tx: mpsc::SyncSender<ModuleUpdate>,
    mut wakeup: UnixStream,
    config: crate::modules::clock::ClockConfig,
) {
    use std::io::Write;
    use tokio::time::{Duration, sleep};

    // Determine the active timezone (first in list, or local if empty).
    let tz = config.timezones.first().copied();

    loop {
        let ms = crate::modules::clock::ms_until_next_tick();
        sleep(Duration::from_millis(ms)).await;

        let fmt = config.formats.first().map_or("%H:%M", |s| s.as_str());
        let label = crate::modules::clock::current_label(fmt, tz);

        let _ = tx.try_send(ModuleUpdate {
            module_id: "clock".to_string(),
            label,
        });
        let _ = wakeup.write_all(&[0u8]);
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

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
    pool_file: Option<std::fs::File>,
    bar_width: u32,
    bar_height: u32,
    computed: Option<ComputedBar>,
    pointer: PointerState,
    icon_cache: IconCache,
    font: Option<FontContext>,
    #[allow(dead_code)]
    updates_tx: mpsc::SyncSender<ModuleUpdate>,
    updates_rx: mpsc::Receiver<ModuleUpdate>,
    wakeup_rx: UnixStream,
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

    /// Update a segment label and repaint. No-op if the bar hasn't been painted yet.
    fn apply_update(
        &mut self,
        update: ModuleUpdate,
        qh: &QueueHandle<Self>,
    ) -> Result<(), AbarError> {
        let found = self
            .spec
            .layout
            .left
            .iter_mut()
            .chain(self.spec.layout.center.iter_mut())
            .chain(self.spec.layout.right.iter_mut())
            .flat_map(|island| island.segments.iter_mut())
            .find(|seg| seg.module_id == update.module_id);

        let Some(seg) = found else {
            return Ok(());
        };
        seg.label = update.label;

        // Only repaint once the layer surface has been configured and painted at least once.
        if self.computed.is_none() {
            return Ok(());
        }
        let Some(shm) = self.shm.clone() else {
            return Ok(());
        };
        self.resize_and_paint(&shm, qh, self.bar_width, self.bar_height)
    }

    fn resize_and_paint(
        &mut self,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<Self>,
        width: u32,
        height: u32,
    ) -> Result<(), AbarError> {
        self.bar_width = width;

        // Initialise the font context once; avoids fontconfig rescanning on every repaint.
        if self.font.is_none() {
            self.font = Some(FontContext::new(
                &self.spec.style.font_name,
                self.spec.style.font_size,
            )?);
        }
        let font = self.font.as_ref().unwrap();

        let computed = compute_bar(&self.spec, width, &|text| font.measure(text));
        let frame = paint_computed(&self.spec, &computed, font, &mut self.icon_cache)?;
        self.bar_height = frame.height;
        self.computed = Some(computed);

        if let Some(ls) = self.layer_surface.as_ref() {
            ls.set_exclusive_zone(frame.height as i32);
            ls.set_size(0, frame.height);
        }

        let stride = frame.stride;
        let buf_h = frame.height;
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

        use std::io::Write;
        file.write_all(&frame.data)
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

// ---------------------------------------------------------------------------
// Wayland dispatch implementations
// ---------------------------------------------------------------------------

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
        {
            if caps.contains(wl_seat::Capability::Pointer) {
                state.bind_pointer(seat, qh);
            }
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
