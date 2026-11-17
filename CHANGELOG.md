# Changelog

## [0.3.0] — Unreleased

### Added

- Exec script lines' `icon` field is shown: before the text, or alone when
  `text` is empty.
- Right click on the clock cycles through its `formats`.
- The workspaces script gets the theme's `[workspaces]` settings as
  `ABAR_WS_ACTIVE_COLOR`, `ABAR_WS_INACTIVE_COLOR` and `ABAR_WS_VISIBILITY`;
  the example scripts honour `visibility_mode = "monitor_specific"`.
- Unknown config and theme keys are logged as warnings.
- HiDPI: the bar and its popups are drawn at the output's scale, fractional
  (`wp_fractional_scale_v1` + `wp_viewporter`) or integer (`wl_output.scale`),
  instead of being upscaled and blurry.
- Tooltips: resting the pointer on a segment for half a second shows its
  tooltip — a tray item's (from trayd), a script line's new `tooltip`
  field, or a custom module's `tooltip` key.
- `[tray] app_icon_theme`: tray apps that ship their own icons
  (`icon_theme_path`, trayd 0.3) are shown with them when your icon themes
  have nothing (`"fallback"`, default), first (`"prefer"`) or never
  (`"never"`).
- The example configs bind middle click and the wheel on tray items to
  `trayctl secondary-activate` and `trayctl scroll` (trayd 0.3).
- Tests drive the Wayland event loop in a headless sway (CI installs sway
  and grim).

### Changed

- A config or theme with a syntax or type error, or a `--config`/`--theme`
  file that does not exist, now stops abar with the location of the problem
  instead of silently using the defaults. `font_size` must be in (0, 512].
- Logs go to stderr, coloured only when it is a terminal.
- Submenus are `xdg_popup`s of the bar: they close on any click outside them
  and hold keyboard focus only while open.
- The clock uses `jiff` instead of `chrono` and `chrono-tz`, both now
  unmaintained (RUSTSEC-2026-0543, RUSTSEC-2026-0555). Timezones come from
  the system's tz database (`/usr/share/zoneinfo`, kept current by the
  distribution) instead of a copy compiled into abar; formats render as
  before. libabar: `ClockConfig::timezones` holds `jiff::tz::TimeZone`s and
  `current_label` takes `Option<&TimeZone>`. Builds without the `clock`
  feature no longer pull in a date library.
- libabar: `paint_computed` and `paint_submenu` take the scale to draw at;
  `Segment`, `PlacedSegment`, `ScriptLine`, `ModuleUpdate`, `TrayConfig`
  and `MinimalTrayItem` gain fields (tooltips, `app_icon_theme`,
  `icon_theme_path`).
- libabar: `run_exec_handler` takes the script environment and a delivery
  closure instead of channels; the tray's own reader is gone. The unused
  workspace label formatter (`format_label`, `workspace_at_x`, …) is removed.
- Tray items with an unknown `status` are shown like `Active` instead of
  making the whole tray line unreadable.
- Dependencies updated to their latest releases: `vello_cpu` 0.4, `parley`
  0.13 and `resvg` 0.50 (rendered output is unchanged).

- Rendering migrated from Cairo + Pango to `vello_cpu` (rasterization) and
  `parley` (text shaping/layout); the build no longer needs cairo, pango or
  glib — only fontconfig for font discovery.
- Markup is parsed by abar itself and supports the commonly used Pango subset:
  `<span>` (`foreground`, `background`, `fgalpha`, `bgalpha`, `font_family`,
  `font_weight`, `font_style`, `font_size`, `underline`, `strikethrough`),
  `<b>`, `<i>`, `<u>`, `<s>`, `<big>`, `<small>`, `<tt>`. Invalid markup is
  now shown as plain text instead of being dropped.

### Fixed

- Every repaint leaked its shared-memory buffer into the compositor
  (~220 MB per 1000 hover changes).
- One mouse wheel notch ran a scroll handler several times (once per
  axis event); touchpads fired one per tiny movement.
- A burst of script output could leave the bar on a stale line forever.
- Exec scripts and their helpers kept running after abar exited; abar did
  not handle `SIGTERM`.
- The restart delay for scripts never went back down after reaching 30 s.
- A newline in a title made the bar two lines tall.
- A clock format with an unknown directive (e.g. `%J`) crashed abar; it is
  now shown as written, with a warning.
- The clock showed frozen seconds with `%S`, could stay on the
  pre-suspend time for up to a minute, and only ever used its first format.
- abar quit with "Resource temporarily unavailable" when a Wayland read
  found no complete message.
- Missing icons were never looked up again, and the icon cache grew without
  bound with apps that change their icon file.

### Security

- With `feed_id = true`, a tray app's id was put unquoted into a `sh -c`
  command, so an id like `$(cmd)` ran `cmd` on click. It is now quoted.
- Relative icon names containing `/` (e.g. `../../x`) from tray apps could
  read image files outside the icon themes; they are rejected.

## [0.2.0] — 04.09.2026

### Added

- `icons.prefer` config switch — `"exact"` (default) keeps strict FreeDesktop
  lookup so semantic suffixes like `-mute-symbolic` are preserved; `"theme"`
  strips suffixes per-theme so a themed icon wins over a `hicolor` exact
  match, trading status semantics for visual consistency.
- Niri support — `examples/niri_config.toml` and exec scripts for
  `keyboard`, `window`, `workspaces`, `tray`, and `leave` under
  `examples/scripts/*/niri*.sh`.
- Sway support — `examples/sway_config.toml` and exec scripts for
  `keyboard`, `window`, `workspaces`, `tray`, and `leave` under
  `examples/scripts/*/sway*.sh`.
- Multi-monitor support — a bar is now created for every connected output
  instead of just one; module updates, hover/press state, and submenus are
  tracked per-output.

### Changed

- Hyprland example scripts reorganized to match the niri/sway layout, split
  into `hyprland_switch.sh`, `hyprland_logout.sh`, `hyprland_menu.sh`, and
  `hyprland_scroll.sh`.

### Fixed

- Icon theme resolution — themes with non-standard layouts (e.g. candy-icons'
  `apps/scalable/` instead of `scalable/apps/`), `Inherits=` theme chains, and
  size-directory matching weren't honored, breaking icon lookups for themes
  other than the strict FreeDesktop default.
- `tray` module — re-subscribing after the `trayd` service died dropped the
  cached island/segment position, so the tray could reappear in the wrong
  slot (or not at all) once the service came back.
- Pointer scroll not registering on the `clock` (and other) modules under
  Sway/Niri — scroll (`Axis`/`AxisDiscrete`) events were only handled after a
  prior `Enter` event had set the hover position, which those compositors
  don't always emit first; duplicate `Axis`/`AxisDiscrete` events for the
  same scroll tick are now also deduplicated.
- `examples/scripts/workspaces/hyprland.sh` — listen for `focusedmon` IPC
  events in addition to `workspace`/`createworkspace`/`destroyworkspace`/
  `moveworkspace`. Switching input focus to a monitor whose workspace was
  already active there only emits `focusedmon`, not `workspace`, so the
  highlighted workspace could go stale until an unrelated workspace change
  happened to resync it.
- Tray picker not opening on the focused monitor in multi-monitor setups —
  `hyprland_menu.sh`/`niri_menu.sh`/`sway_menu.sh` now detect the focused
  output via each compositor's own IPC and export `ABAR_OUTPUT` before
  handing off to the shared `examples/scripts/tray/tray-menu.sh`, which reads
  it and passes `--output` to `tofi`.

## [0.1.0] — 11.06.2026

### Added

- Wayland layer shell bar using `zwlr_layer_shell_v1` — anchors to screen edge
  with correct exclusive zone; renders via Cairo + Pango on a shared-memory
  buffer.
- Islands layout: rounded-rect background regions with configurable padding and
  gap; `left` / `center` / `right` regions; nested arrays for multi-module
  islands.
- FreeDesktop icon resolution — PNG preferred, optional SVG via the `svg`
  feature (`resvg`); `XDG_ICON_THEME` aware.
- Custom modules — icon-only segments with configurable pointer-event handlers
  (`on_left_click`, `on_right_click`, `on_middle_click`, `on_scroll_up`,
  `on_scroll_down`); startup error on unresolvable icon.
- Exec-handler model — all compositor-specific modules are driven by
  user-provided scripts over stdout NDJSON; no compositor IPC libraries inside
  abar (see `docs/EXEC.md`).
- `clock` module — format rotation, timezone cycling, per-minute tick.
- `keyboard` module — layout label driven by exec script; Hyprland reference
  script at `examples/scripts/keyboard/hyprland.sh`.
- `workspaces` module — exec-driven; Hyprland reference script emits
  Pango-markup coloured workspace list
  (`examples/scripts/workspaces/hyprland.sh`).
- `window` module — active window title from exec script; configurable
  `max_length` truncation (`examples/scripts/window/hyprland.sh`).
- `mpris` module — media info from exec script; `playerctl` reference script at
  `examples/scripts/mpris/playerctl.sh`.
- `tray` module — SNI system tray backed by
  [`trayd`](https://github.com/Gigas002/trayd); `trayctl subscribe` streams
  `Vec<MinimalTrayItem>` JSON arrays; `Passive` items skipped; `feed_id` appends
  `app_id` to `on_*` handlers; reference scripts at
  `examples/scripts/tray/`.
- XDG config resolution (`$XDG_CONFIG_HOME/abar/`) with `--config` / `--theme`
  CLI overrides.
- Per-feature Cargo gates: `clock`, `keyboard`, `workspaces`, `window`,
  `mpris`, `tray`, `svg`.
