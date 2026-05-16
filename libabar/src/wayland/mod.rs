use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
#[cfg(any(feature = "clock", feature = "keyboard"))]
use std::sync::Arc;
#[cfg(feature = "clock")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "keyboard")]
use std::sync::RwLock;
#[cfg(feature = "keyboard")]
use wayland_client::protocol::wl_keyboard::{self, KeymapFormat};

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
        #[cfg(feature = "keyboard")]
        kb: KeyboardWlState::default(),
        icon_cache: IconCache::new(),
        font: None,
        updates_tx: updates_tx.clone(),
        updates_rx,
        wakeup_rx,
        #[cfg(feature = "clock")]
        clock_tz_index: Arc::new(AtomicUsize::new(0)),
        #[cfg(feature = "clock")]
        clock_timezones: Vec::new(),
        #[cfg(feature = "clock")]
        clock_formats: Vec::new(),
    };

    // Spawn clock background task.
    #[cfg(feature = "clock")]
    if let Some(clock_cfg) = modules.clock {
        state.clock_timezones = clock_cfg.timezones.clone();
        state.clock_formats = clock_cfg.formats.clone();
        let tx = updates_tx.clone();
        let wakeup = wakeup_tx.try_clone().map_err(|source| AbarError::Io {
            path: "/dev/null".into(),
            source,
        })?;
        let tz_index = state.clock_tz_index.clone();
        spawn::ensure_runtime()?.spawn(clock_task(tx, wakeup, clock_cfg, tz_index));
    }

    // Load keyboard fallback labels; real layout names arrive via wl_keyboard.keymap.
    #[cfg(feature = "keyboard")]
    if let Some(kb_cfg) = modules.keyboard {
        state.kb.config_layouts = kb_cfg.layouts;
    }

    // Spawn a Hyprland event socket listener when both features are active.
    // Delivers real-time layout names without requiring keyboard focus on the bar.
    #[cfg(all(feature = "keyboard", feature = "hyprland"))]
    {
        let has_keyboard = state
            .spec
            .layout
            .left
            .iter()
            .chain(state.spec.layout.center.iter())
            .chain(state.spec.layout.right.iter())
            .flat_map(|island| island.segments.iter())
            .any(|seg| seg.module_id == "keyboard");
        if has_keyboard {
            let tx = updates_tx.clone();
            let wakeup = wakeup_tx.try_clone().map_err(|source| AbarError::Io {
                path: "/dev/null".into(),
                source,
            })?;
            let xkb_layouts = Arc::clone(&state.kb.xkb_layouts);
            let config_layouts = state.kb.config_layouts.clone();
            spawn::ensure_runtime()?.spawn(hyprland_keyboard_task(
                tx,
                wakeup,
                xkb_layouts,
                config_layouts,
            ));
        }
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
    tz_index: Arc<AtomicUsize>,
) {
    use std::io::Write;
    use tokio::time::{Duration, sleep};

    loop {
        let ms = crate::modules::clock::ms_until_next_tick();
        sleep(Duration::from_millis(ms)).await;

        let idx = tz_index.load(Ordering::Relaxed);
        let tz = config.timezones.get(idx).copied();
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
// Hyprland keyboard background task
// ---------------------------------------------------------------------------

#[cfg(all(feature = "keyboard", feature = "hyprland"))]
async fn hyprland_keyboard_task(
    tx: mpsc::SyncSender<ModuleUpdate>,
    wakeup: UnixStream,
    xkb_layouts: Arc<RwLock<Vec<String>>>,
    config_layouts: Vec<String>,
) {
    use std::io::Write;
    use std::sync::Mutex;
    use hyprland::event_listener::{AsyncEventListener, LayoutEvent};

    let tx = Arc::new(tx);
    let wakeup = Arc::new(Mutex::new(wakeup));
    let config_layouts = Arc::new(config_layouts);

    let mut listener = AsyncEventListener::new();

    let tx_c = Arc::clone(&tx);
    let wakeup_c = Arc::clone(&wakeup);
    let xkb_c = Arc::clone(&xkb_layouts);
    let cfg_c = Arc::clone(&config_layouts);
    listener.add_layout_changed_handler(move |data: LayoutEvent| {
        let tx = Arc::clone(&tx_c);
        let wakeup = Arc::clone(&wakeup_c);
        let xkb = Arc::clone(&xkb_c);
        let cfg = Arc::clone(&cfg_c);
        Box::pin(async move {
            // Map Hyprland's layout name to a group index via the parsed xkb names,
            // then pick the user's config label for that group.
            let label = {
                let xkb = xkb.read().unwrap();
                let idx = xkb.iter().position(|n| n == &data.layout_name);
                match idx {
                    Some(i) => crate::modules::keyboard::current_label(&xkb, &cfg, i as u32),
                    None => {
                        tracing::warn!(
                            hyprland_name = %data.layout_name,
                            xkb_layouts = ?*xkb,
                            "keyboard layout not found in XKB names; using raw Hyprland name"
                        );
                        data.layout_name.clone()
                    }
                }
            };
            let _ = tx.try_send(ModuleUpdate {
                module_id: "keyboard".into(),
                label,
            });
            let _ = wakeup.lock().unwrap().write_all(&[0u8]);
        })
    });

    if let Err(e) = listener.start_listener_async().await {
        tracing::warn!(error = %e, "hyprland keyboard listener stopped");
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[cfg(feature = "keyboard")]
#[derive(Default)]
struct KeyboardWlState {
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Layout names extracted from the compositor's XKB keymap.
    /// Shared with the Hyprland task so it can map layout names to group indices.
    xkb_layouts: Arc<RwLock<Vec<String>>>,
    /// Currently active XKB group index (updated on modifiers events, requires focus).
    active_group: u32,
    /// Display labels from config (`[keyboard].layouts`); index matches XKB group order.
    config_layouts: Vec<String>,
}

#[cfg(feature = "keyboard")]
impl KeyboardWlState {
    fn current_label(&self) -> String {
        let xkb = self.xkb_layouts.read().unwrap();
        crate::modules::keyboard::current_label(&xkb, &self.config_layouts, self.active_group)
    }
}

#[derive(Default)]
struct PointerState {
    pointer: Option<wl_pointer::WlPointer>,
    on_surface: bool,
    x: f64,
    y: f64,
    // Set when an Axis event fires; cleared on Frame. Prevents the paired
    // AxisDiscrete (which arrives after Axis) from double-counting the click.
    had_axis: bool,
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
    #[cfg(feature = "keyboard")]
    kb: KeyboardWlState,
    icon_cache: IconCache,
    font: Option<FontContext>,
    #[allow(dead_code)]
    updates_tx: mpsc::SyncSender<ModuleUpdate>,
    updates_rx: mpsc::Receiver<ModuleUpdate>,
    wakeup_rx: UnixStream,
    #[cfg(feature = "clock")]
    clock_tz_index: Arc<AtomicUsize>,
    #[cfg(feature = "clock")]
    clock_timezones: Vec<chrono_tz::Tz>,
    #[cfg(feature = "clock")]
    clock_formats: Vec<String>,
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

    #[cfg(feature = "keyboard")]
    fn bind_keyboard(&mut self, seat: &wl_seat::WlSeat, qh: &QueueHandle<Self>) {
        if self.kb.keyboard.is_some() {
            return;
        }
        let kb = seat.get_keyboard(qh, ());
        self.kb.keyboard = Some(kb);
        debug!("keyboard bound");
    }

    fn dispatch_pointer_action(&mut self, action: PointerAction, _qh: &QueueHandle<Self>) {
        if !self.pointer.on_surface || self.computed.is_none() {
            return;
        }
        let x = self.pointer.x;
        let y = self.pointer.y;

        #[cfg(feature = "clock")]
        if matches!(action, PointerAction::ScrollUp | PointerAction::ScrollDown)
            && self.clock_timezones.len() > 1
        {
            let clock_hit = {
                let computed = self.computed.as_ref().unwrap();
                crate::hit_test::hit_test(computed, x, y)
                    .is_some_and(|s| s.module_id == "clock")
            };
            if clock_hit {
                let n = self.clock_timezones.len();
                let cur = self.clock_tz_index.load(Ordering::Relaxed);
                let next = if action == PointerAction::ScrollUp {
                    cur.checked_sub(1).unwrap_or(n - 1)
                } else {
                    (cur + 1) % n
                };
                self.clock_tz_index.store(next, Ordering::Relaxed);
                let label = {
                    let tz = self.clock_timezones[next];
                    let fmt = self.clock_formats.first().map_or("%H:%M", |s| s.as_str());
                    crate::modules::clock::current_label(fmt, Some(tz))
                };
                if let Err(e) = self.apply_update(
                    ModuleUpdate { module_id: "clock".to_string(), label },
                    _qh,
                ) {
                    warn!(error = %e, "clock tz update repaint failed");
                }
                return;
            }
        }

        let computed = self.computed.as_ref().unwrap();
        input::dispatch_pointer_action(computed, x, y, action);
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
        let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = event
        else {
            return;
        };
        if caps.contains(wl_seat::Capability::Pointer) {
            state.bind_pointer(seat, qh);
        }
        #[cfg(feature = "keyboard")]
        if caps.contains(wl_seat::Capability::Keyboard) {
            state.bind_keyboard(seat, qh);
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
        qh: &QueueHandle<Self>,
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
                    state.dispatch_pointer_action(action, qh);
                }
            }
            wl_pointer::Event::Axis { axis, value, .. } => {
                if axis != WEnum::Value(Axis::VerticalScroll) || value == 0.0 {
                    return;
                }
                state.pointer.had_axis = true;
                let action = if value < 0.0 {
                    PointerAction::ScrollUp
                } else {
                    PointerAction::ScrollDown
                };
                state.dispatch_pointer_action(action, qh);
            }
            wl_pointer::Event::AxisDiscrete { axis, discrete, .. } => {
                // Axis already handled this frame (Axis arrives before AxisDiscrete).
                if state.pointer.had_axis {
                    return;
                }
                if axis != WEnum::Value(Axis::VerticalScroll) || discrete == 0 {
                    return;
                }
                let action = if discrete < 0 {
                    PointerAction::ScrollUp
                } else {
                    PointerAction::ScrollDown
                };
                state.dispatch_pointer_action(action, qh);
            }
            wl_pointer::Event::Frame => {
                state.pointer.had_axis = false;
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

#[cfg(feature = "keyboard")]
impl Dispatch<wl_keyboard::WlKeyboard, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                if format != WEnum::Value(KeymapFormat::XkbV1) {
                    return;
                }
                // Use libxkbcommon directly: new_from_fd mmaps the FD so the
                // file-position issue (compositor sends FD at EOF) is irrelevant.
                use xkbcommon::xkb;
                let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
                // SAFETY: fd is a valid owned memfd from the compositor, live for this call.
                let Ok(Some(km)) = (unsafe {
                    xkb::Keymap::new_from_fd(
                        &ctx,
                        fd,
                        size as usize,
                        xkb::KEYMAP_FORMAT_TEXT_V1,
                        xkb::KEYMAP_COMPILE_NO_FLAGS,
                    )
                }) else {
                    return;
                };
                let layouts: Vec<String> =
                    (0..km.num_layouts()).map(|i| km.layout_get_name(i).to_string()).collect();
                debug!(xkb_layouts = ?layouts, "parsed XKB keyboard layout names");
                *state.kb.xkb_layouts.write().unwrap() = layouts;
                let label = state.kb.current_label();
                if let Err(e) = state.apply_update(
                    ModuleUpdate { module_id: "keyboard".into(), label },
                    qh,
                ) {
                    warn!(error = %e, "keyboard keymap repaint failed");
                }
            }
            wl_keyboard::Event::Modifiers { group, .. } => {
                // Only received when the bar surface has keyboard focus.
                state.kb.active_group = group;
                let label = state.kb.current_label();
                if let Err(e) = state.apply_update(
                    ModuleUpdate { module_id: "keyboard".into(), label },
                    qh,
                ) {
                    warn!(error = %e, "keyboard modifiers repaint failed");
                }
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
