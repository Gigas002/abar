use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
#[cfg(feature = "clock")]
use std::sync::Arc;
#[cfg(feature = "clock")]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use wayland_client::protocol::wl_keyboard;

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
        keyboard: None,
        submenu: None,
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
        #[cfg(feature = "tray")]
        tray_rx: None,
        #[cfg(feature = "tray")]
        tray_events: crate::model::SegmentEvents::default(),
        #[cfg(feature = "tray")]
        tray_feed_id: false,
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

    // Spawn exec handler for keyboard if configured; initial label is already set in BarSpec.
    #[cfg(feature = "keyboard")]
    if let Some(kb_cfg) = modules.keyboard
        && let Some(cmd) = kb_cfg.exec
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
            spawn::ensure_runtime()?.spawn(crate::exec::run_exec_handler::<
                crate::modules::keyboard::KeyboardData,
                _,
            >(
                "keyboard".to_string(),
                cmd,
                tx,
                wakeup,
                |data| ModuleUpdate::text("keyboard", data.label),
            ));
        }
    }

    // Spawn exec handler for workspaces if configured.
    #[cfg(feature = "workspaces")]
    if let Some(ws_cfg) = modules.workspaces
        && let Some(cmd) = ws_cfg.exec
    {
        let has_workspaces = state
            .spec
            .layout
            .left
            .iter()
            .chain(state.spec.layout.center.iter())
            .chain(state.spec.layout.right.iter())
            .flat_map(|island| island.segments.iter())
            .any(|seg| seg.module_id == "workspaces");
        if has_workspaces {
            let tx = updates_tx.clone();
            let wakeup = wakeup_tx.try_clone().map_err(|source| AbarError::Io {
                path: "/dev/null".into(),
                source,
            })?;
            spawn::ensure_runtime()?.spawn(crate::exec::run_exec_handler::<
                crate::modules::ScriptLine,
                _,
            >(
                "workspaces".to_string(),
                cmd,
                tx,
                wakeup,
                |line| ModuleUpdate::from_script("workspaces", line),
            ));
        }
    }

    // Spawn exec handler for window if configured.
    #[cfg(feature = "window")]
    if let Some(window_cfg) = modules.window
        && let Some(cmd) = window_cfg.exec
    {
        let max_length = window_cfg.max_length;
        let has_window = state
            .spec
            .layout
            .left
            .iter()
            .chain(state.spec.layout.center.iter())
            .chain(state.spec.layout.right.iter())
            .flat_map(|island| island.segments.iter())
            .any(|seg| seg.module_id == "window");
        if has_window {
            let tx = updates_tx.clone();
            let wakeup = wakeup_tx.try_clone().map_err(|source| AbarError::Io {
                path: "/dev/null".into(),
                source,
            })?;
            spawn::ensure_runtime()?.spawn(crate::exec::run_exec_handler::<
                crate::modules::ScriptLine,
                _,
            >(
                "window".to_string(),
                cmd,
                tx,
                wakeup,
                move |line| {
                    let mut update = ModuleUpdate::from_script("window", line);
                    if max_length > 0 {
                        update.text =
                            crate::modules::window::truncate_title(&update.text, max_length);
                    }
                    update
                },
            ));
        }
    }

    // Spawn exec handler for mpris if configured.
    #[cfg(feature = "mpris")]
    if let Some(mpris_cfg) = modules.mpris
        && let Some(cmd) = mpris_cfg.exec
    {
        let max_length = mpris_cfg.max_length;
        let has_mpris = state
            .spec
            .layout
            .left
            .iter()
            .chain(state.spec.layout.center.iter())
            .chain(state.spec.layout.right.iter())
            .flat_map(|island| island.segments.iter())
            .any(|seg| seg.module_id == "mpris");
        if has_mpris {
            let tx = updates_tx.clone();
            let wakeup = wakeup_tx.try_clone().map_err(|source| AbarError::Io {
                path: "/dev/null".into(),
                source,
            })?;
            spawn::ensure_runtime()?.spawn(crate::exec::run_exec_handler::<
                crate::modules::ScriptLine,
                _,
            >(
                "mpris".to_string(),
                cmd,
                tx,
                wakeup,
                move |line| {
                    let mut update = ModuleUpdate::from_script("mpris", line);
                    if max_length > 0 {
                        update.text =
                            crate::modules::window::truncate_title(&update.text, max_length);
                    }
                    update
                },
            ));
        }
    }

    // Spawn exec handler for tray if configured.
    #[cfg(feature = "tray")]
    if let Some(tray_cfg) = modules.tray
        && let Some(cmd) = tray_cfg.exec
    {
        let has_tray = state
            .spec
            .layout
            .left
            .iter()
            .chain(state.spec.layout.center.iter())
            .chain(state.spec.layout.right.iter())
            .flat_map(|island| island.segments.iter())
            .any(|seg| seg.module_id == "tray");
        if has_tray {
            let (tray_tx, tray_rx) =
                mpsc::sync_channel::<Vec<crate::modules::tray::ipc::MinimalTrayItem>>(16);
            state.tray_rx = Some(tray_rx);
            state.tray_events = tray_cfg.events.clone();
            state.tray_feed_id = tray_cfg.feed_id;
            let wakeup = wakeup_tx.try_clone().map_err(|source| AbarError::Io {
                path: "/dev/null".into(),
                source,
            })?;
            spawn::ensure_runtime()?.spawn(crate::modules::tray::run_tray_exec_handler(
                cmd, tray_tx, wakeup,
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

        // Apply any tray item list updates.
        #[cfg(feature = "tray")]
        loop {
            let items = state.tray_rx.as_ref().and_then(|rx| rx.try_recv().ok());
            let Some(items) = items else { break };
            if let Err(e) = state.apply_tray_update(items, &qh) {
                warn!(error = %e, "tray update repaint failed");
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

        let _ = tx.try_send(ModuleUpdate::text("clock", label));
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
    // Set when an Axis event fires; cleared on Frame. Prevents the paired
    // AxisDiscrete (which arrives after Axis) from double-counting the click.
    had_axis: bool,
    /// `(island_index, segment_index)` of the segment currently under the pointer.
    hovered: Option<(usize, usize)>,
    /// `(island_index, segment_index)` of the segment being pressed.
    pressed: Option<(usize, usize)>,
    /// True when the pointer is over the open submenu surface.
    on_submenu: bool,
    submenu_x: f64,
    submenu_y: f64,
}

struct SubmenuState {
    surface: wl_surface::WlSurface,
    layer_surface: ZwlrLayerSurfaceV1,
    pool: Option<wl_shm_pool::WlShmPool>,
    pool_file: Option<std::fs::File>,
    buffer: Option<wl_buffer::WlBuffer>,
    items: Vec<crate::model::SubmenuItemConfig>,
    hovered: Option<usize>,
    item_height: f64,
    width: u32,
    #[allow(dead_code)]
    height: u32,
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
    keyboard: Option<wl_keyboard::WlKeyboard>,
    submenu: Option<SubmenuState>,
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
    #[cfg(feature = "tray")]
    tray_rx: Option<mpsc::Receiver<Vec<crate::modules::tray::ipc::MinimalTrayItem>>>,
    #[cfg(feature = "tray")]
    tray_events: crate::model::SegmentEvents,
    #[cfg(feature = "tray")]
    tray_feed_id: bool,
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

    fn bind_keyboard(&mut self, seat: &wl_seat::WlSeat, qh: &QueueHandle<Self>) {
        if self.keyboard.is_some() {
            return;
        }
        let kb = seat.get_keyboard(qh, ());
        self.keyboard = Some(kb);
        debug!("keyboard bound");
    }

    /// Recompute hovered island from current pointer position and repaint if it changed.
    fn update_hover(&mut self, qh: &QueueHandle<Self>) {
        let x = self.pointer.x;
        let y = self.pointer.y;
        let new_hover = self
            .computed
            .as_ref()
            .and_then(|c| crate::hit_test::segment_coords_at(c, x, y));
        if new_hover != self.pointer.hovered {
            self.pointer.hovered = new_hover;
            if let Some(shm) = self.shm.clone()
                && let Err(e) = self.resize_and_paint(&shm, qh, self.bar_width, self.bar_height)
            {
                warn!(error = %e, "hover repaint failed");
            }
        }
    }

    /// Clear hover and pressed state on pointer leave, repaint if needed.
    fn clear_interaction(&mut self, qh: &QueueHandle<Self>) {
        let had = self.pointer.hovered.is_some() || self.pointer.pressed.is_some();
        self.pointer.hovered = None;
        self.pointer.pressed = None;
        if had
            && let Some(shm) = self.shm.clone()
            && let Err(e) = self.resize_and_paint(&shm, qh, self.bar_width, self.bar_height)
        {
            warn!(error = %e, "leave repaint failed");
        }
    }

    fn open_submenu(
        &mut self,
        items: Vec<crate::model::SubmenuItemConfig>,
        seg_x: f64,
        seg_right: f64,
        qh: &QueueHandle<Self>,
    ) {
        self.close_submenu(qh);

        // Measure item dimensions (needs font and style).
        let (width, height, item_height) = {
            let Some(font) = self.font.as_ref() else {
                return;
            };
            let mut max_w = 0.0_f64;
            let mut text_h = 0.0_f64;
            for item in &items {
                let (w, h) = font.measure(&item.content);
                max_w = max_w.max(w);
                text_h = text_h.max(h);
            }
            let item_h = text_h + 2.0 * self.spec.style.island_padding_y;
            let w = ((max_w + 2.0 * self.spec.style.island_padding_x).ceil() as u32).max(1);
            let h = ((item_h * items.len() as f64).ceil() as u32).max(1);
            (w, h, item_h)
        };

        let surface = {
            let Some(compositor) = self.compositor.as_ref() else {
                return;
            };
            compositor.create_surface(qh, ())
        };

        let layer_surface = {
            let Some(layer_shell) = self.layer_shell.as_ref() else {
                return;
            };
            layer_shell.get_layer_surface(
                &surface,
                None,
                Layer::Overlay,
                "abar-submenu".to_string(),
                qh,
                true,
            )
        };

        // Always anchor Top|Left. For the normal case align the submenu's left edge with
        // the segment's left edge. If that would overflow the right edge of the output,
        // shift left so the submenu's right edge aligns with the segment's right edge instead.
        let bar_w = self.bar_width as f64;
        let left_margin = if seg_x + width as f64 <= bar_w {
            seg_x
        } else {
            (seg_right - width as f64).max(0.0)
        };

        layer_surface.set_anchor(Anchor::Top | Anchor::Left);
        layer_surface.set_margin(0, 0, 0, left_margin as i32);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer_surface.set_size(width, height);
        surface.commit();

        self.submenu = Some(SubmenuState {
            surface,
            layer_surface,
            pool: None,
            pool_file: None,
            buffer: None,
            items,
            hovered: None,
            item_height,
            width,
            height,
        });
    }

    fn close_submenu(&mut self, _qh: &QueueHandle<Self>) {
        if let Some(sm) = self.submenu.take() {
            sm.layer_surface.destroy();
            sm.surface.destroy();
        }
        self.pointer.on_submenu = false;
    }

    fn repaint_submenu(&mut self, qh: &QueueHandle<Self>) -> Result<(), AbarError> {
        let (items, hovered, item_height) = match self.submenu.as_ref() {
            Some(sm) => (sm.items.clone(), sm.hovered, sm.item_height),
            None => return Ok(()),
        };
        let Some(shm) = self.shm.clone() else {
            return Ok(());
        };
        let Some(font) = self.font.as_ref() else {
            return Ok(());
        };

        let frame = crate::render::paint_submenu(
            &items,
            &self.spec.style,
            &self.spec.colors,
            hovered,
            item_height,
            font,
        )?;

        let stride = frame.stride;
        let buf_h = frame.height;
        let size = (stride as u64)
            .checked_mul(buf_h as u64)
            .ok_or_else(|| AbarError::WaylandProtocol("submenu buffer size overflow".into()))?;

        {
            let sm = self.submenu.as_mut().unwrap();
            sm.buffer = None;
            sm.pool = None;
            sm.pool_file = None;
        }

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
            frame.width as i32,
            buf_h as i32,
            stride,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );

        let sm = self.submenu.as_mut().unwrap();
        sm.surface.attach(Some(&buffer), 0, 0);
        sm.surface
            .damage_buffer(0, 0, frame.width as i32, buf_h as i32);
        sm.surface.commit();
        sm.pool_file = Some(file);
        sm.pool = Some(pool);
        sm.buffer = Some(buffer);

        Ok(())
    }

    fn update_submenu_hover(&mut self, qh: &QueueHandle<Self>) {
        let x = self.pointer.submenu_x;
        let y = self.pointer.submenu_y;
        let new_hovered = self.submenu.as_ref().and_then(|sm| {
            let idx = (y / sm.item_height) as usize;
            if x >= 0.0 && x <= sm.width as f64 && y >= 0.0 && idx < sm.items.len() {
                Some(idx)
            } else {
                None
            }
        });
        let changed = self
            .submenu
            .as_ref()
            .is_some_and(|sm| sm.hovered != new_hovered);
        if changed {
            if let Some(sm) = self.submenu.as_mut() {
                sm.hovered = new_hovered;
            }
            if let Err(e) = self.repaint_submenu(qh) {
                warn!(error = %e, "submenu hover repaint failed");
            }
        }
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
                crate::hit_test::hit_test(computed, x, y).is_some_and(|s| s.module_id == "clock")
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
                if let Err(e) = self.apply_update(ModuleUpdate::text("clock", label), _qh) {
                    warn!(error = %e, "clock tz update repaint failed");
                }
                return;
            }
        }

        // Left click on a segment with a submenu opens/closes it instead of running on_left_click.
        if matches!(action, PointerAction::LeftClick) {
            let submenu_info = {
                let computed = self.computed.as_ref().unwrap();
                crate::hit_test::segment_coords_at(computed, x, y).and_then(
                    |(island_idx, seg_idx)| {
                        let island = &computed.islands[island_idx];
                        let seg = &island.segments[seg_idx];
                        if seg.submenu.is_empty() {
                            None
                        } else {
                            Some((seg.submenu.clone(), seg.x, seg.x + seg.width))
                        }
                    },
                )
            };
            if let Some((items, seg_x, seg_right)) = submenu_info {
                if self.submenu.is_some() {
                    self.close_submenu(_qh);
                } else {
                    self.open_submenu(items, seg_x, seg_right, _qh);
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
        seg.label = update.text;
        seg.use_markup = update.use_markup;

        // Only repaint once the layer surface has been configured and painted at least once.
        if self.computed.is_none() {
            return Ok(());
        }
        let Some(shm) = self.shm.clone() else {
            return Ok(());
        };
        self.resize_and_paint(&shm, qh, self.bar_width, self.bar_height)
    }

    /// Replace tray segments in the layout with one icon-only segment per visible item,
    /// then repaint. No-op if the bar hasn't been painted yet or no tray slot exists.
    #[cfg(feature = "tray")]
    fn apply_tray_update(
        &mut self,
        items: Vec<crate::modules::tray::ipc::MinimalTrayItem>,
        qh: &QueueHandle<Self>,
    ) -> Result<(), AbarError> {
        use crate::model::Segment;
        use crate::modules::tray::ipc::TrayItemStatus;

        // Build one segment per visible item.
        // Icon resolution order: icon_handle → title (as FreeDesktop name) → text fallback.
        // Events from config; when feed_id is set, each configured on_* command gets ` <app_id>` appended.
        let size = self.spec.style.font_size.round() as u32;
        let tray_feed_id = self.tray_feed_id;
        let tray_events = self.tray_events.clone();
        let mut new_segs: Vec<Segment> = Vec::new();
        for i in items.iter().filter(|i| i.status != TrayItemStatus::Passive) {
            // Prefer icon_handle; fall back to title as a FreeDesktop icon name.
            let icon_name = i
                .icon_handle
                .as_deref()
                .or(i.title.as_deref())
                .and_then(|name| self.icon_cache.get(name, size).map(|_| name.to_string()));
            let overlay_name = i.overlay_icon_handle.as_deref().and_then(|name| {
                let overlay_px = (size as f64 * 0.55).round().max(1.0) as u32;
                self.icon_cache
                    .get(name, overlay_px)
                    .map(|_| name.to_string())
            });
            let fallback_label = i
                .tooltip_title
                .as_deref()
                .or(i.title.as_deref())
                .unwrap_or(&i.app_id);
            let mut seg = match icon_name {
                Some(name) => {
                    let mut seg = Segment::icon_only(format!("tray:{}", i.app_id), name);
                    seg.label = fallback_label.to_string();
                    seg.overlay_icon_name = overlay_name;
                    seg
                }
                None => Segment::new(format!("tray:{}", i.app_id), fallback_label.to_string()),
            };
            let mut events = tray_events.clone();
            if tray_feed_id {
                let id = &i.app_id;
                let append = |opt: Option<String>| opt.map(|cmd| format!("{cmd} {id}"));
                events.on_left_click = append(events.on_left_click);
                events.on_right_click = append(events.on_right_click);
                events.on_middle_click = append(events.on_middle_click);
                events.on_scroll_up = append(events.on_scroll_up);
                events.on_scroll_down = append(events.on_scroll_down);
            }
            seg.events = events;
            new_segs.push(seg);
        }

        // Locate the island that holds the "tray" placeholder or existing "tray:*" segments.
        let pos = {
            let zones: [&Vec<crate::model::Island>; 3] = [
                &self.spec.layout.left,
                &self.spec.layout.center,
                &self.spec.layout.right,
            ];
            let mut found = None;
            'find: for (zi, zone) in zones.iter().enumerate() {
                for (ii, island) in zone.iter().enumerate() {
                    if island
                        .segments
                        .iter()
                        .any(|s| s.module_id == "tray" || s.module_id.starts_with("tray:"))
                    {
                        found = Some((zi, ii));
                        break 'find;
                    }
                }
            }
            found
        };

        let Some((zone_idx, island_idx)) = pos else {
            return Ok(());
        };

        let zone = match zone_idx {
            0 => &mut self.spec.layout.left,
            1 => &mut self.spec.layout.center,
            _ => &mut self.spec.layout.right,
        };
        let island = &mut zone[island_idx];

        // Find the contiguous span of existing tray segments and splice in the new ones.
        let tray_start = island
            .segments
            .iter()
            .position(|s| s.module_id == "tray" || s.module_id.starts_with("tray:"))
            .unwrap_or(0);
        let tray_count = island
            .segments
            .iter()
            .filter(|s| s.module_id == "tray" || s.module_id.starts_with("tray:"))
            .count();
        island
            .segments
            .splice(tray_start..tray_start + tray_count, new_segs);

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

        let computed = compute_bar(&self.spec, width, &|text, is_markup| {
            if is_markup {
                font.measure_markup(text)
            } else {
                font.measure(text)
            }
        });
        let frame = paint_computed(
            &self.spec,
            &computed,
            font,
            &mut self.icon_cache,
            self.pointer.hovered,
            self.pointer.pressed,
        )?;
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
                state.pointer.on_submenu = state
                    .submenu
                    .as_ref()
                    .is_some_and(|sm| surface == sm.surface);
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if state.surface.as_ref().is_some_and(|ours| &surface == ours) {
                    state.pointer.on_surface = false;
                    state.clear_interaction(qh);
                }
                if state
                    .submenu
                    .as_ref()
                    .is_some_and(|sm| surface == sm.surface)
                {
                    state.pointer.on_submenu = false;
                    let needs_repaint = state
                        .submenu
                        .as_ref()
                        .is_some_and(|sm| sm.hovered.is_some());
                    if needs_repaint {
                        if let Some(sm) = state.submenu.as_mut() {
                            sm.hovered = None;
                        }
                        if let Err(e) = state.repaint_submenu(qh) {
                            warn!(error = %e, "submenu leave repaint failed");
                        }
                    }
                }
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                if state.pointer.on_submenu {
                    state.pointer.submenu_x = surface_x;
                    state.pointer.submenu_y = surface_y;
                    state.update_submenu_hover(qh);
                } else {
                    state.pointer.x = surface_x;
                    state.pointer.y = surface_y;
                    state.update_hover(qh);
                }
            }
            wl_pointer::Event::Button {
                button,
                state: btn_state,
                ..
            } => {
                if btn_state == WEnum::Value(ButtonState::Pressed) {
                    if state.pointer.on_submenu {
                        // Click inside submenu: run the hovered item's action then close.
                        if button == BTN_LEFT {
                            let action = state
                                .submenu
                                .as_ref()
                                .and_then(|sm| sm.hovered.map(|idx| sm.items[idx].action.clone()));
                            state.close_submenu(qh);
                            if let Some(cmd) = action {
                                spawn::spawn_shell_command(&cmd);
                            }
                        }
                    } else if state.submenu.is_some() {
                        // Click outside the submenu (on bar or elsewhere): just close it.
                        state.close_submenu(qh);
                    } else {
                        // Normal bar button press.
                        state.pointer.pressed = state.pointer.hovered;
                        if let Some(shm) = state.shm.clone()
                            && let Err(e) =
                                state.resize_and_paint(&shm, qh, state.bar_width, state.bar_height)
                        {
                            warn!(error = %e, "press repaint failed");
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
                } else if btn_state == WEnum::Value(ButtonState::Released)
                    && state.pointer.pressed.is_some()
                {
                    state.pointer.pressed = None;
                    if let Some(shm) = state.shm.clone()
                        && let Err(e) =
                            state.resize_and_paint(&shm, qh, state.bar_width, state.bar_height)
                    {
                        warn!(error = %e, "release repaint failed");
                    }
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

impl Dispatch<wl_keyboard::WlKeyboard, ()> for AppState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        // Esc (evdev key 1) closes any open submenu.
        if let wl_keyboard::Event::Key {
            key,
            state: key_state,
            ..
        } = event
        {
            use wayland_client::protocol::wl_keyboard::KeyState;
            if key_state == WEnum::Value(KeyState::Pressed) && key == 1 {
                state.close_submenu(qh);
            }
        }
    }
}

/// Dispatch for the submenu layer surface (userdata = `true` distinguishes it from the bar).
impl Dispatch<ZwlrLayerSurfaceV1, bool> for AppState {
    fn event(
        state: &mut Self,
        layer_surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &bool,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                layer_surface.ack_configure(serial);
                if let Err(e) = state.repaint_submenu(qh) {
                    warn!(error = %e, "submenu configure repaint failed");
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                debug!("submenu layer surface closed");
                // Drop the state but don't call close_submenu (which would call destroy() again).
                state.submenu = None;
                state.pointer.on_submenu = false;
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
