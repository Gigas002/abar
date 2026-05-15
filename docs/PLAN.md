# abar — Rust architecture + implementation plan

This document is the **human roadmap** and **agent playbook** for **abar**: a minimal Wayland-native status bar (ashell / waybar inspired) using **Cairo + Pango** for drawing (same stack direction as [tofi-rs](https://github.com/philj56/tofi-rs) `libtofi`), **no** heavyweight UI toolkits (e.g. iced), and **no** in-bar dialogs or menus — user-visible actions are **spawn external commands** (shell runner) or **non-GUI** side effects only.

It mirrors the **execution discipline** of `wau/docs/WAU_RS_PLAN.md`:

- Library-first crate split, small verifiable phases, strict quality gates (fmt, clippy `-D warnings` with feature matrix, tests, `cargo doc`, `typos`, `cargo deny`).
- **Directory modules** with **sibling `tests.rs`** — tests never live in the same file as logic (same rule as WAU §2.0).
- **Per-integration Cargo features** so minimal installs and CI do not bitrot optional code paths.

**Reference configs (source of truth for schemas):**

- `examples/config.toml` — layout, modules, events as command strings, keyboard/clock options.
- `examples/theme.toml` — colors, opacity, module-specific theme keys (ashell-like palette).
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

- **Library-first**: **`libabar`** — parsing, layout math, module trait/state, theme merge, spawn helpers, Wayland protocol glue that is testable; **`abar`** — thin `main` (tracing init, CLI, load settings, run event loop).
- **`abar` contains no domain logic** beyond wiring; **`libabar` does not depend on clap** and does not assume a specific logger implementation beyond `tracing`.
- **Step sizing**: small PR-sized phases with explicit **Verify** blocks.
- **Feature matrix in CI**: default, `--all-features`, `--no-default-features` (core must still build: e.g. bar shell + clock-only or stub modules — define explicitly in Phase 0). **Tray** is part of the first shippable bar; **MPRIS** is explicitly **not** in that milestone (see §8 post-first-release).
- **Naming**: short, descriptive; prefer clarity over abstraction depth.

### 1.3 Non-goals / dropped ashell concepts

- **No** first-party popovers, settings panels, Wi-Fi/BT/audio “applets”, calendar UI, or **any** in-process menu widgets.
- **No** iced / winit / egui / Qt / GTK windowing for the bar.
- **Tempo**, **Privacy**, embedded **Settings** modules, **weather in clock strip**, etc. from `ashell_config.toml` — **out of scope** unless reintroduced later as **optional** features with **external** commands only.
- **No** promise of pixel-perfect ashell clone; target is **similar** islands + colors + font from examples.

### 1.4 Definitions

- **Module**: a unit that contributes one island (or shares an island in a **group**). Either **built-in** (behind its own feature) or **custom** (icon + events only).
- **Island**: one rounded background region containing one or more module **segments** (text/icon) with inner spacing.
- **Layout row**: three logical regions `left` / `center` / `right`, each a list of **slots**; a slot is either a module name or a **nested array** expressing one island with multiple modules (see `examples/config.toml`).

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
      error.rs                 # thiserror
      config/                  # parse + validate config.toml; merge CLI paths
      theme/                   # parse theme.toml; defaults + module sections
      layout/                  # resolve layout → ordered islands + alignment
      render/                  # cairo+pango: measure, draw islands, damage regions
      wayland/                 # compositor connection, layer_shell, outputs, input
      modules/                 # one subdirectory per built-in module
        clock/
          mod.rs
          tests.rs
        keyboard/
          mod.rs
          tests.rs
        # ... workspaces, window, tray — compositor + tray behind features; MPRIS deferred (§8)
      spawn/                   # safe command execution, logging failures
      model/                   # shared small types (ids, colors, keys)
  abar/
    Cargo.toml                 # clap, tracing-subscriber, libabar features passthrough
    src/
      main.rs                  # minimal
      cli/
        mod.rs
        tests.rs
      settings/
        mod.rs                 # merged view: cli > env > config
        tests.rs
  docs/
    PLAN.md                    # this file
  .github/workflows/           # already scaffolded — extend as in §7
```

**Crate boundary rules**

- `libabar` has **no** `clap`; **no** `println!` in library code (use `tracing`).
- After `Settings` (or `RuntimeConfig`) is built in `abar`, only that merged struct crosses into the run loop — avoid threading raw `clap` types through `libabar`.

**Optional:** `abar` features are **thin passthroughs** to `libabar` features (WAU §2.2 pattern) so packagers can `cargo install abar --no-default-features --features "clock,keyboard,hyprland,tray"`.

---

## 3. Data model and config

### 3.1 `config.toml` (see `examples/config.toml`)

**Intent**

- **`[base]`**: `font` (required), `theme` filename or path relative to themes dir.
- **`[layout]`**: `left` / `center` / `right` lists; nested arrays = single island, inner order = left-to-right segments.
- **Per-module tables** (e.g. `[keyboard]`, `[clock]`) for module-specific options; **global event tables** live on each module definition — for custom modules under `[custom_modules]`, for built-ins merge: defaults < `[clock]` etc.
- **Events**: string commands executed via shell (`sh -c` or explicit documented runner); scroll/button names as in example.

**Invariants**

- Unknown keys: policy TBD in Phase 1 (recommend **warn** via tracing once strict mode is off; **fail** in `--validate` mode if implemented).
- Missing `font` in base: **fail fast at startup** with clear error (per example comment).

### 3.2 `theme.toml` (see `examples/theme.toml`)

**Intent**

- Global `background_color`, `foreground_color`, `opacity`; optional per-module sections (e.g. `[workspaces]` colors, `visibility_mode`).
- **`scale_factor`**: deferred (TODO in example) — Phase 2 can hardcode `1.0` + env-based fractional scale from Wayland only.

### 3.3 Mapping from `ashell_config.toml`

- **Appearance** keys map into `theme.toml`; **layout** naming differs — any future migration helpers (e.g. ashell module name → abar id) are **optional** and not required for the first release.

---

## 4. Rendering and UI

### 4.1 Cairo + Pango pipeline

- Build layout: for each island, compute **width/height** from max of children measurements + padding + corner radius.
- Draw to **ARGB32** (or premultiplied, decide once and test on multiple compositors) image surface; upload to `wl_buffer`.
- **Icons**: prefer **freedesktop-icon-name** resolution loading symbolic / fullcolor PNG/SVG (SVG via `resvg` is optional behind feature — start with **PNG only** or text fallback to reduce deps in v0).
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

Each backend behind its own feature so CI and minimal users do not link unused IPC. **Near-term target:** **Hyprland only** for `workspaces` / `window` (IPC for tags/workspaces and active window title). Additional compositors are **out of scope until explicitly planned** — each future compositor remains **its own Cargo feature** and module set (same rule as WAU §2.2).

| Feature (example name) | Responsibility                                                                                                              |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `hyprland`             | Hyprland IPC: workspaces, active window title (hyprland-rs or hand-rolled Unix socket — pick smallest dep that fits policy) |

**Rule:** no compositor-specific code in `workspaces` / `window` without the matching feature; either stub (“inactive”) or omit modules at compile time.

### 5.3 Tray (**must-have** for first shippable milestone)

- **Required** for the first working release in this plan: system tray in the bar (same user expectation as **ashell**). **ashell**’s tray implementation is a useful **behavioral reference** (StatusNotifier host, item lifecycle, menus exposed over D-Bus) even though ashell’s **UI** stack is iced — abar reimplements **host + rendering** with **Cairo + Pango** only.
- **D-Bus:** use **native Rust** [**zbus**](https://crates.io/crates/zbus) (and the usual pure-Rust ecosystem around it). **Do not** depend on `libdbus` / `dbus-glib` C libraries for tray or future media features.
- Isolate all D-Bus I/O behind a **`tray` feature** in `libabar` with `abar` passthrough so `--no-default-features` CI stays meaningful, but **default `abar` binary** should include tray (first milestone assumes tray on).

### 5.4 MPRIS (deferred — see §8 post-first-release)

- **Not** required for the first working draft or v0 definition of done. When added later, use **`zbus`** only (same policy as §5.3); keep behind a dedicated **`mpris`** feature.

---

## 6. Module catalog (compile-time)

Each built-in module: **`libabar/src/modules/<name>/`** + **`tests.rs`**, gated by **`features.<name>`**.

| Module       | First-milestone scope                                | Notes                                                     |
| ------------ | ---------------------------------------------------- | --------------------------------------------------------- |
| `clock`      | timezones + format cycle + optional `on_left_click`  | no GUI calendar — external command only                   |
| `keyboard`   | layout switch + labels from config                   | optional override clicks                                  |
| `workspaces` | compositor feature (`hyprland` first)                | monitor filter per theme `visibility_mode`                |
| `window`     | active title                                         | ellipsis, compositor feature                              |
| `tray`       | **required** — StatusNotifier-style host, **`zbus`** | behavior reference: **ashell** (not UI stack); no libdbus |
| `custom`     | icon + events                                        | always available if `custom_modules` table exists         |
| `mpris`      | **post-first-release** (§8)                          | track/artist via **`zbus`** when implemented              |

**Custom modules**: unique name, **icon name** required; missing icon → **startup error** (per examples).

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
- Integration tests under `libabar/tests/` for: config parse fixtures, theme merge, layout expansion (nested arrays → islands).

### 7.2 CI

Existing workflows (`build`, `fmt-clippy`, `test`, `doc`, `typos`, `deny`) should be kept and updated once crates exist; matrix already includes feature variants — ensure **doc** and **test** jobs use the same matrices as WAU recommends.

---

## 8. Phased steps

### Phase 0 — Workspace + hygiene + empty vertical slice

- [ ] Fix root `Cargo.toml` **workspace members**: `["libabar", "abar"]` (add `libabar` crate).
- [ ] Implement minimal `libabar` + `abar` with **no modules** yet: parse **minimal** `config.toml` (only `[base]` + empty layout) and **theme.toml**; exit with structured error if font missing.
- [ ] Wire **tracing** + `tracing-subscriber` in `abar` only.
- [ ] Populate **`deny.toml` licenses allow list** for used crates.
- [ ] Hello Wayland: connect, bind globals, create **layer surface** strip (solid color from theme), no text yet.

**Verify**: all gates in §7; manual run on **Hyprland** (or another compositor you explicitly add later).

### Phase 1 — Config + theme + layout model

- [ ] Full serde models matching `examples/config.toml` / `theme.toml` (including nested layout arrays, `custom_modules`, event strings).
- [ ] XDG path resolution + `--config` / `--theme` flags (`abar` / `clap`).
- [ ] Validation: unknown modules, duplicate names, feature not enabled at compile time → compile-time `cfg` + runtime check for config that references disabled modules (clear error).

**Verify**: unit tests for parse + validation; golden error messages snapshot optional.

### Phase 2 — Render core (Cairo + Pango)

- [ ] Font loading, Pango measurement helpers, Cairo rounded-rect helper.
- [ ] Island layout pass: compute bar height from font metrics + padding; horizontal distribution for `left`/`center`/`right` (center cluster truly centered).
- [ ] Draw static placeholder text per module slot (“clock”, “kb”, …) before real data.

**Verify**: headless tests where possible (image buffer pixel samples); optional `insta` PNG snapshots gated behind feature.

### Phase 3 — Pointer input + spawn

- [ ] Wayland pointer events → hit-test which island/segment.
- [ ] Map to configured command; execute without blocking UI thread (use `std::thread` or small bounded worker; log failures).

**Verify**: integration test with mock command (script that touches tempfile).

### Phase 4 — `clock` + `keyboard` modules

- [ ] `clock`: formats + timezones rotation; tick each second/minute based on whether seconds in format; optional overrides.
- [ ] `keyboard`: integrate **xkb** / compositor layout APIs as needed, or `locale1` / **Hyprland** — **pick simplest** for the near-term compositor target (§5.2).

**Verify**: unit tests for format rotation logic (no Wayland).

### Phase 5 — Compositor modules (`workspaces`, `window`)

- [ ] Behind **`hyprland`** feature only for the near-term; IPC integrated with the main Wayland event loop (avoid starving the Wayland socket — non-blocking IPC or short-lived queries on a timer).

**Verify**: manual on Hyprland; mocked JSON/socket tests where feasible.

### Phase 6 — **Tray** (must-have): `zbus` + StatusNotifier host

- [ ] Implement tray host and item rendering (icons, attention state, **simple** menu exposure if required by spec — still drawn with Cairo/Pango or delegated only via user-spawned commands per product rules in §1.3; **no** iced menus).
- [ ] All D-Bus via **`zbus`**; gate in **`tray`** feature with **default-on** for the `abar` binary.
- [ ] Use **ashell** source as a **semantic** reference for registration names, watcher protocol, and edge cases; do not copy iced-dependent UI.

**Verify**: manual with real tray apps; unit tests for protocol parsing/state where possible; CI strategy for headless D-Bus documented if tests are skipped.

### Phase 7 — Polish + first release

- [ ] README: install deps (cairo, pango, wayland), feature flags matrix, example screenshots.
- [ ] CHANGELOG policy; tag v0.1.0 (first working draft / first milestone).

**Verify**: full §7 gates + manual dogfood against `examples/*.toml`.

### Post-first-release — `mpris` (optional enhancement)

- [ ] **After** Phase 7 ships: add **`mpris`** module behind **`mpris`** feature using **`zbus`** only (no libdbus); polling or signals with a conservative rate limit so the Wayland loop stays responsive.
- [ ] Not part of the first milestone’s definition of done (§9).

**Verify**: dbus test harness or documented CI skip with local manual checklist.

---

## 9. Definition of done (v0 / first working draft)

- [ ] Bar shows on Wayland with **islands** matching theme from `examples/theme.toml`.
- [ ] Layout from `examples/config.toml` works for **clock**, **keyboard**, **custom_modules**, **tray**, and **Hyprland**-backed **workspaces + window** (document any gaps vs ashell).
- [ ] **Tray** works with real StatusNotifier items via **`zbus`** (no libdbus).
- [ ] **MPRIS** is **not** required for this milestone (planned in §8 post-first-release).
- [ ] Pointer actions spawn user commands; built-in clock/keyboard behaviors work without GUIs.
- [ ] **No** iced / winit for bar UI; Cairo+Pango drawing path is live.
- [ ] CI green on default / all-features / no-default-features; docs build.

---

## 10. Dependency policy (from WAU §2.1, adapted)

- **Edition**: `2024` (already in workspace package).
- **Versions**: `x.y` or `x` in manifests; lockfile committed.
- **Health**: avoid archived / unmaintained crates.
- **Heavy deps**: justify in PR (e.g. **`zbus`** for **tray** and later **MPRIS**); keep unused code paths behind features.

---

## 11. Document maintenance

Update this plan when:

- feature/module set changes
- compositor backend policy changes
- examples change — update `examples/*.toml` first, then this doc

---

## Revision history

| Date       | Change                                                                                                            |
| ---------- | ----------------------------------------------------------------------------------------------------------------- |
| 2026-05-15 | Initial abar plan derived from WAU_RS_PLAN discipline + examples configs                                          |
| 2026-05-15 | Niri removed from scope; tray must-have with **zbus** + ashell semantic reference; MPRIS moved post-first-release |
