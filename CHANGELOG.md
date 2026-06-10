# Changelog

## [0.1.0] — Unreleased

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
