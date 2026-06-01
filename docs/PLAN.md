# abar — Rust architecture + implementation plan

This document is the **human roadmap** and **agent playbook** for **abar**: a minimal Wayland-native status bar (ashell / waybar inspired) using **Cairo + Pango** for drawing (same stack direction as [tofi-rs](https://github.com/philj56/tofi-rs) `libtofi`), **no** heavyweight UI toolkits (e.g. iced), and **no** in-bar dialogs or menus — user-visible actions are **spawn external commands** (shell runner) or **non-GUI** side effects only.

It mirrors the **execution discipline** of `wau/docs/WAU_RS_PLAN.md`:

- Library-first crate split, small verifiable phases, strict quality gates (fmt, clippy `-D warnings` with feature matrix, tests, `cargo doc`, `typos`, `cargo deny`).
- **Directory modules** with **sibling `tests.rs`** — tests never live in the same file as logic (same rule as WAU §2.0).
- **Per-integration Cargo features** so minimal installs and CI do not bitrot optional code paths.

**Reference configs (source of truth for schemas):**

- `examples/config.toml` — layout, modules, events as command strings, keyboard/clock options.
- `examples/theme.toml` — RGBA colors, module-specific theme keys (ashell-like palette).
- `examples/ashell_config.toml` — behavioral reference only; many keys are **intentionally dropped** (see §1.3).

---

## 1. Goals and constraints

### 1.1 Goals

- **Minimal surface area**: smallest useful bar; every optional capability behind **compile-time** `features` where practical.
- **Islands UI**: visually separated “islands” per module (or per **nested group** in `layout`, see `examples/config.toml` `right = [ [ "keyboard" ], [ "tray", "clock", ... ] ]`) — rounded rects, padding, gap; overall look aligned with **ashell** defaults from `examples/theme.toml` / `ashell_config.toml` **appearance** (not feature parity).
- **Wayland-native**: `zwlr_layer_shell_v1` (or equivalent stable path if chosen later), correct anchor/exclusive zone, fractional scale / buffer scale where supported.
- **Cairo + Pango**: measure and paint text/icons on an **image buffer** (shm or equivalent) and attach to the layer surface — same family of deps as `tofi-rs/libtofi` (`cairo-rs`, `pango`, `pangocairo`); keep gtk-rs stack versions aligned within one minor.
- **Event model**: pointer buttons and scroll invoke **optional** user-defined commands from config (`on_left_click`, …). Built-in toggles (e.g. keyboard layout, clock format) are **state changes inside the bar**, still **without** opening custom GUI overlays; if a user overrides with `on_*` commands, those win.
- **Config discovery**: XDG-style resolution (e.g. `$XDG_CONFIG_HOME/abar/config.toml`, theme under `.../abar/themes/` as in comments in `examples/config.toml`), plus `--config` override on the binary.

### 1.2 Discipline (non-negotiable, from WAU)

- **Library-first**: **`libabar`** — layout math, module trait/state, spawn helpers, Wayland protocol glue that is testable without config file formats; **`abar`** — `main` (tracing, CLI, read config/theme TOML, run loop).
- **`abar` contains no domain logic** beyond wiring; **`libabar` does not depend on clap** or **toml** and does not assume a specific logger implementation beyond `tracing`.
- **Tokio for async work**: use **[`tokio`](https://crates.io/crates/tokio)** as the standard runtime for background tasks — config event commands (`tokio::process`), long-running `exec` scripts (stdout line reader per module), and timers. The Wayland client loop stays **synchronous** on the main thread (`blocking_dispatch`); never block it on subprocess or socket I/O — offload with `tokio::spawn` (and `spawn_blocking` only when a sync API is unavoidable).
- **Step sizing**: small PR-sized phases with explicit **Verify** blocks.
- **Feature matrix in CI**: default, `--all-features`, `--no-default-features` (core must still build: e.g. bar shell + clock-only or stub modules — define explicitly in Phase 0). **Tray** is part of the first shippable bar; **MPRIS** is explicitly **not** in that milestone (see §8 post-first-release).
- **Naming**: short, descriptive; prefer clarity over abstraction depth.
- **Code comments**: describe current behavior only (invariants, protocol steps, non-obvious effects). No roadmap phase labels, session/chat context, prompts, or long rationale unrelated to reading the code.

### 1.3 Non-goals / dropped ashell concepts

- **No** first-party popovers, settings panels, Wi-Fi/BT/audio “applets”, calendar UI, or **any** in-process menu widgets.
- **No** iced / winit / egui / Qt / GTK windowing for the bar.
- **No** D-Bus, compositor IPC, or media IPC libraries inside abar: `zbus`, `dbus`, `hyprland-rs`, `niri-ipc`, `libxkbcommon`, and equivalents are **banned** from `libabar` and `abar`. All such state (workspaces, active window, keyboard layout, tray items, media info) is pushed by **external daemons or scripts** over the abar IPC socket (see §5.2–5.4, §8 Phase 7–8).
- **Tempo**, **Privacy**, embedded **Settings** modules, **weather in clock strip**, etc. from `ashell_config.toml` — **out of scope** unless reintroduced later as **optional** features with **external** commands only.
- **No** promise of pixel-perfect ashell clone; target is **similar** islands + colors + font from examples.

### 1.4 Definitions

- **Module**: a unit that contributes one island (or shares an island in a **group**). Either **built-in** (behind its own feature) or **custom** (icon + events only).
- **Island**: one rounded background region containing one or more module **segments** (text/icon) with inner spacing.
- **Layout row**: three logical regions `left` / `center` / `right`, each a list of **entries**; an entry is either a module name or a **nested array** expressing one island with multiple modules (see `examples/config.toml`).

---

## 2. Repository layout (target)

```text
abar/                          # workspace root (already exists)
  Cargo.toml                   # workspace members: libabar, abar
  Cargo.lock                   # committed
  deny.toml
  examples/
    config.toml
    theme.toml
    ashell_config.toml         # reference only
  libabar/
    Cargo.toml                 # features defined here + re-export policy
    src/
      lib.rs
      error.rs                 # thiserror (Wayland / SHM only; no config/theme)
      layout/                  # (future) resolve layout → ordered islands + alignment
      icon/                    # freedesktop-icon-name lookup + PNG (optional SVG) → Cairo
      render/                  # cairo+pango: measure, draw islands, damage regions
      wayland/                 # compositor connection, layer_shell, outputs, input
      modules/                 # (future) one subdirectory per built-in module
        clock/
          mod.rs
          tests.rs
        keyboard/
          mod.rs
          tests.rs
        # ... workspaces, window, tray — compositor + tray behind features; MPRIS deferred (§8)
      spawn/                   # Tokio runtime + sh -c command execution (logging failures)
      model/                   # (future) shared small types (ids, colors, keys)
  abar/
    Cargo.toml                 # clap, toml, tracing-subscriber, libabar features passthrough
    src/
      main.rs                  # minimal
      error.rs                 # config/theme/file validation errors (thiserror)
      config/                  # TOML parse + read files / resolve theme path
        mod.rs
        tests.rs
      cli/
        mod.rs
        tests.rs
      settings/
        mod.rs                 # merged view: cli > env > config (later)
        tests.rs
  scripts/                       # example exec scripts (reference implementations)
    keyboard.sh                # Hyprland keyboard layout → {"text": "…"}
    # workspaces.sh, window.sh — to be added in Phase 8
  docs/
    PLAN.md                    # this file
    EXEC.md                    # exec JSON model + script contract (Phase 7)
  .github/workflows/           # already scaffolded — extend as in §7
```

**Crate boundary rules**

- `libabar` has **no** `clap`, **no** `toml`, **no** config/theme file parsers; **no** `println!` in library code (use `tracing`).
- After `Settings` (or `RuntimeConfig`) is built in `abar`, only that merged struct crosses into the run loop — avoid threading raw `clap` types through `libabar`.

**Optional:** `abar` features are **thin passthroughs** to `libabar` features (WAU §2.2 pattern) so packagers can `cargo install abar --no-default-features --features "clock,keyboard,hyprland,tray"`.

---

## 3. Data model and config

### 3.1 `config.toml` (see `examples/config.toml`)

**Intent**

- **`[base]`**: `font` (required), `theme` filename or path relative to themes dir.
- **`[layout]`**: `left` / `center` / `right` lists; nested arrays = single island, inner order = left-to-right segments.
- **Per-module tables** (e.g. `[keyboard]`, `[clock]`) for module-specific options; **global event tables** live on each module definition — custom modules under `[modules].custom` (array of `{ name, icon, on_* }`), for built-ins merge: defaults < `[clock]` etc.
- **Events**: string commands executed via shell (`sh -c` through `tokio::process` on the shared runtime); scroll/button names as in example.

**Invariants**

- Unknown keys: ignored by serde unless we add explicit handling later.
- Missing `font` in base: TOML deserialize error if `[base]` / `font` absent.

### 3.2 `theme.toml` (see `examples/theme.toml`)

**Intent**

- Global `background_color`, `foreground_color` (RGBA hex, alpha in color); optional per-module sections (e.g. `[workspaces]` colors, `visibility_mode`).
- **`scale_factor`**: deferred (TODO in example) — Phase 2 can hardcode `1.0` + env-based fractional scale from Wayland only.

### 3.3 Mapping from `ashell_config.toml`

- **Appearance** keys map into `theme.toml`; **layout** naming differs — any future migration helpers (e.g. ashell module name → abar id) are **optional** and not required for the first release.

---

## 4. Rendering and UI

### 4.1 Cairo + Pango pipeline

- Build layout: for each island, compute **width/height** from max of children measurements + padding + corner radius.
- Draw to **ARGB32** (or premultiplied, decide once and test on multiple compositors) image surface; upload to `wl_buffer`.
- **Icons**: **freedesktop-icon-name** resolution (XDG icon theme paths) → PNG into Cairo (**Phase 4**; required for **custom** modules to be visible). SVG via optional **`svg`** feature + `resvg` later. Built-ins may stay text-only until their module phase adds icons where needed.
- **Text**: Pango layout with font description from `[base].font`; ellipsis rules for long window titles (module `window`).

### 4.2 Islands geometry

- Outer bar: transparent or solid strip (theme); each island: rounded rect fill + optional border; inner gap between segments inside a grouped island.
- **Spacing** between islands: theme key or sane default (e.g. 8 px).

### 4.3 Damage / redraw

- Full redraw acceptable for **v0**; optimize to **damage rectangles** per-island when clock seconds are enabled vs not.

---

## 5. Wayland and compositor policy

### 5.1 Core protocols (everyone)

- `wayland-client`, `wayland-protocols` (staging as needed), `wayland-protocols-wlr` for `wlr-layer-shell-unstable-v1` and related.
- Seat: pointer **required** for interactions; keyboard **not** required if all input is pointer-based.

### 5.2 Compositor-specific **feature** modules

**Architecture decision (issue #8):** abar does **not** depend on any compositor IPC library. Compositor state is delivered by **user-provided scripts** configured via an `exec` field on each module. abar spawns the script as a long-running child process and reads newline-delimited JSON from its stdout; the script is the thick layer that handles all compositor-specific logic (e.g. `hyprctl`, `socat`, Hyprland event socket). See `scripts/keyboard.sh` for a reference implementation.

- `workspaces`, `window`, `keyboard` are **exec-handler modules**: they hold state and render it; they have no knowledge of how the data arrives.
- The script decides what compositor to talk to, what events to subscribe to, and how to map raw compositor data to the abar JSON model — abar just reads lines.
- **No** `hyprland`, `xkb`, or compositor-named Cargo features remain in `libabar` after Phase 8 refactoring.

| What was            | What it becomes after Phase 8                                                 |
| ------------------- | ----------------------------------------------------------------------------- |
| `hyprland` feature  | **Removed**; logic moves into a user script (e.g. `scripts/keyboard.sh`)      |
| `xkb` feature       | **Removed**; keyboard layout read from script stdout                          |
| `workspaces` module | exec-handler: pure state sink + render; `exec` field in config drives updates |
| `window` module     | exec-handler: pure state sink + render; `exec` field in config drives updates |
| `keyboard` module   | exec-handler: pure state sink + render; `exec` field in config drives updates |

### 5.3 Tray (**must-have** for first shippable milestone)

- **Required** for the first working release. No D-Bus / `zbus` inside abar. Architecture for how tray data reaches abar is **TBD** (under design — see Phase 9).

### 5.4 MPRIS

- Implemented as an **exec-handler module**: a user script (e.g. wrapping **`playerctl`**) outputs JSON lines to stdout; abar reads and renders. No `zbus`, no `libdbus`, no D-Bus code inside abar.
- Keep behind a dedicated **`mpris`** feature; deferred to post-first-release (see §8).

---

## 6. Module catalog (compile-time)

Modules are split into three tiers (issue #8):

**Tier 1 — Built-in**: self-contained logic inside abar, no external daemon needed.

| Module  | Scope                                               | Notes           |
| ------- | --------------------------------------------------- | --------------- |
| `clock` | timezones + format cycle + optional `on_left_click` | no GUI calendar |

**Tier 2 — Custom**: user-defined icon + fire-and-forget click actions. No daemon; config-only.

| Module   | Scope         | Notes                                                                              |
| -------- | ------------- | ---------------------------------------------------------------------------------- |
| `custom` | icon + events | **icon paint in Phase 4** — config `icon` parsed today but not shown until Phase 4 |

**Tier 3 — exec-handler modules**: abar spawns a user-configured script (`exec` field in config) as a long-running child process and reads newline-delimited JSON from its stdout. The script owns all compositor/IPC/D-Bus logic; abar owns only the JSON model + rendering. No compositor IPC libs inside abar.

| Module       | Scope after Phase 8 refactor                                              | Notes                                                 |
| ------------ | ------------------------------------------------------------------------- | ----------------------------------------------------- |
| `keyboard`   | display layout label; reads `{"text": "…"}` lines from `exec` script      | replaces `hyprland` event socket + `xkb` feature      |
| `workspaces` | display workspace list; reads JSON lines from `exec` script               | replaces `hyprland` feature; monitor filter via theme |
| `window`     | display active title (ellipsis); reads `{"text": "…"}` from `exec` script | replaces `hyprland` active-window handler             |
| `tray`       | **TBD** — architecture under design                                       | Phase 9; no D-Bus/zbus in abar                        |
| `mpris`      | **post-first-release** — reads JSON from a `playerctl`-based script       | no D-Bus/zbus in abar                                 |

**Custom modules**: unique name, **icon name** required (FreeDesktop). After Phase 4: missing icon at startup → **structured error** in `abar`.

---

## 7. Quality gates (mirror WAU §7)

Whenever a phase is marked complete:

- `cargo fmt --check`
- `typos`
- `cargo deny check licenses` (ensure `deny.toml` **allow** list populated before enforcing in CI)
- `cargo clippy --workspace --all-targets --no-default-features -- -D warnings`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --no-default-features`
- `cargo test --workspace --all-features`
- `cargo doc --workspace --no-deps`

### 7.1 Test discipline

- Unit tests in **`tests.rs`** next to `mod.rs` per directory module.
- Integration tests under `abar` (config/theme TOML) and `libabar/tests/` for: layout expansion (nested arrays → islands), render fixtures, etc.

### 7.2 CI

Existing workflows (`build`, `fmt-clippy`, `test`, `doc`, `typos`, `deny`) should be kept and updated once crates exist; matrix already includes feature variants — ensure **doc** and **test** jobs use the same matrices as WAU recommends.

---

## 8. Phased steps

### Phase 0 — Workspace + hygiene + empty vertical slice

- [x] Fix root `Cargo.toml` **workspace members**: `["libabar", "abar"]` (add `libabar` crate).
- [x] Implement minimal `libabar` (Wayland strip) + `abar`: **`abar`** parses **minimal** `config.toml` (only `[base]` + empty layout) and **theme.toml**; exit with structured error if font missing; **`libabar`** receives plain values (e.g. SHM pixel bytes) only.
- [x] Wire **tracing** + `tracing-subscriber` in `abar` only.
- [x] Populate **`deny.toml` licenses allow list** for used crates.
- [x] Hello Wayland: connect, bind globals, create **layer surface** strip (solid color from theme), no text yet.

**Verify**: all gates in §7; manual run on **Hyprland** (or another compositor you explicitly add later).

### Phase 1 — Config + theme + layout model

- [x] Full serde models matching `examples/config.toml` / `theme.toml` (including nested layout arrays, `[modules].custom`, event strings).
- [x] XDG path resolution + `--config` / `--theme` flags (`abar` / `clap`).
- [x] No runtime validation layer (parse only); feature gates remain compile-time via Cargo features.

**Verify**: unit tests for parse/deserialize of `examples/*.toml`.

### Phase 2 — Render core (Cairo + Pango)

- [x] Font loading, Pango measurement helpers, Cairo rounded-rect helper.
- [x] Island layout pass: compute bar height from font metrics + padding; horizontal distribution for `left`/`center`/`right` (center cluster truly centered).
- [x] Draw static placeholder **text** per module entry (“clock”, “kb”, …) before real module data.

**Verify**: headless tests where possible (image buffer pixel samples); optional `insta` PNG snapshots gated behind feature.

### Phase 3 — Pointer input + spawn

- [x] Wayland pointer events → hit-test which island/segment.
- [x] Map to configured command; execute without blocking the Wayland thread via **Tokio** (`tokio::spawn` + `tokio::process::Command` with `sh -c`; log failures).

**Verify**: integration test with mock command (script that touches tempfile).

### Phase 4 — FreeDesktop icons + **custom** modules (visible)

**Prerequisite for custom modules:** config already requires `icon` per `[modules].custom` entry, but the bar only draws placeholder **text** until this phase. Pointer events (Phase 3) can target custom segments by name; users still need **icons** to recognize them.

- [x] **`libabar/src/icon/`**: resolve **freedesktop-icon-name** via XDG icon theme (`hicolor`, user themes, `XDG_DATA_DIRS`); load **PNG** into a Cairo image source; cache decoded pixmaps per name/size where useful.
- [x] Optional Cargo feature **`svg`** + `resvg` for SVG assets (later polish; PNG-only is acceptable for first icon milestone).
- [x] Extend **`Segment`** / layout / paint: carry `icon_name` (and display mode — **icon-only** for custom modules, text and/or icon for built-ins later); measure segment size from icon dimensions (scale with `[base].font_size`, e.g. 1× em box).
- [x] **`abar` `Settings`**: wire each layout custom module to its config `icon`; **fail startup** with a clear error if the icon cannot be resolved (per `examples/config.toml` comment).
- [x] Paint icons centered in segment rects; reuse the same decode/blit helpers for **tray** item pixmaps in Phase 7.
- [x] Respect `XDG_ICON_THEME` / common theme name when present (document behavior).

**Verify**: unit tests with a **fixture icon theme** directory (resolve name → file, load PNG); headless render test (non-transparent pixels in icon bbox); manual run with `examples/config.toml` — `system_info`, `audio`, `network`, etc. show as icons, not strings.

### Phase 5 — `clock` + `keyboard` modules

- [x] `clock`: formats + timezones rotation; tick per minute only (no per-second updates); optional overrides.
- [x] `keyboard`: display current active layout name; **no built-in switching logic** — user wires layout switching via `on_left_click` / `on_right_click` in config (e.g. `hyprctl switchxkblayout all next`).
  - **`hyprland` feature**: subscribe to Hyprland event socket (`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`) via Tokio background task; parse `activelayout>>keyboard,layoutname` lines → update label in real-time, no polling.
  - **`xkb` feature** (no `hyprland`): compositor-agnostic path via `wl_keyboard` seat + **libxkbcommon** state tracking.
  - **Neither**: segment shows static initial label from `[keyboard].layouts[0]` in config.

**Verify**: unit tests for format rotation logic (no Wayland).

### Phase 6 — Compositor modules (`workspaces`, `window`)

- [x] **`workspaces`**: behind **`hyprland`** feature; Hyprland IPC via `AsyncEventListener` (workspace changed/added/deleted/moved); Pango markup for `active_color` / `inactive_color` from theme; `visibility_mode = "monitor_specific"` filters to active monitor's workspaces; compositor-agnostic `format_label` + `WorkspacesConfig` in `libabar`; `use_markup` flag added to `Segment` / `PlacedSegment` for color-differentiated workspace rendering.
- [x] **`window`**: active title, ellipsis, compositor feature; `WindowConfig { max_length }` in `libabar`; Hyprland `AsyncEventListener` (`add_active_window_changed_handler`) + `Client::get_active_async()` for initial title; `truncate_title` helper with Unicode scalar-value counting; optional `max_length` in `[window]` config (default 50, 0 = no limit).

**Verify**: manual on Hyprland; mocked JSON/socket tests where feasible.

### Phase 7 — exec-handler infrastructure + JSON models

> **Motivation (issue #8):** remove all compositor IPC libs from abar. Each exec-handler module spawns a user script and reads newline-delimited JSON from its stdout. This phase builds the generic machinery before Phase 8 swaps out the module internals.

- [x] **JSON model**: define a shared `ModuleUpdate` type (in `libabar/src/model/` or per-module) — minimum: `{ "text": String }`. Optional fields (to be extended per module as needed): `"icon": String` (FreeDesktop name or path), `"markup": bool` (treat `text` as Pango markup). Serialize/deserialize via `serde_json`; unknown fields ignored.
- [x] **`libabar/src/exec/`**: Tokio task that spawns a configured command (`sh -c <exec_string>`) as a child process, reads lines from stdout, deserializes each as `ModuleUpdate`, and sends it over a `tokio::sync::watch` or `mpsc` channel to the module state. Restarts the script on unexpected exit (with backoff + tracing warning). Forwards stdin writes from abar for back-channel signals (reserved for future use; not required now).
- [x] **Module trait**: gains `fn apply_update(&mut self, update: ModuleUpdate)` — exec-handler modules implement this to update their held state; built-in modules (`clock`) do not need it.
- [x] **Config**: each exec-handler module config gains an `exec: Option<String>` field; if absent the module renders a static placeholder.
- [x] Add `scripts/` dir to repo with `keyboard.sh` as the reference implementation; document the JSON model and exec contract in `docs/EXEC.md`.

**Verify**: unit test — spawn a trivial script that emits `{"text": "hello"}` and exits; assert `ModuleUpdate` is received on the channel; no Wayland required.

### Phase 8 — Refactor compositor modules + keyboard to exec-handlers

> **Removes** all direct compositor IPC from abar (`hyprland-rs`, `niri-ipc`, `libxkbcommon`). After this phase, `libabar` has no compositor-named features; all three modules read state from their `exec` script via the Phase 7 infrastructure.

- [x] **`keyboard`**: delete `hyprland` event-socket path and `xkb` feature path; module holds `current_layout: String`, updated via `apply_update` from exec script stdout; static placeholder if `exec` is absent.
- [x] **`workspaces`**: delete `hyprland` feature wiring (`AsyncEventListener`, `hyprland-rs` dep); module receives `ModuleUpdate` from exec script; `visibility_mode` and Pango markup rendering stay (script is responsible for emitting pre-formatted markup in `text` with `"markup": true`).
- [x] **`window`**: delete Hyprland `add_active_window_changed_handler` + `Client::get_active_async()`; module receives `ModuleUpdate` from exec script; `truncate_title` + `max_length` stay unchanged (applied after receiving `text`).
- [x] Remove `hyprland` and `xkb` features from `libabar/Cargo.toml`; update `abar/Cargo.toml` passthroughs; scrub feature matrix in CI.
- [x] `hyprland-rs`, `niri-ipc`, `libxkbcommon` must not appear in `Cargo.lock`.
- [x] Update `examples/config.toml` with `exec` field examples for `keyboard`, `workspaces`, `window`.
- [ ] `mpris` feature implemented in the same manner

**Verify**: `cargo build --no-default-features` and `--all-features` both succeed; `Cargo.lock` contains neither `hyprland-rs` nor `libxkbcommon`; existing layout/render tests still pass; manual: modules show placeholder when `exec` is absent, live data when `keyboard.sh` runs.

### Phase 9 — **Tray** (must-have)

> `trayd` is a persistent Wayland SNI daemon that exposes a Unix-socket NDJSON IPC (`$XDG_RUNTIME_DIR/trayd.sock`). abar connects to it as a pure IPC client — no `zbus`, no `libdbus` inside abar. Menu interaction is fully delegated to `trayctl menu --app-id <id>` (a one-shot external binary). Protocol details: `trayd/docs/IPC.md`; integration flow: `trayd/examples/abar.md`.

Behind a **`tray`** Cargo feature in `libabar` and `abar`.

- [x] **Wire types** (`libabar/src/modules/tray/ipc.rs`): define serde-backed structs that mirror the trayd NDJSON protocol — `MinimalTrayItem { app_id, title?, status, icon_handle? }`, `TrayEvent { kind, items }` (only `"update"` kind needed), `TrayRequest` (tagged `cmd` enum: `ping`, `subscribe`, `get_pixmap`), `TrayResponse` (tagged `type` enum: `pong`, `event`, `pixmap`, `error`). Unknown fields ignored via `#[serde(other)]` / `deny_unknown_fields = false`. No `zbus` or D-Bus types.
- [ ] **IPC client Tokio task** (`libabar/src/modules/tray/client.rs`): on startup, attempt to open a `tokio::net::UnixStream` to `socket_path` (default `$XDG_RUNTIME_DIR/trayd.sock`); if the connect fails, log a `tracing::warn!` and leave the tray empty — do **not** auto-start `trayd` (that is systemd / WM / user responsibility). On a successful connection, immediately send `subscribe` and loop reading NDJSON lines: deserialize each as `TrayResponse`, extract `TrayEvent::update` items, and send `Vec<MinimalTrayItem>` over a `tokio::sync::watch` channel consumed by the tray module. On EOF or error, reconnect with exponential back-off (start 1 s, cap 30 s) and `tracing::warn!`. No `ping` step — a successful connect + first `event` response is sufficient proof that the daemon is alive.
- [ ] **Pixmap fetching** (`libabar/src/modules/tray/pixmap.rs`): expose an async function `fetch_pixmap(socket_path, app_id, size) -> Result<Vec<u8>>` that opens a **separate** short-lived `UnixStream`, sends `get_pixmap{app_id, size}`, reads one response line, base64-decodes the `data` field, and returns the raw ARGB32 bytes (`width × height × 4`, big-endian per SNI spec). Maintain an in-memory `HashMap<(AppId, u32), cairo::ImageSurface>` cache inside the tray module; invalidate entries for items missing from the new `update` event. Use the existing Phase 4 FreeDesktop icon path (`icon_handle`) as a fallback when `get_pixmap` returns an error or empty data.
- [ ] **`TrayConfig`** (`libabar/src/modules/tray/mod.rs` or `libabar` public API): `TrayConfig { socket_path: Option<String>, icon_size: u32, dmenu_cmd: Option<String> }` — runtime config passed into the tray module. `socket_path` defaults to `$XDG_RUNTIME_DIR/trayd.sock`; `icon_size` defaults to match the bar's icon pixel size (e.g. 22); `dmenu_cmd` is forwarded to `trayctl` as `--dmenu-cmd` if set.
- [ ] **Tray module** (`libabar/src/modules/tray/`): state holds `Vec<MinimalTrayItem>` and the pixmap cache. `apply_update(items)` updates state and drops stale cache entries. `view()` renders one icon **segment** per item: pixmap (from cache) preferred, FreeDesktop icon fallback via `icon_handle`, nothing if both are absent. On left-click, spawn `trayctl menu --app-id <app_id>` (append `--dmenu-cmd <cmd>` if configured, `--socket <path>` if non-default) using the existing Phase 3 / Phase 7 `spawn` infrastructure. Status `"Passive"` items are skipped (not rendered).
- [ ] **Config** (`abar/src/config/modules.rs`): add `Tray { socket_path: Option<String>, icon_size: Option<u32>, dmenu_cmd: Option<String>, #[serde(flatten)] events: Option<Events> }` deserializing from `[tray]` in `config.toml`; update `examples/config.toml` with a commented `[tray]` entry.
- [ ] **Settings wiring** (`abar/src/settings/mod.rs`): map `config::Tray` → `libabar::TrayConfig`; pass into the run loop alongside other module configs; conditionally compile behind `#[cfg(feature = "tray")]`.
- [ ] **Layout + render integration**: the tray module slots into the existing island / segment system as an icon-bearing module (same `Segment` / `PlacedSegment` path established in Phase 4); pixmap surfaces use the same Cairo blit helper as FreeDesktop icons; `use_markup = false` (no Pango markup needed). Multiple tray items share one island if grouped in layout, or each occupies its own segment according to `layout` config.

**Verify**: unit test — mock NDJSON server emitting a canned `subscribe` snapshot + one update; assert `Vec<MinimalTrayItem>` is received and `apply_update` correctly drops stale cache entries; no Wayland required. Integration test — `get_pixmap` decode: feed a known base64 ARGB32 blob and assert `ImageSurface` dimensions match. Manual: start `trayd`, confirm tray icons appear in the bar; click an item and confirm `trayctl` launches the dmenu picker; stop `trayd` and confirm the bar continues running with an empty tray.

### Phase 10 — Polish + first release

- [ ] README: install deps (cairo, pango, wayland, icon theme), feature flags matrix, example screenshots; document the exec JSON model and link to `scripts/` examples for Hyprland workspaces/window/keyboard.
- [ ] CHANGELOG policy; tag v0.1.0 (first working draft / first milestone).

**Verify**: full §7 gates + manual dogfood against `examples/*.toml`.

### Post-first-release — `mpris` (optional enhancement)

- [ ] **After** Phase 10 ships: add **`mpris`** module as an exec-handler — a user script (e.g. wrapping `playerctl`) emits `{"text": "Artist — Title"}` (or richer fields) to stdout; abar reads and renders. No `zbus` or D-Bus code in abar.
- [ ] Not part of the first milestone’s definition of done (§9).

**Verify**: dbus test harness or documented CI skip with local manual checklist.

---

## 9. Definition of done (v0 / first working draft)

- [ ] Bar shows on Wayland with **islands** matching theme from `examples/theme.toml`.
- [ ] Layout from `examples/config.toml` works for **clock**, **keyboard**, **`[modules].custom`** (FreeDesktop **icons** visible — Phase 4), **tray**, **workspaces**, and **window**.
- [ ] `workspaces`, `window`, `keyboard` are **exec-handler modules**: no `hyprland-rs` / `libxkbcommon` in `Cargo.lock`; state arrives via stdout from a user `exec` script; `scripts/` contains working Hyprland examples.
- [ ] **Tray** works as designed once Phase 9 is defined; no `zbus` or D-Bus in abar.
- [ ] **MPRIS** is **not** required for this milestone (planned post-Phase 10).
- [ ] Pointer actions spawn user commands; built-in clock behavior works without GUIs.
- [ ] **No** iced / winit for bar UI; Cairo+Pango drawing path is live.
- [ ] CI green on default / all-features / no-default-features; docs build; no banned deps in lock file.

---

## 10. Dependency policy (from WAU §2.1, adapted)

- **Edition**: `2024` (already in workspace package).
- **Versions**: `x.y` or `x` in manifests; lockfile committed.
- **Health**: avoid archived / unmaintained crates.
- **Async runtime**: **`tokio`** (`rt-multi-thread`, `process`, `time`, …) in **`libabar`** — standard for parallel background work; keep the dependency lean (no full workspace stack unless a phase needs it).
- **Banned deps**: `zbus`, `dbus`, `libdbus`, `dbus-glib`, `hyprland-rs`, `niri-ipc`, `libxkbcommon` — none of these may appear in `Cargo.lock`. All D-Bus and compositor IPC lives in external daemons.
- **Heavy deps**: justify in PR; keep unused code paths behind features.

---

## 11. Document maintenance

Update this plan when:

- feature/module set changes
- compositor backend policy changes
- examples change — update `examples/*.toml` first, then this doc
- §1.2 **Code comments** rule changes

---

## Revision history

| Date       | Change                                                                                                                                                                                                                                                                                                                                          |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-05-15 | Initial abar plan derived from WAU_RS_PLAN discipline + examples configs                                                                                                                                                                                                                                                                        |
| 2026-05-15 | Niri removed from scope; tray must-have with **zbus** + ashell semantic reference; MPRIS moved post-first-release                                                                                                                                                                                                                               |
| 2026-05-15 | §1.2 code-comment rule; layout tree: no `paths/`; `libabar` has no `toml`                                                                                                                                                                                                                                                                       |
| 2026-05-15 | Phase 3 done; **Tokio** documented as async runtime for spawn and future IPC/tray                                                                                                                                                                                                                                                               |
| 2026-05-15 | **Phase 4** added: FreeDesktop icons + visible custom modules; later phases renumbered (5–8)                                                                                                                                                                                                                                                    |
| 2026-05-16 | **Phase 5** keyboard: no built-in switching; hyprland feature = event socket, otherwise wl_keyboard + libxkbcommon                                                                                                                                                                                                                              |
| 2026-05-16 | **Phase 5** implemented: clock (chrono + chrono-tz, minute tick), keyboard (hyprland socket / xkb / static), poll loop replaces blocking_dispatch; xkb = separate feature                                                                                                                                                                       |
| 2026-05-16 | **Phase 6** workspaces: `use_markup` on Segment/PlacedSegment; Pango markup rendering path; Hyprland IPC via AsyncEventListener; monitor-specific filter; compositor-agnostic format_label                                                                                                                                                      |
| 2026-05-17 | **Phase 6** window: `WindowConfig { max_length }` + `truncate_title`; Hyprland `add_active_window_changed_handler`; optional `[window] max_length` config; pre-existing dead_code fixed                                                                                                                                                         |
| 2026-05-21 | **Architecture decision (issue #8):** ban compositor IPC libs from abar; `workspaces`/`window`/`keyboard` become IPC-handler modules (Tier 3); insert Phase 7 (IPC protocol + receiver) and Phase 8 (refactor compositor modules) before old Phase 7; renumber old 7→9, 8→10; update §1.3, §5.2, §6, §9                                         |
| 2026-05-21 | **No D-Bus/zbus in abar:** `tray` becomes Tier 3 IPC handler — `trayd` owns StatusNotifier/D-Bus, pushes JSON to abar; `mpris` uses `playerctl`-based external daemon; `zbus`/`dbus` added to banned-dep list (§1.3, §5.3–5.4, §6, §9, §10)                                                                                                     |
| 2026-05-21 | **exec-handler model:** replace Unix socket IPC with stdout-pipe-from-child-process; `exec` field per module config; abar reads newline-delimited `ModuleUpdate { text, icon?, markup? }`; script is thick layer for compositor specifics; Phase 7 rewritten; Phase 9 (tray) marked TBD; `scripts/keyboard.sh` + `docs/EXEC.md` added to layout |
| 2026-06-01 | **Phase 9 (tray) fully specified:** `trayd` IPC client (Unix socket NDJSON, `subscribe` stream, `get_pixmap` one-shots, exponential reconnect), pixmap cache, `TrayConfig`, tray module (Passive items skipped, click → spawn `trayctl`), `[tray]` config table, settings wiring, `tray` feature gate                                           |
