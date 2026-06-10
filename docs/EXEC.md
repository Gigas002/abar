# abar exec-handler contract

Exec-handler modules spawn a user-provided script (`sh -c <exec>`) and read
newline-delimited JSON from its stdout. abar restarts the script automatically
on unexpected exit with exponential backoff (1 s → 2 s → … → 30 s max).

---

## Standard modules — `ScriptLine`

`workspaces`, `window`, and `mpris` each expect one JSON object per line:

```json
{ "text": "string", "markup": false, "icon": null }
```

| Field    | Type            | Required | Description |
|----------|-----------------|----------|-------------|
| `text`   | string          | yes      | Text to display. May contain Pango markup when `markup` is `true`. |
| `markup` | bool            | no       | Default `false`. When `true`, `text` is passed to Pango as markup. |
| `icon`   | string \| null  | no       | FreeDesktop icon name or absolute path. Overrides the segment icon. |

Unknown fields are silently ignored.

**Examples:**

```sh
# plain text
jq -cn --arg t "22:05" '{text: $t}'

# Pango markup (workspaces with colour)
jq -cn --arg t '<span foreground="#00c1e4">1</span>  2  3' '{text: $t, markup: true}'
```

---

## Keyboard module — `KeyboardData`

The `keyboard` module expects one JSON object per line using a `label` field
(not `text`, so it does not collide with Pango markup processing):

```json
{ "label": "en-US" }
```

| Field   | Type   | Required | Description |
|---------|--------|----------|-------------|
| `label` | string | yes      | Layout name to display in the segment. |

See `examples/scripts/keyboard/hyprland.sh` for a working Hyprland example.

---

## Tray module — `Vec<MinimalTrayItem>`

The `tray` module expects one JSON **array** per line. Each element describes
one SNI tray item:

```json
[
  { "app_id": "nm-applet", "title": "Network", "status": "Active", "icon_handle": "network-wireless" },
  { "app_id": "pasystray", "title": null,       "status": "Passive", "icon_handle": null }
]
```

| Field         | Type            | Required | Description |
|---------------|-----------------|----------|-------------|
| `app_id`      | string          | yes      | Unique stable identifier for the item (used in `trayctl menu --app-id`). |
| `title`       | string \| null  | no       | Human-readable name shown in fallback pickers. |
| `status`      | string          | yes      | `"Active"`, `"Passive"`, or `"NeedsAttention"`. `Passive` items are not rendered. |
| `icon_handle` | string \| null  | no       | FreeDesktop icon name. Items without a resolvable icon are skipped. |

Emit an empty array `[]` to clear the tray:

```json
[]
```

See `examples/scripts/tray/tray.sh` for the reference script (`trayctl subscribe`).

### `feed_id`

When `feed_id = true` in `[tray]`, abar appends each item's `app_id` as a
positional argument to every configured `on_*` handler when segments are
rebuilt. For example, with:

```toml
[tray]
feed_id = true
on_left_click = "~/.config/abar/scripts/tray/tray-menu.sh"
```

clicking the `nm-applet` tray item invokes:

```sh
sh -c "~/.config/abar/scripts/tray/tray-menu.sh nm-applet"
```

See `examples/scripts/tray/tray-menu.sh` for a `tofi`-based picker that uses
this to call `trayctl menu --app-id <app_id>` directly.

---

## Script lifecycle

- Scripts are spawned via `sh -c <exec>`.
- stdout is consumed line-by-line; blank lines are skipped.
- Lines that are not valid JSON for the expected type are logged as warnings
  and skipped — the script keeps running.
- When the script exits (for any reason), abar logs a warning and restarts it
  after a backoff delay starting at 1 s, doubling up to 30 s.
- stdin is kept open as a reserved back-channel (not currently used).

Scripts should be **long-running** where possible (emit an initial line then
block on events) to avoid noisy restart logs. One-shot scripts that exit
cleanly are also fine — abar will restart them, which effectively becomes
a poll loop at the backoff interval.
