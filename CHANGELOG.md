# Changelog

## Unreleased

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
