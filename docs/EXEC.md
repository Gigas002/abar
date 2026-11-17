# abar exec-handler contract

Exec-handler modules spawn a user-provided script (`sh -c <exec>`) and read
newline-delimited JSON from its stdout. abar restarts the script whenever it
exits (see [Script lifecycle](#script-lifecycle)).

---

## Standard modules — `ScriptLine`

`workspaces`, `window`, and `mpris` each expect one JSON object per line:

```json
{ "text": "string", "markup": false, "icon": null }
```

| Field    | Type           | Required | Description                                                                                                                  |
| -------- | -------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `text`   | string         | yes      | Text to display. May contain Pango-style markup when `markup` is `true`.                                                     |
| `markup` | bool           | no       | Default `false`. When `true`, `text` is parsed as markup (see below).                                                        |
| `icon`   | string \| null | no       | FreeDesktop icon name or absolute path, shown before the text (alone when `text` is empty). Ignored when it cannot be found. |

Unknown fields are silently ignored.

**Examples:**

```sh
# plain text
jq -cn --arg t "22:05" '{text: $t}'

# Pango markup (workspaces with colour)
jq -cn --arg t '<span foreground="#00c1e4">1</span>  2  3' '{text: $t, markup: true}'
```

**Markup subset.** abar parses the commonly used part of Pango markup itself:

- Tags: `<span>`, `<b>`, `<i>`, `<u>`, `<s>`, `<big>`, `<small>`, `<tt>`.
- `<span>` attributes: `foreground`/`fgcolor`/`color`, `background`/`bgcolor`,
  `fgalpha`/`alpha`, `bgalpha`, `font_family`/`face`, `font_weight`/`weight`,
  `font_style`/`style`, `font_size`/`size`, `underline`, `strikethrough`.
  Other attributes are ignored.
- Colors: `#rgb`, `#rrggbb`, `#rrggbbaa`, `#rrrrggggbbbb`, or a basic color name.
- Entities: `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&#NN;`, `&#xNN;`.

Markup that fails to parse (unknown tag, unclosed tag, …) is shown as plain text.

### Workspaces environment

The workspaces script gets the theme's `[workspaces]` settings in its
environment, so it can colour and filter its output:

| Variable                 | From theme key    | Value                                                                                   |
| ------------------------ | ----------------- | --------------------------------------------------------------------------------------- |
| `ABAR_WS_ACTIVE_COLOR`   | `active_color`    | `#rrggbb` (alpha dropped); unset when the theme has none.                               |
| `ABAR_WS_INACTIVE_COLOR` | `inactive_color`  | `#rrggbb` (alpha dropped); unset when the theme has none.                               |
| `ABAR_WS_VISIBILITY`     | `visibility_mode` | `all_monitors` (default), or `monitor_specific`: only the focused monitor's workspaces. |

The example scripts in `examples/scripts/workspaces/` honour all three.

---

## Keyboard module — `KeyboardData`

The `keyboard` module expects one JSON object per line using a `label` field
(not `text`, so it does not collide with Pango markup processing):

```json
{ "label": "en-US" }
```

| Field   | Type   | Required | Description                            |
| ------- | ------ | -------- | -------------------------------------- |
| `label` | string | yes      | Layout name to display in the segment. |

See `examples/scripts/keyboard/hyprland.sh` for a working Hyprland example.

---

## Tray module — `Vec<MinimalTrayItem>`

The `tray` module expects one JSON **array** per line. Each element describes
one SNI tray item:

```json
[
  {
    "app_id": "nm-applet",
    "title": "Network",
    "status": "Active",
    "icon_handle": "network-wireless"
  },
  {
    "app_id": "pasystray",
    "title": null,
    "status": "Passive",
    "icon_handle": null
  }
]
```

| Field                 | Type           | Required | Description                                                                                                               |
| --------------------- | -------------- | -------- | ------------------------------------------------------------------------------------------------------------------------- |
| `app_id`              | string         | yes      | Unique stable identifier for the item (used in `trayctl menu --app-id`).                                                  |
| `title`               | string \| null | no       | Human-readable name shown in fallback pickers.                                                                            |
| `status`              | string         | yes      | `"Active"`, `"Passive"`, or `"NeedsAttention"`. `Passive` items are not rendered; any other value is shown like `Active`. |
| `icon_handle`         | string \| null | no       | FreeDesktop icon name. Items without a resolvable icon are shown as text (tooltip title, title or `app_id`).              |
| `tooltip_title`       | string \| null | no       | Also tried as an icon name, lowercased (`"Discord"` → `discord`).                                                         |
| `tooltip_description` | string \| null | no       | Read, not shown yet.                                                                                                      |
| `icon_theme_path`     | string \| null | no       | Directory of icons the app ships (SNI `IconThemePath`); see `app_icon_theme`.                                             |

Other fields (trayd also sends `category`, `has_pixmap`, …) are ignored.

Emit an empty array `[]` to clear the tray:

```json
[]
```

See `examples/scripts/tray/tray.sh` for the reference script (`trayctl subscribe`).

### `feed_id`

When `feed_id = true` in `[tray]`, abar appends each item's `app_id` as a
positional argument to every configured `on_*` handler when segments are
rebuilt. The id is single-quoted, so it arrives as one argument exactly as
the app reported it, even with spaces or shell characters in it. For example, with:

```toml
[tray]
feed_id = true
on_left_click = "~/.config/abar/scripts/tray/tray-menu.sh"
```

clicking the `nm-applet` tray item invokes:

```sh
sh -c "~/.config/abar/scripts/tray/tray-menu.sh 'nm-applet'"
```

See `examples/scripts/tray/tray-menu.sh` for a `tofi`-based picker that uses
this to call `trayctl menu --app-id <app_id>` directly.

### `app_icon_theme`

Some apps ship their own icons and name the directory in `icon_theme_path`.
`app_icon_theme` in `[tray]` decides whether abar looks there:

| Value                  | Effect                                                                               |
| ---------------------- | ------------------------------------------------------------------------------------ |
| `"fallback"` (default) | Only when nothing in your icon themes matches the icon name, tooltip title or title. |
| `"prefer"`             | The app's icons first, then your icon themes.                                        |
| `"never"`              | Never; such items show text when your themes lack the icon.                          |

Inside the directory, abar looks for a theme named like yours, then
`hicolor` (`<dir>/hicolor/22x22/apps/<name>.png`), then `<dir>/<name>.png`.

---

## Script lifecycle

- Scripts are spawned via `sh -c <exec>`.
- stdout is consumed line-by-line; blank lines are skipped.
- Lines that are not valid JSON for the expected type are logged as warnings
  and skipped — the script keeps running.
- When the script exits (for any reason), abar logs a warning and restarts it
  after a delay: 1 s, doubling up to 30 s while it keeps exiting within 10 s
  of starting, and 1 s again after a run of 10 s or more.
- Each script runs in its own process group. When abar stops (`SIGTERM`,
  `SIGINT`, `SIGHUP`, or the compositor closing the bar), and before a
  script is restarted, the whole group gets `SIGTERM`, so helpers such as
  `socat` or `trayctl subscribe` don't outlive it.
- When a script prints many lines faster than the bar repaints, only the
  latest one is shown; the final state is never lost.
- stdin is kept open as a reserved back-channel (not currently used).

Scripts should be **long-running** where possible (emit an initial line then
block on events) to avoid noisy restart logs. One-shot scripts that exit
cleanly are also fine — abar will restart them, which effectively becomes
a poll loop at the backoff interval.
