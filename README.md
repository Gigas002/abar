# abar

Minimalistic Wayland-native status bar rendered with [vello_cpu](https://github.com/linebender/vello) + [parley](https://github.com/linebender/parley), inspired by ashell/waybar.

![](examples/preview.png)

## Requirements

**Build dependencies:**

- Rust toolchain
- `libfontconfig1-dev` / `fontconfig` (system font discovery)
- A FreeDesktop icon theme (e.g. `hicolor`, `candy-icons`)

## Features

| Feature      | Description                                                                          |
| ------------ | ------------------------------------------------------------------------------------ |
| `clock`      | Clock; right click cycles its formats, scrolling cycles its timezones.               |
| `keyboard`   | Keyboard layout module driven by an exec script.                                     |
| `workspaces` | Workspace list module driven by an exec script.                                      |
| `window`     | Active window title module driven by an exec script.                                 |
| `mpris`      | Media player info module driven by an exec script.                                   |
| `tray`       | SNI system tray via [`trayd`](https://github.com/Gigas002/trayd) and an exec script. |
| `svg`        | SVG icon support (requires `resvg`; PNG-only without this flag).                     |

No features are enabled by default.

## Configuration

abar follows XDG conventions:

| Path                                       | Description                                                           |
| ------------------------------------------ | --------------------------------------------------------------------- |
| `$XDG_CONFIG_HOME/abar/config.toml`        | Bar layout and module config (default: `~/.config/abar/config.toml`). |
| `$XDG_CONFIG_HOME/abar/themes/<name>.toml` | Theme file referenced by `theme` key in config.                       |
| `$XDG_CONFIG_HOME/abar/scripts/`           | Recommended location for exec scripts.                                |

See [`examples/config.toml`](examples/config.toml) and
[`examples/theme.toml`](examples/theme.toml) for annotated reference configs.

A config or theme that is not valid TOML, or has a wrong type somewhere,
stops abar with the file, line and column of the problem. Unknown keys
(usually typos) are logged as warnings and ignored. Without `--config`, a
missing `config.toml` means the built-in defaults.

## Running

abar logs to stderr (`RUST_LOG=debug` for more detail). On `SIGTERM`,
`SIGINT` or `SIGHUP` it closes its bars and stops every exec script along
with whatever the scripts started.

## Exec script contract

All exec-handler modules (`keyboard`, `workspaces`, `window`, `mpris`, `tray`)
are driven by user scripts. See [`docs/EXEC.md`](docs/EXEC.md) for the full
JSON contract, field reference, and script lifecycle documentation.

## Icon theme

Custom modules and tray items use FreeDesktop icon names. abar reads the
`XDG_ICON_THEME` environment variable to select which theme to search (PNG
preferred, SVG supported via the `svg` feature).

```sh
export XDG_ICON_THEME=candy-icons
```

If `XDG_ICON_THEME` is unset, abar falls back to `hicolor`. Themes listed in
the theme's `index.theme` `Inherits=` line are searched next, then `hicolor`.

Icons that cannot be resolved are displayed as text (the module name) with a
warning logged.
