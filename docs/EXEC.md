# abar exec-handler contract

Exec-handler modules (`keyboard`, `workspaces`, `window`, `mpris`) receive live state from a
user-configured script rather than from abar directly.  This document defines the JSON model
each script must emit and the responsibilities on each side of the pipe.

---

## How it works

abar spawns the configured command via `sh -c <exec>` and reads **newline-delimited JSON** from
its stdout.  One line = one module update.  The script is the thick layer that handles all
compositor-specific, D-Bus, or daemon-specific logic; abar handles only the JSON model and
rendering.

If the script exits for any reason, abar restarts it with exponential backoff (1 s → 2 s → …
→ 30 s cap) and logs a warning.

---

## JSON model

Each line must be a JSON object.  Unknown fields are silently ignored.

### Required

| Field  | Type   | Description                        |
| ------ | ------ | ---------------------------------- |
| `text` | string | Text to display in the bar segment |

### Optional

| Field    | Type    | Default | Description                                                     |
| -------- | ------- | ------- | --------------------------------------------------------------- |
| `markup` | boolean | `false` | When `true`, `text` is rendered as Pango markup                 |
| `icon`   | string  | absent  | FreeDesktop icon name or absolute path (reserved; not yet used) |

### Examples

Plain text:
```json
{"text": "en-US"}
```

Pango markup (workspace list with colored active entry):
```json
{"text": "<span foreground=\"#cba6f7\">2</span>  3  4", "markup": true}
```

---

## Script responsibilities

- Emit one JSON line per state change (the first line is shown immediately on startup).
- Run indefinitely (subscribe to compositor events, a D-Bus signal, etc.) so updates arrive
  in real time without polling.
- Handle all compositor-specific IPC internally; abar has no knowledge of the protocol.
- Exit cleanly when stdin closes (abar may close stdin as a shutdown signal in a future
  version).

## abar responsibilities

- Spawn the script with `sh -c <exec>`, inheriting the environment.
- Read each stdout line, parse it as `ScriptLine`, update the segment, and repaint.
- Log and skip lines that are not valid JSON.
- Restart the script with backoff on unexpected exit.
- Provide a stdin pipe for future back-channel signals (currently unused).

---

## Configuration

Add an `exec` field to the relevant module table in `config.toml`:

```toml
[keyboard]
exec = "~/.config/abar/scripts/keyboard.sh"
on_left_click = "hyprctl switchxkblayout all next"

[window]
exec = "~/.config/abar/scripts/window.sh"
max_length = 50

[workspaces]
exec = "~/.config/abar/scripts/workspaces.sh"
```

If `exec` is absent the module renders a static placeholder (set by the initial config value,
e.g. the first entry in `[keyboard].layouts`).

---

## Reference implementations

`scripts/keyboard.sh` — Hyprland keyboard layout via `hyprctl` + `.socket2.sock`:
- Emits `{"text": "en-US"}` on startup.
- Subscribes to `activelayout` events and re-emits on each layout change.
- Requires `hyprctl`, `jq`, and `socat`.

Additional scripts for `workspaces` and `window` will be added in Phase 8.
