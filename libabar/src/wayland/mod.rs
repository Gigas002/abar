use std::io::Read;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
#[cfg(feature = "clock")]
use std::sync::atomic::{AtomicUsize, Ordering};
use wayland_client::protocol::wl_keyboard;

use rustix::event::{PollFd, PollFlags, poll};
use tracing::{debug, warn};
use wayland_client::protocol::wl_pointer::{Axis, ButtonState};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_output, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface,
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
use crate::modules::{Mailbox, ModuleConfigs, ModuleUpdate};
use crate::render::{FontContext, paint_computed};
use crate::spawn;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

/// Blocks until the layer surface is closed or an unrecoverable error occurs.
pub fn run_bar(spec: BarSpec, modules: ModuleConfigs) -> Result<(), AbarError> {
    let runtime = spawn::ensure_runtime()?;
    // Read before the per-feature blocks below move fields out of it.
    let _ = &modules;

    let io_err = |source| AbarError::Io {
        path: "wakeup socket".into(),
        source,
    };
    let (wakeup_rx, wakeup_tx) = UnixStream::pair().map_err(io_err)?;
    wakeup_rx.set_nonblocking(true).map_err(io_err)?;
    let mailbox = Arc::new(Mailbox::new(wakeup_tx).map_err(io_err)?);

    let conn = Connection::connect_to_env()?;
    let display = conn.display();
    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    display.get_registry(&qh, ());

    let icon_mode = if spec.style.icon_prefer_theme {
        crate::icon::IconLookupMode::PreferTheme
    } else {
        crate::icon::IconLookupMode::Exact
    };

    let mut state = AppState {
        running: true,
        spec,
        compositor: None,
        shm: None,
        layer_shell: None,
        seat: None,
        outputs: Vec::new(),
        bars: Vec::new(),
        pointer: PointerState::default(),
        keyboard: None,
        submenu: None,
        icon_cache: IconCache::with_mode(icon_mode),
        font: None,
        mailbox: Arc::clone(&mailbox),
        #[cfg(feature = "clock")]
        clock_tz_index: Arc::new(AtomicUsize::new(0)),
        #[cfg(feature = "clock")]
        clock_timezones: Vec::new(),
        #[cfg(feature = "clock")]
        clock_formats: Vec::new(),
        #[cfg(feature = "tray")]
        tray_events: crate::model::SegmentEvents::default(),
        #[cfg(feature = "tray")]
        tray_feed_id: false,
        #[cfg(feature = "tray")]
        tray_island: None,
    };

    // Spawn clock background task.
    #[cfg(feature = "clock")]
    if let Some(clock_cfg) = modules.clock {
        state.clock_timezones = clock_cfg.timezones.clone();
        state.clock_formats = clock_cfg.formats.clone();
        let tz_index = state.clock_tz_index.clone();
        runtime.spawn(clock_task(Arc::clone(&mailbox), clock_cfg, tz_index));
    }

    // Exec-driven modules run only when they are on the bar.
    let on_bar = |module: &str| {
        let layout = &state.spec.layout;
        layout
            .left
            .iter()
            .chain(&layout.center)
            .chain(&layout.right)
            .flat_map(|island| &island.segments)
            .any(|seg| seg.module_id == module)
    };
    let text_module = |module: &'static str, max_length: usize| {
        let mailbox = Arc::clone(&mailbox);
        move |line: crate::modules::ScriptLine| {
            let mut update = ModuleUpdate::from_script(module, line);
            if max_length > 0 {
                update.text = crate::modules::truncate_title(&update.text, max_length);
            }
            mailbox.post(update);
        }
    };
    let _ = &text_module;

    #[cfg(feature = "keyboard")]
    if let Some(cmd) = modules.keyboard.and_then(|k| k.exec)
        && on_bar("keyboard")
    {
        let mailbox = Arc::clone(&mailbox);
        runtime.spawn(crate::exec::run_exec_handler(
            "keyboard".into(),
            cmd,
            move |data: crate::modules::keyboard::KeyboardData| {
                mailbox.post(ModuleUpdate::text("keyboard", data.label));
            },
        ));
    }

    #[cfg(feature = "workspaces")]
    if let Some(cmd) = modules.workspaces.and_then(|w| w.exec)
        && on_bar("workspaces")
    {
        runtime.spawn(crate::exec::run_exec_handler(
            "workspaces".into(),
            cmd,
            text_module("workspaces", 0),
        ));
    }

    #[cfg(feature = "window")]
    if let Some(cfg) = modules.window
        && let Some(cmd) = cfg.exec
        && on_bar("window")
    {
        runtime.spawn(crate::exec::run_exec_handler(
            "window".into(),
            cmd,
            text_module("window", cfg.max_length),
        ));
    }

    #[cfg(feature = "mpris")]
    if let Some(cfg) = modules.mpris
        && let Some(cmd) = cfg.exec
        && on_bar("mpris")
    {
        runtime.spawn(crate::exec::run_exec_handler(
            "mpris".into(),
            cmd,
            text_module("mpris", cfg.max_length),
        ));
    }

    #[cfg(feature = "tray")]
    if let Some(tray_cfg) = modules.tray
        && let Some(cmd) = tray_cfg.exec.clone()
        && on_bar("tray")
    {
        state.tray_events = tray_cfg.events.clone();
        state.tray_feed_id = tray_cfg.feed_id;
        let mailbox = Arc::clone(&mailbox);
        runtime.spawn(crate::exec::run_exec_handler(
            "tray".into(),
            cmd,
            move |items: Vec<crate::modules::tray::ipc::MinimalTrayItem>| mailbox.post_tray(items),
        ));
    }

    // Keep the bindings used when no module features are active.
    let _ = (&on_bar, runtime);
    drop(mailbox);

    loop {
        event_queue
            .flush()
            .map_err(|e| AbarError::WaylandProtocol(format!("flush failed: {e}")))?;

        event_queue
            .dispatch_pending(&mut state)
            .map_err(|e| AbarError::WaylandProtocol(format!("dispatch failed: {e}")))?;

        // Apply everything the background tasks posted, then repaint once.
        if let Err(e) = state.apply_pending(&qh) {
            warn!(error = %e, "module update repaint failed");
        }

        if !state.running {
            break;
        }

        // Flush any surface commits queued by apply_pending before blocking in poll;
        // without this the compositor would not see the updated buffer until the next
        // Wayland event (e.g. pointer motion) triggered another flush.
        event_queue
            .flush()
            .map_err(|e| AbarError::WaylandProtocol(format!("flush failed: {e}")))?;

        // Acquire the Wayland read lock; if events are already pending just loop.
        let Some(read_guard) = event_queue.prepare_read() else {
            continue;
        };

        // Poll the Wayland connection and the wakeup socket; None = wait indefinitely.
        let mut pollfds = [
            PollFd::from_borrowed_fd(read_guard.connection_fd(), PollFlags::IN),
            PollFd::new(&wakeup_rx, PollFlags::IN),
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
        let wayland_ready = pollfds[0].revents().contains(PollFlags::IN);

        // Drain the wakeup socket so it doesn't stay readable forever.
        let mut buf = [0u8; 256];
        while matches!((&wakeup_rx).read(&mut buf), Ok(n) if n > 0) {}

        if wayland_ready {
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
    mailbox: Arc<Mailbox>,
    config: crate::modules::clock::ClockConfig,
    tz_index: Arc<AtomicUsize>,
) {
    use tokio::time::{Duration, sleep};

    loop {
        let ms = crate::modules::clock::ms_until_next_tick();
        sleep(Duration::from_millis(ms)).await;

        let idx = tz_index.load(Ordering::Relaxed);
        let tz = config.timezones.get(idx).copied();
        let fmt = config.formats.first().map_or("%H:%M", |s| s.as_str());
        let label = crate::modules::clock::current_label(fmt, tz);

        mailbox.post(ModuleUpdate::text("clock", label));
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Default)]
struct PointerState {
    pointer: Option<wl_pointer::WlPointer>,
    /// Output (registry) name of the bar surface the pointer is currently over, if any.
    active_bar_output: Option<u32>,
    x: f64,
    y: f64,
    /// Vertical scroll collected over a pointer frame.
    scroll: input::ScrollAccumulator,
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
    buffer: Option<wl_buffer::WlBuffer>,
    items: Vec<crate::model::SubmenuItemConfig>,
    hovered: Option<usize>,
    item_height: f64,
    width: u32,
    #[allow(dead_code)]
    height: u32,
}

/// A bound Wayland output, tracked so a bar can be created/destroyed for it.
struct OutputInfo {
    /// Registry global name; used to correlate `GlobalRemove` events and to
    /// tie a `Bar` back to the output it was created for.
    name: u32,
    output: wl_output::WlOutput,
}

/// Per-output bar surface state. One `Bar` is created for each connected
/// output so abar shows on every monitor instead of just one.
struct Bar {
    /// Registry global name of the `wl_output` this bar is anchored to.
    output_name: u32,
    surface: wl_surface::WlSurface,
    layer_surface: ZwlrLayerSurfaceV1,
    pending_configure: Option<(u32, u32, u32)>,
    /// The buffer last attached; released buffers destroy themselves.
    buffer: Option<wl_buffer::WlBuffer>,
    bar_width: u32,
    bar_height: u32,
    computed: Option<ComputedBar>,
}

impl Bar {
    fn destroy(self) {
        self.layer_surface.destroy();
        self.surface.destroy();
        if let Some(buffer) = self.buffer {
            buffer.destroy();
        }
    }
}

struct AppState {
    running: bool,
    spec: BarSpec,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    seat: Option<wl_seat::WlSeat>,
    /// Every currently bound output; used to spin up a `Bar` per monitor.
    outputs: Vec<OutputInfo>,
    /// One bar per connected output.
    bars: Vec<Bar>,
    pointer: PointerState,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    submenu: Option<SubmenuState>,
    icon_cache: IconCache,
    font: Option<FontContext>,
    /// Updates posted by background tasks (exec scripts, clock).
    mailbox: Arc<Mailbox>,
    #[cfg(feature = "clock")]
    clock_tz_index: Arc<AtomicUsize>,
    #[cfg(feature = "clock")]
    clock_timezones: Vec<chrono_tz::Tz>,
    #[cfg(feature = "clock")]
    clock_formats: Vec<String>,
    #[cfg(feature = "tray")]
    tray_events: crate::model::SegmentEvents,
    #[cfg(feature = "tray")]
    tray_feed_id: bool,
    /// Cached (zone_idx, island_idx, tray_start) from the last successful tray
    /// placement.  Preserved across empty-item updates so we can repopulate the
    /// tray after trayd restarts without losing the slot position.
    #[cfg(feature = "tray")]
    tray_island: Option<(usize, usize, usize)>,
}

impl AppState {
    /// Create a `Bar` for every known output that doesn't already have one.
    /// No-op until the compositor and layer-shell globals are both bound.
    fn create_bars_for_new_outputs(&mut self, qh: &QueueHandle<Self>) {
        let Some(compositor) = self.compositor.clone() else {
            return;
        };
        let Some(layer_shell) = self.layer_shell.clone() else {
            return;
        };

        // Reuse a previously-painted height as the initial exclusive zone so a
        // newly hotplugged monitor doesn't briefly show a 1px-tall bar.
        let initial_height = self
            .bars
            .iter()
            .find_map(|b| b.computed.as_ref().map(|_| b.bar_height))
            .unwrap_or(1);

        let output_names: Vec<u32> = self.outputs.iter().map(|o| o.name).collect();
        for name in output_names {
            if self.bars.iter().any(|b| b.output_name == name) {
                continue;
            }
            let Some(output) = self.outputs.iter().find(|o| o.name == name) else {
                continue;
            };

            let surface = compositor.create_surface(qh, ());
            let layer_surface = layer_shell.get_layer_surface(
                &surface,
                Some(&output.output),
                Layer::Top,
                "abar".into(),
                qh,
                name,
            );

            layer_surface.set_anchor(Anchor::Top | Anchor::Left | Anchor::Right);
            layer_surface.set_exclusive_zone(initial_height as i32);
            layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
            layer_surface.set_size(0, initial_height);

            surface.commit();

            self.bars.push(Bar {
                output_name: name,
                surface,
                layer_surface,
                pending_configure: None,
                buffer: None,
                bar_width: 1,
                bar_height: initial_height,
                computed: None,
            });
            debug!(output_name = name, "layer surface created for output");
        }
    }

    /// Destroy the bar (if any) associated with a `wl_output` that just went away.
    fn remove_output(&mut self, name: u32) {
        self.outputs.retain(|o| o.name != name);

        if let Some(idx) = self.bars.iter().position(|b| b.output_name == name) {
            let bar = self.bars.remove(idx);
            bar.destroy();

            if self.pointer.active_bar_output == Some(name) {
                self.pointer.active_bar_output = None;
                self.pointer.hovered = None;
                self.pointer.pressed = None;
            }
            debug!(output_name = name, "bar removed for disconnected output");
        }
    }

    /// Index into `self.bars` of the bar currently under the pointer, if any.
    fn active_bar_idx(&self) -> Option<usize> {
        let name = self.pointer.active_bar_output?;
        self.bars.iter().position(|b| b.output_name == name)
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
        let Some(bar_idx) = self.active_bar_idx() else {
            return;
        };
        let x = self.pointer.x;
        let y = self.pointer.y;
        let new_hover = self.bars[bar_idx]
            .computed
            .as_ref()
            .and_then(|c| crate::hit_test::segment_coords_at(c, x, y));
        if new_hover != self.pointer.hovered {
            self.pointer.hovered = new_hover;
            if let Some(shm) = self.shm.clone() {
                let (w, h) = (self.bars[bar_idx].bar_width, self.bars[bar_idx].bar_height);
                if let Err(e) = self.resize_and_paint(bar_idx, &shm, qh, w, h) {
                    warn!(error = %e, "hover repaint failed");
                }
            }
        }
    }

    /// Clear hover and pressed state on pointer leave, repaint if needed.
    fn clear_interaction(&mut self, output_name: u32, qh: &QueueHandle<Self>) {
        let had = self.pointer.hovered.is_some() || self.pointer.pressed.is_some();
        self.pointer.hovered = None;
        self.pointer.pressed = None;
        let Some(bar_idx) = self.bars.iter().position(|b| b.output_name == output_name) else {
            return;
        };
        if had && let Some(shm) = self.shm.clone() {
            let (w, h) = (self.bars[bar_idx].bar_width, self.bars[bar_idx].bar_height);
            if let Err(e) = self.resize_and_paint(bar_idx, &shm, qh, w, h) {
                warn!(error = %e, "leave repaint failed");
            }
        }
    }

    fn open_submenu(
        &mut self,
        items: Vec<crate::model::SubmenuItemConfig>,
        seg_x: f64,
        seg_right: f64,
        bar_idx: usize,
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

        let bar_output_name = self.bars[bar_idx].output_name;
        let output = self
            .outputs
            .iter()
            .find(|o| o.name == bar_output_name)
            .map(|o| &o.output);

        let layer_surface = {
            let Some(layer_shell) = self.layer_shell.as_ref() else {
                return;
            };
            layer_shell.get_layer_surface(
                &surface,
                output,
                Layer::Overlay,
                "abar-submenu".to_string(),
                qh,
                true,
            )
        };

        // Always anchor Top|Left. For the normal case align the submenu's left edge with
        // the segment's left edge. If that would overflow the right edge of the output,
        // shift left so the submenu's right edge aligns with the segment's right edge instead.
        let bar_w = self.bars[bar_idx].bar_width as f64;
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
            if let Some(buffer) = sm.buffer {
                buffer.destroy();
            }
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

        let buffer = shm_buffer(&shm, &frame, qh)?;
        let sm = self.submenu.as_mut().unwrap();
        sm.surface.attach(Some(&buffer), 0, 0);
        sm.surface
            .damage_buffer(0, 0, frame.width as i32, frame.height as i32);
        sm.surface.commit();
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

    fn dispatch_pointer_action(&mut self, action: PointerAction, qh: &QueueHandle<Self>) {
        let Some(bar_idx) = self.active_bar_idx() else {
            return;
        };
        if self.bars[bar_idx].computed.is_none() {
            return;
        }
        let x = self.pointer.x;
        let y = self.pointer.y;

        #[cfg(feature = "clock")]
        if matches!(action, PointerAction::ScrollUp | PointerAction::ScrollDown)
            && self.clock_timezones.len() > 1
        {
            let clock_hit = {
                let computed = self.bars[bar_idx].computed.as_ref().unwrap();
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
                self.set_module(ModuleUpdate::text("clock", label));
                if let Err(e) = self.repaint_all_bars(qh) {
                    warn!(error = %e, "clock tz update repaint failed");
                }
                return;
            }
        }

        // Left click on a segment with a submenu opens/closes it instead of running on_left_click.
        if matches!(action, PointerAction::LeftClick) {
            let submenu_info = {
                let computed = self.bars[bar_idx].computed.as_ref().unwrap();
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
                    self.close_submenu(qh);
                } else {
                    self.open_submenu(items, seg_x, seg_right, bar_idx, qh);
                }
                return;
            }
        }

        let computed = self.bars[bar_idx].computed.as_ref().unwrap();
        input::dispatch_pointer_action(computed, x, y, action);
    }

    fn on_configure(
        &mut self,
        bar_idx: usize,
        serial: u32,
        width: u32,
        height: u32,
        qh: &QueueHandle<Self>,
    ) -> Result<(), AbarError> {
        let width = width.max(1);
        let height = height.max(1);

        let Some(shm) = self.shm.clone() else {
            self.bars[bar_idx].pending_configure = Some((width, height, serial));
            return Ok(());
        };

        self.bars[bar_idx].layer_surface.ack_configure(serial);
        self.resize_and_paint(bar_idx, &shm, qh, width, height)
    }

    fn try_flush_pending_configure(&mut self, qh: &QueueHandle<Self>) -> Result<(), AbarError> {
        let Some(shm) = self.shm.clone() else {
            return Ok(());
        };
        for idx in 0..self.bars.len() {
            let Some((w, h, serial)) = self.bars[idx].pending_configure.take() else {
                continue;
            };
            self.bars[idx].layer_surface.ack_configure(serial);
            self.resize_and_paint(idx, &shm, qh, w, h)?;
        }
        Ok(())
    }

    /// Apply everything waiting in the mailbox and repaint once if anything changed.
    fn apply_pending(&mut self, qh: &QueueHandle<Self>) -> Result<(), AbarError> {
        let pending = self.mailbox.take();
        let mut changed = false;
        for update in pending.modules {
            changed |= self.set_module(update);
        }
        #[cfg(feature = "tray")]
        if let Some(items) = pending.tray {
            changed |= self.set_tray(items);
        }
        if changed {
            self.repaint_all_bars(qh)?;
        }
        Ok(())
    }

    /// Store a module's new label; `false` when nothing visible changed.
    fn set_module(&mut self, update: ModuleUpdate) -> bool {
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
            return false;
        };
        if seg.label == update.text && seg.use_markup == update.use_markup {
            return false;
        }
        seg.label = update.text;
        seg.use_markup = update.use_markup;
        true
    }

    /// Replace tray segments in the layout with one icon-only segment per visible item;
    /// `false` when nothing visible changed or no tray slot exists.
    #[cfg(feature = "tray")]
    fn set_tray(&mut self, items: Vec<crate::modules::tray::ipc::MinimalTrayItem>) -> bool {
        use crate::model::Segment;
        use crate::modules::tray::ipc::TrayItemStatus;

        // Build one segment per visible item.
        // Icon resolution order: icon_handle → tooltip_title (lowercased) → title (lowercased) → text fallback.
        // Events from config; when feed_id is set, each configured on_* command gets ` <app_id>` appended.
        let size = self.spec.style.font_size.round() as u32;
        let tray_feed_id = self.tray_feed_id;
        let tray_events = self.tray_events.clone();
        let mut new_segs: Vec<Segment> = Vec::new();
        for i in items.iter().filter(|i| i.status != TrayItemStatus::Passive) {
            // Prefer icon_handle; then try tooltip_title/title lowercased as icon names.
            let icon_name = i
                .icon_handle
                .as_deref()
                .and_then(|name| self.icon_cache.get(name, size).map(|_| name.to_string()))
                .or_else(|| {
                    // Try tooltip_title lowercased as a FreeDesktop icon name
                    // (e.g. "Discord" → "discord" which exists in many icon themes).
                    i.tooltip_title
                        .as_deref()
                        .map(|t| t.to_lowercase())
                        .and_then(|name| self.icon_cache.get(&name, size).map(|_| name))
                })
                .or_else(|| {
                    // Try title lowercased as a FreeDesktop icon name.
                    i.title
                        .as_deref()
                        .map(|t| t.to_lowercase())
                        .and_then(|name| self.icon_cache.get(&name, size).map(|_| name))
                });
            let mut seg = match icon_name {
                Some(name) => Segment::icon_only(format!("tray:{}", i.app_id), name),
                None => {
                    // No resolvable icon: show tooltip_title/title/app_id as text.
                    let label = i
                        .tooltip_title
                        .as_deref()
                        .or(i.title.as_deref())
                        .unwrap_or(&i.app_id)
                        .to_string();
                    Segment::new(format!("tray:{}", i.app_id), label)
                }
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
        // Fall back to the cached position from a previous update so the slot survives a
        // clear (when items == [] removes all tray:* segments from the island).
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

        // If the live search found the island, update the cache with the current tray_start
        // so we know exactly where to splice on future updates (including after a clear).
        // If not found, fall back to the cached slot — this happens after an empty-item
        // update wiped all tray:* segments and trayd has now restarted with new items.
        let (zone_idx, island_idx, tray_start) = if let Some((zi, ii)) = pos {
            let zone = match zi {
                0 => &self.spec.layout.left,
                1 => &self.spec.layout.center,
                _ => &self.spec.layout.right,
            };
            let ts = zone[ii]
                .segments
                .iter()
                .position(|s| s.module_id == "tray" || s.module_id.starts_with("tray:"))
                .unwrap_or(0);
            self.tray_island = Some((zi, ii, ts));
            (zi, ii, ts)
        } else if let Some(cached) = self.tray_island {
            cached
        } else {
            return false;
        };

        // Splice the new segments into the island at the cached tray position.
        let zone = match zone_idx {
            0 => &mut self.spec.layout.left,
            1 => &mut self.spec.layout.center,
            _ => &mut self.spec.layout.right,
        };
        let island = &mut zone[island_idx];
        let tray_count = island
            .segments
            .iter()
            .filter(|s| s.module_id == "tray" || s.module_id.starts_with("tray:"))
            .count();
        let old = &island.segments[tray_start..tray_start + tray_count];
        if old == new_segs.as_slice() {
            return false;
        }
        island
            .segments
            .splice(tray_start..tray_start + tray_count, new_segs);
        true
    }

    /// Repaint every bar that has already been configured/painted at least once,
    /// preserving each bar's own size. Called after the shared `spec` changes
    /// (module label/tray updates) so every connected output picks up the change.
    fn repaint_all_bars(&mut self, qh: &QueueHandle<Self>) -> Result<(), AbarError> {
        let Some(shm) = self.shm.clone() else {
            return Ok(());
        };
        for idx in 0..self.bars.len() {
            if self.bars[idx].computed.is_none() {
                continue;
            }
            let (w, h) = (self.bars[idx].bar_width, self.bars[idx].bar_height);
            self.resize_and_paint(idx, &shm, qh, w, h)?;
        }
        Ok(())
    }

    fn resize_and_paint(
        &mut self,
        bar_idx: usize,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<Self>,
        width: u32,
        height: u32,
    ) -> Result<(), AbarError> {
        self.bars[bar_idx].bar_width = width;

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
        // Only the bar currently under the pointer should show hover/press state.
        let is_active = self.active_bar_idx() == Some(bar_idx);
        let hovered = if is_active {
            self.pointer.hovered
        } else {
            None
        };
        let pressed = if is_active {
            self.pointer.pressed
        } else {
            None
        };
        let frame = paint_computed(
            &self.spec,
            &computed,
            font,
            &mut self.icon_cache,
            hovered,
            pressed,
        )?;
        let bar = &mut self.bars[bar_idx];
        bar.bar_height = frame.height;
        bar.computed = Some(computed);

        bar.layer_surface.set_exclusive_zone(frame.height as i32);
        bar.layer_surface.set_size(0, frame.height);

        let buffer = shm_buffer(shm, &frame, qh)?;
        let bar = &mut self.bars[bar_idx];
        bar.surface.attach(Some(&buffer), 0, 0);
        bar.surface
            .damage_buffer(0, 0, width as i32, frame.height as i32);
        bar.surface.commit();
        bar.buffer = Some(buffer);

        let _ = height;
        Ok(())
    }
}

/// Copy `frame` into a fresh shared-memory buffer.
///
/// The pool is destroyed right away (the compositor keeps the memory alive for
/// as long as the buffer exists), and the buffer destroys itself when the
/// compositor releases it (see the `wl_buffer` dispatch), so nothing piles up
/// on the compositor side however often the bar repaints.
fn shm_buffer(
    shm: &wl_shm::WlShm,
    frame: &crate::render::Frame,
    qh: &QueueHandle<AppState>,
) -> Result<wl_buffer::WlBuffer, AbarError> {
    use std::io::Write;

    let shm_err = |source| AbarError::Io {
        path: std::path::PathBuf::from("/dev/shm"),
        source,
    };
    let size = i32::try_from(frame.data.len())
        .map_err(|_| AbarError::WaylandProtocol("buffer size overflow".into()))?;
    let mut file = tempfile::tempfile_in("/dev/shm").map_err(shm_err)?;
    file.write_all(&frame.data).map_err(shm_err)?;

    let pool = shm.create_pool(file.as_fd(), size, qh, ());
    let buffer = pool.create_buffer(
        0,
        frame.width as i32,
        frame.height as i32,
        frame.stride,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );
    pool.destroy();
    Ok(buffer)
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
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_compositor" => {
                    let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(
                        name,
                        5.min(version),
                        qh,
                        (),
                    );
                    state.compositor = Some(compositor);
                    state.create_bars_for_new_outputs(qh);
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
                    state.create_bars_for_new_outputs(qh);
                }
                "wl_seat" => {
                    let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, 8.min(version), qh, ());
                    state.seat = Some(seat);
                }
                "wl_output" => {
                    let output =
                        registry.bind::<wl_output::WlOutput, _, _>(name, 4.min(version), qh, ());
                    state.outputs.push(OutputInfo { name, output });
                    state.create_bars_for_new_outputs(qh);
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                state.remove_output(name);
            }
            _ => {}
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
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer.active_bar_output = state
                    .bars
                    .iter()
                    .find(|b| b.surface == surface)
                    .map(|b| b.output_name);
                state.pointer.on_submenu = state
                    .submenu
                    .as_ref()
                    .is_some_and(|sm| surface == sm.surface);
                if state.pointer.on_submenu {
                    state.pointer.submenu_x = surface_x;
                    state.pointer.submenu_y = surface_y;
                    state.update_submenu_hover(qh);
                } else if state.pointer.active_bar_output.is_some() {
                    state.pointer.x = surface_x;
                    state.pointer.y = surface_y;
                    state.update_hover(qh);
                }
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if let Some(output_name) = state
                    .bars
                    .iter()
                    .find(|b| b.surface == surface)
                    .map(|b| b.output_name)
                {
                    state.pointer.active_bar_output = None;
                    state.clear_interaction(output_name, qh);
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
                        if let Some(bar_idx) = state.active_bar_idx()
                            && let Some(shm) = state.shm.clone()
                        {
                            let (w, h) = (
                                state.bars[bar_idx].bar_width,
                                state.bars[bar_idx].bar_height,
                            );
                            if let Err(e) = state.resize_and_paint(bar_idx, &shm, qh, w, h) {
                                warn!(error = %e, "press repaint failed");
                            }
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
                    if let Some(bar_idx) = state.active_bar_idx()
                        && let Some(shm) = state.shm.clone()
                    {
                        let (w, h) = (
                            state.bars[bar_idx].bar_width,
                            state.bars[bar_idx].bar_height,
                        );
                        if let Err(e) = state.resize_and_paint(bar_idx, &shm, qh, w, h) {
                            warn!(error = %e, "release repaint failed");
                        }
                    }
                }
            }
            // Scroll events are collected per frame; one action runs per
            // notch (or notch-sized touchpad distance), not per event.
            wl_pointer::Event::Axis {
                axis: WEnum::Value(Axis::VerticalScroll),
                value,
                ..
            } => {
                state.pointer.scroll.axis(value);
            }
            wl_pointer::Event::AxisDiscrete {
                axis: WEnum::Value(Axis::VerticalScroll),
                discrete,
            } => {
                state.pointer.scroll.discrete(discrete);
            }
            wl_pointer::Event::AxisValue120 {
                axis: WEnum::Value(Axis::VerticalScroll),
                value120,
            } => {
                state.pointer.scroll.value120(value120);
            }
            wl_pointer::Event::AxisStop {
                axis: WEnum::Value(Axis::VerticalScroll),
                ..
            } => {
                state.pointer.scroll.stop();
            }
            wl_pointer::Event::Frame => {
                let steps = state.pointer.scroll.frame();
                let action = if steps < 0 {
                    PointerAction::ScrollUp
                } else {
                    PointerAction::ScrollDown
                };
                for _ in 0..steps.unsigned_abs() {
                    state.dispatch_pointer_action(action, qh);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, u32> for AppState {
    fn event(
        state: &mut Self,
        _layer_surface: &ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        data: &u32,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let output_name = *data;
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                let Some(bar_idx) = state.bars.iter().position(|b| b.output_name == output_name)
                else {
                    return;
                };
                if let Err(e) = state.on_configure(bar_idx, serial, width, height, qh) {
                    warn!(error = %e, "configure handling failed");
                    state.running = false;
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                debug!(output_name, "layer surface closed");
                if let Some(idx) = state.bars.iter().position(|b| b.output_name == output_name) {
                    let bar = state.bars.remove(idx);
                    bar.destroy();
                    if state.pointer.active_bar_output == Some(output_name) {
                        state.pointer.active_bar_output = None;
                        state.pointer.hovered = None;
                        state.pointer.pressed = None;
                    }
                }
                // Only stop the whole app once every bar has been closed (e.g. compositor
                // shutting down), not just because one monitor was unplugged.
                if state.bars.is_empty() {
                    state.running = false;
                }
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

impl Dispatch<wl_buffer::WlBuffer, ()> for AppState {
    fn event(
        state: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            // Every buffer is attached once; when the compositor is done
            // with it, nothing will use it again.
            for slot in state
                .bars
                .iter_mut()
                .map(|b| &mut b.buffer)
                .chain(state.submenu.iter_mut().map(|sm| &mut sm.buffer))
            {
                if slot.as_ref() == Some(buffer) {
                    *slot = None;
                }
            }
            buffer.destroy();
        }
    }
}
wayland_client::delegate_noop!(AppState: ignore ZwlrLayerShellV1);
wayland_client::delegate_noop!(AppState: ignore wl_output::WlOutput);

#[cfg(test)]
mod tests;
