//! A headless sway to run abar in, a virtual pointer to click it with, and
//! `grim` to read back what it drew.
//!
//! Each test starts its own sway in a private runtime directory, so tests run
//! in parallel and never touch the session they are started from. Without
//! `sway` or `grim` the tests are skipped, unless `ABAR_LIVE_TESTS=1` (as in
//! CI), where that is a failure.

#![allow(dead_code)]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use wayland_client::protocol::{wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, delegate_noop};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

pub const WIDTH: u32 = 1280;
pub const HEIGHT: u32 = 720;

/// Start a [`Sway`], or return from the test when the tools are missing.
#[macro_export]
macro_rules! sway_or_skip {
    () => {
        match common::Sway::start() {
            Some(sway) => sway,
            None => return,
        }
    };
}

/// Poll `f` until it returns `Some`, for up to 10 s.
pub fn wait_for<T>(what: &str, f: impl FnMut() -> Option<T>) -> T {
    try_wait_for(f).unwrap_or_else(|| panic!("timed out waiting for {what}"))
}

/// Poll `f` until it returns `Some`, or `None` after 10 s.
pub fn try_wait_for<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

pub struct Sway {
    child: Child,
    runtime: tempfile::TempDir,
    display: String,
    ipc: PathBuf,
}

impl Sway {
    pub fn start() -> Option<Self> {
        let required = std::env::var_os("ABAR_LIVE_TESTS").is_some_and(|v| v == "1");
        if !(have("sway") && have("grim")) {
            assert!(!required, "ABAR_LIVE_TESTS=1 but sway or grim is missing");
            eprintln!("skipped: sway and grim are needed for live tests");
            return None;
        }
        let runtime = tempfile::Builder::new()
            .prefix("abar-live")
            .tempdir()
            .unwrap();
        let conf = runtime.path().join("sway.conf");
        std::fs::write(
            &conf,
            format!(
                "output HEADLESS-1 resolution {WIDTH}x{HEIGHT} position 0 0 scale 1\n\
                 seat seat0 hide_cursor 0\n\
                 default_border none\n\
                 xwayland disable\n"
            ),
        )
        .unwrap();
        let log = std::fs::File::create(runtime.path().join("sway.log")).unwrap();
        let child = Command::new("sway")
            .arg("-c")
            .arg(&conf)
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DISPLAY")
            .env_remove("SWAYSOCK")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env_remove("NIRI_SOCKET")
            .env("XDG_RUNTIME_DIR", runtime.path())
            .env("WLR_BACKENDS", "headless")
            .env("WLR_RENDERER", "pixman")
            .env("WLR_LIBINPUT_NO_DEVICES", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("spawn sway");
        let mut sway = Self {
            child,
            display: String::new(),
            ipc: PathBuf::new(),
            runtime,
        };
        let dir = sway.runtime.path().to_owned();
        let (display, ipc) = wait_for("sway sockets", || {
            if let Ok(Some(status)) = sway.child.try_wait() {
                panic!("sway exited ({status}):\n{}", sway.log());
            }
            let names: Vec<String> = std::fs::read_dir(&dir)
                .ok()?
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            let display = names
                .iter()
                .find(|n| n.starts_with("wayland-") && !n.ends_with(".lock"))?;
            let ipc = names.iter().find(|n| n.starts_with("sway-ipc."))?;
            Some((display.clone(), dir.join(ipc)))
        });
        sway.display = display;
        sway.ipc = ipc;
        // Ready once IPC answers.
        wait_for("sway IPC", || sway.try_msg(&["-t", "get_outputs"]));
        Some(sway)
    }

    pub fn runtime_dir(&self) -> &Path {
        self.runtime.path()
    }

    pub fn log(&self) -> String {
        std::fs::read_to_string(self.runtime.path().join("sway.log")).unwrap_or_default()
    }

    fn try_msg(&self, args: &[&str]) -> Option<String> {
        self.msg_result(args).ok()
    }

    /// `swaymsg <args>`: its stdout, or what it and sway said on failure.
    fn msg_result(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new("swaymsg")
            .arg("-s")
            .arg(&self.ipc)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.success() {
            Ok(stdout)
        } else {
            Err(format!(
                "{}\n{stdout}{}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            ))
        }
    }

    /// The names of sway's outputs, in sway's order.
    pub fn outputs(&self) -> Vec<String> {
        // -p: the readable form, which swaymsg only prints to a terminal by default.
        self.msg(&["-p", "-t", "get_outputs"])
            .lines()
            .filter_map(|l| l.strip_prefix("Output "))
            .filter_map(|l| l.split_whitespace().next())
            .map(str::to_owned)
            .collect()
    }

    /// Create a headless output and return its name, which sway picks.
    pub fn create_output(&self) -> String {
        let before = self.outputs();
        self.msg(&["create_output"]);
        wait_for("the new output", || {
            self.outputs().into_iter().find(|o| !before.contains(o))
        })
    }

    /// Run a sway command (`swaymsg <args>`), panicking when it fails.
    pub fn msg(&self, args: &[&str]) -> String {
        self.msg_result(args)
            .unwrap_or_else(|e| panic!("swaymsg {args:?} failed: {e}"))
    }

    /// A command line set up to talk to this compositor.
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut cmd = Command::new(program);
        cmd.env_remove("DISPLAY")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env_remove("NIRI_SOCKET")
            .env("XDG_RUNTIME_DIR", self.runtime.path())
            .env("WAYLAND_DISPLAY", &self.display)
            .env("SWAYSOCK", &self.ipc);
        cmd
    }

    /// Capture a region of the layout (logical coordinates); pixels are
    /// physical, so a scale-2 output gives twice the width and height.
    pub fn screenshot(&self, x: u32, y: u32, w: u32, h: u32) -> Image {
        let out = self
            .command("grim")
            .args(["-g", &format!("{x},{y} {w}x{h}"), "-t", "ppm", "-"])
            .output()
            .expect("run grim");
        assert!(
            out.status.success(),
            "grim failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Image::from_ppm(&out.stdout)
    }

    pub fn pointer(&self) -> Pointer {
        Pointer::connect(self)
    }
}

impl Drop for Sway {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An RGB image read back with `grim`.
#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Image {
    fn from_ppm(data: &[u8]) -> Self {
        // P6\n<w> <h>\n<max>\n<data>, fields separated by any whitespace.
        let mut fields = Vec::new();
        let mut pos = 0;
        while fields.len() < 4 {
            while data[pos].is_ascii_whitespace() {
                pos += 1;
            }
            let start = pos;
            while !data[pos].is_ascii_whitespace() {
                pos += 1;
            }
            fields.push(std::str::from_utf8(&data[start..pos]).unwrap().to_owned());
        }
        assert_eq!(fields[0], "P6");
        let width = fields[1].parse().unwrap();
        let height = fields[2].parse().unwrap();
        Self {
            width,
            height,
            rgb: data[pos + 1..].to_vec(),
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    /// Number of pixels of exactly `color`.
    pub fn count(&self, color: [u8; 3]) -> usize {
        self.rgb
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|p| **p == color)
            .count()
    }

    /// Rows (from the top) in which some pixel differs from `background`.
    pub fn rows_not(&self, background: [u8; 3]) -> usize {
        (0..self.height)
            .filter(|&y| (0..self.width).any(|x| self.pixel(x, y) != background))
            .count()
    }
}

#[derive(Default)]
struct PointerState {
    manager: Option<ZwlrVirtualPointerManagerV1>,
    seat: Option<wl_seat::WlSeat>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for PointerState {
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
                "zwlr_virtual_pointer_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(2), qh, ()));
                }
                "wl_seat" => state.seat = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}
delegate_noop!(PointerState: ignore ZwlrVirtualPointerManagerV1);
delegate_noop!(PointerState: ignore ZwlrVirtualPointerV1);
delegate_noop!(PointerState: ignore wl_seat::WlSeat);

pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;
pub const BTN_MIDDLE: u32 = 0x112;

/// A `zwlr_virtual_pointer_v1` on the compositor's seat.
pub struct Pointer {
    conn: Connection,
    queue: EventQueue<PointerState>,
    state: PointerState,
    pointer: ZwlrVirtualPointerV1,
}

impl Pointer {
    fn connect(sway: &Sway) -> Self {
        let socket = sway.runtime.path().join(&sway.display);
        let stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
        let conn = Connection::from_socket(stream).unwrap();
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut state = PointerState::default();
        queue.roundtrip(&mut state).unwrap();
        let pointer = state
            .manager
            .as_ref()
            .expect("compositor has no zwlr_virtual_pointer_manager_v1")
            .create_virtual_pointer(state.seat.as_ref(), &qh, ());
        let mut this = Self {
            conn,
            queue,
            state,
            pointer,
        };
        this.sync();
        this
    }

    fn sync(&mut self) {
        self.conn.flush().unwrap();
        self.queue.roundtrip(&mut self.state).unwrap();
    }

    fn time() -> u32 {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        (ms & 0xffff_ffff) as u32
    }

    /// Move to `(x, y)` in the layout (logical pixels).
    pub fn move_to(&mut self, x: u32, y: u32) {
        self.pointer
            .motion_absolute(Self::time(), x, y, WIDTH, HEIGHT);
        self.pointer.frame();
        self.sync();
    }

    pub fn click(&mut self, button: u32) {
        for state in [
            wl_pointer::ButtonState::Pressed,
            wl_pointer::ButtonState::Released,
        ] {
            self.pointer.button(Self::time(), button, state);
            self.pointer.frame();
            self.sync();
        }
    }

    /// `notches` wheel clicks: positive scrolls down, negative up.
    pub fn wheel(&mut self, notches: i32) {
        self.pointer.axis_source(wl_pointer::AxisSource::Wheel);
        self.pointer.axis_discrete(
            Self::time(),
            wl_pointer::Axis::VerticalScroll,
            15.0 * f64::from(notches),
            notches,
        );
        self.pointer.frame();
        self.sync();
    }
}

/// An abar process on a [`Sway`], with its config, theme and log in a
/// temporary directory.
pub struct Abar {
    child: Child,
    pub dir: tempfile::TempDir,
}

/// The test theme's colors: a plain bar background and island background.
pub const BAR_BG: [u8; 3] = [0, 0, 0];
pub const ISLAND_BG: [u8; 3] = [0x20, 0x40, 0x60];
pub const FOREGROUND: [u8; 3] = [0xff, 0xff, 0xff];
pub const HOVER_BG: [u8; 3] = [0x60, 0x40, 0x20];

pub const THEME: &str = r##"
[base]
background_color = "#204060FF"
foreground_color = "#FFFFFFFF"
hover_background_color = "#604020FF"
active_background_color = "#604020FF"
island_radius = 0
"##;

impl Abar {
    /// Start abar with `config` (`{dir}` is replaced by its directory) and
    /// [`THEME`], and wait until it draws.
    pub fn start(sway: &Sway, config: &str) -> Self {
        Self::start_with(sway, config, &[])
    }

    pub fn start_with(sway: &Sway, config: &str, env: &[(&str, &str)]) -> Self {
        let dir = tempfile::Builder::new()
            .prefix("abar-cfg")
            .tempdir()
            .unwrap();
        let config = config.replace("{dir}", dir.path().to_str().unwrap());
        std::fs::write(dir.path().join("config.toml"), config).unwrap();
        std::fs::write(dir.path().join("theme.toml"), THEME).unwrap();
        let log = std::fs::File::create(dir.path().join("abar.log")).unwrap();
        let child = sway
            .command(env!("CARGO_BIN_EXE_abar"))
            .arg("--config")
            .arg(dir.path().join("config.toml"))
            .arg("--theme")
            .arg(dir.path().join("theme.toml"))
            .env("RUST_LOG", "libabar=debug,abar=debug")
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("spawn abar");
        let mut abar = Self { child, dir };
        abar.wait_for_log("first paint", "bar painted");
        abar
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn log(&self) -> String {
        std::fs::read_to_string(self.path("abar.log")).unwrap_or_default()
    }

    /// Wait until the log contains `needle` `n` times or more.
    pub fn wait_for_log_count(&mut self, what: &str, needle: &str, n: usize) {
        let found = try_wait_for(|| {
            if let Ok(Some(status)) = self.child.try_wait() {
                panic!("abar exited ({status}):\n{}", self.log());
            }
            (self.log().matches(needle).count() >= n).then_some(())
        });
        if found.is_none() {
            panic!("timed out waiting for {what}; abar's log:\n{}", self.log());
        }
    }

    pub fn wait_for_log(&mut self, what: &str, needle: &str) {
        self.wait_for_log_count(what, needle, 1);
    }

    /// Wait until the file `name` in the config directory contains `text`.
    pub fn wait_for_file(&self, name: &str, text: &str) -> String {
        try_wait_for(|| {
            let mut s = String::new();
            std::fs::File::open(self.path(name))
                .ok()?
                .read_to_string(&mut s)
                .ok()?;
            s.contains(text).then_some(s)
        })
        .unwrap_or_else(|| {
            panic!(
                "timed out waiting for {text:?} in {name}; abar's log:\n{}",
                self.log()
            )
        })
    }

    /// A virtual pointer, once abar has bound the seat's pointer (it does so
    /// when the device appears; input sent before that is lost).
    pub fn pointer(&mut self, sway: &Sway) -> Pointer {
        let pointer = sway.pointer();
        self.wait_for_log("pointer bound", "pointer bound");
        pointer
    }

    /// The bar's height in logical pixels, from its last paint.
    pub fn bar_height(&self) -> u32 {
        let log = self.log();
        let line = log
            .lines()
            .rfind(|l| l.contains("bar painted"))
            .expect("painted");
        let field = line
            .split_whitespace()
            .find_map(|w| w.strip_prefix("height="))
            .expect("height field");
        field.parse().unwrap()
    }

    pub fn try_wait(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().unwrap()
    }

    /// Send `SIGTERM` and wait for abar to exit.
    pub fn terminate(&mut self) -> ExitStatus {
        let pid = rustix::process::Pid::from_child(&self.child);
        rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
        wait_for("abar to exit", || self.try_wait())
    }
}

impl Drop for Abar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
