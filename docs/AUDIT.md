# Audit

A whole-application audit of libabar and abar (2026-11-17) in six parts:
security (§2), bugs (§3), performance (§4), UI usability and design (§5),
code quality, tests and project hygiene (§6), and features compared with other
bars (§7). §8 lists what was left open, and §9 is the checklist to re-run the
audit.

Each finding says what was found, how serious it is, and what was done:
**fixed** (with the commit), **accepted** (with the reason), or **OK** (checked,
nothing to do). Every fix is its own commit, with a regression test where one
can be written; each fix was reproduced on the original build and checked
again on the fixed one.

Severity: **high** (the bar shows wrong state or degrades the desktop in normal
use), **medium** (a crash, leak or unbounded growth reachable by a script or
tray app, behaviour that contradicts the docs or config), **low** (hardening,
polish, or needs an unusual setup).

## 1. Method

Every source file was read in full. Each finding below was then reproduced
against a running bar. The user's own bar (and its trayd) was never touched:
every live test ran in a separate **headless sway** (wlroots headless backend,
pixman renderer, 1280×720) with test configs whose modules were fed from FIFOs
and small scripts. A build of the original code (b484a0a) ran next to each
fix for before/after comparisons.

- **Input** came from a small virtual-pointer/virtual-keyboard client
  (`zwlr_virtual_pointer_v1`, `zwp_virtual_keyboard_v1`): moves, clicks,
  wheel notches, hi-res wheel and touchpad scrolls, key presses.
- **Output** was read back with `grim` screenshots.
- **Leaks** were measured as the system's `Shmem` in `/proc/meminfo` (the
  compositor holds the bar's buffers). CPU and RSS came from `/proc/<pid>`.
- **Focus** was checked with an Alacritty window that recorded what was typed
  into it.
- **The tray** ran against the local trayd/trayctl (not yet released) on a
  private `dbus-daemon` with a scripted fake StatusNotifierItem.
- **Static checks**: the gates, `cargo deny check`, `typos`,
  `cargo llvm-cov`, and `cargo package` of libabar.

All test processes, outputs and buses were removed afterwards.

## 2. Security

abar runs commands from its own config (`exec`, `on_*`, submenu actions)
through `sh -c`. That config is the user's own, so those commands are trusted.
Untrusted text comes from what scripts print, which includes window titles,
media titles and tray data from other apps.

| Area                 | Finding                                                                                                                                                                                                                                                                    | Status                   |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ |
| Tray id in a command | With `feed_id = true`, a tray item's `app_id` (set by the app) was appended unquoted to the `on_*` commands run by `sh -c`. A fake app with the id `evil app $(touch PWNED)` created the file when its icon was clicked. The id is now single-quoted and arrives verbatim. | Fixed, high (e817f04)    |
| Icon name traversal  | A relative icon name such as `../../x` (tray apps choose their icon names) was joined onto the icon theme directories, reading image files outside them. Relative names containing `/` or starting with `.` are rejected; absolute paths stay supported, as documented.    | Fixed, low (f3549d5)     |
| Icon cache growth    | Every icon name stayed cached forever. Electron/Chromium tray icons point at a new temp file on every change, so the cache only grew. It now holds at most 256 icons (least recently used dropped first).                                                                  | Fixed, medium (f3549d5)  |
| Script output        | Markup that fails to parse is shown as text; control characters (newlines, escape sequences) can no longer change the bar's height (§3). Lines that aren't valid JSON are logged and skipped.                                                                              | Fixed (237bb8d) / OK     |
| Orphaned processes   | Scripts and their helpers outlived the bar (§3).                                                                                                                                                                                                                           | Fixed (117916a)          |
| Config commands      | `exec`, `on_*` and submenu actions run through `sh -c`; they come from the user's config.                                                                                                                                                                                  | Accepted                 |
| `unsafe`             | One block (borrowing the wakeup socket's fd) went away with the mailbox rewrite; both crates now `#![forbid(unsafe_code)]`.                                                                                                                                                | Fixed (af2f8db, f0e76cc) |
| Dependencies         | `cargo deny check`: advisories, bans, licenses, sources OK. CI ran only the licence check; it now runs all four.                                                                                                                                                           | Fixed (f0e76cc)          |

## 3. Bugs

| Area                       | Finding                                                                                                                                                                                                                                                                                                                                               | Severity | Status          |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | --------------- |
| Buffer leak                | Every repaint created a new `wl_shm` pool and buffer and never destroyed them (dropping a wayland-rs proxy does not destroy it). The compositor kept each one: system `Shmem` grew 222 MB per 1000 hover changes, 89 MB per 400. Buffers are destroyed on `release` now; growth is 0.                                                                 | high     | Fixed (dd2be31) |
| Scroll multiplied          | One wheel notch ran `on_scroll_*` once per axis event: 4 runs on one notch with a hi-res wheel, 30 for a short touchpad swipe. Scrolls now count 15-unit steps per pointer frame: 1 notch → 1 run, 2 notches → 2, a 90-unit touchpad swipe → 6. The example workspace scripts lose their `flock` debounce.                                            | high     | Fixed (bfc51aa) |
| Lost updates               | Script lines went through bounded channels with `try_send`, which drops the newest message when full: a 3000-line burst left the bar on "n2985" instead of the last line. Updates now go into a mailbox that keeps the latest one per module; the loop applies them all and repaints once. The burst ends on its last line.                           | high     | Fixed (af2f8db) |
| Orphaned scripts           | abar did not handle `SIGTERM` and never stopped its scripts: after the bar exited, a script's `sleep 777` lived on, re-parented to init (so did `socat` / `trayctl subscribe` in real setups, after every bar restart). Scripts run in their own process group; on `SIGTERM`/`SIGINT`/`SIGHUP`, compositor close or error every group gets `SIGTERM`. | high     | Fixed (117916a) |
| Submenu kept focus         | Submenus were overlay layer surfaces with exclusive keyboard focus. Clicking elsewhere left them open, and every key typed went to the menu (verified: nothing reached an Alacritty window). They are now `xdg_popup`s with a grab: any outside click closes them, and the typed key reaches the window.                                              | high     | Fixed (c498778) |
| Restart delay              | The restart delay doubled to 30 s and never came back down, so the tray stayed empty for 30 s after each trayd restart once a few had happened. A script that ran 10 s or more now restarts after 1 s.                                                                                                                                                | medium   | Fixed (77df93b) |
| Newline in a label         | Text was laid out with line breaks: a newline in a window title (or `&#10;` in markup) grew the bar from 45 to 70 px and pushed every window down. Control characters are replaced before shaping.                                                                                                                                                    | medium   | Fixed (237bb8d) |
| Clock                      | The clock slept until the next minute: `%S` showed frozen seconds, and after a suspend the pre-suspend time could stay up to a minute. Only `formats[0]` was ever used, although README and CHANGELOG promise rotation. It now checks every second, posts only on change, and right click cycles formats.                                             | medium   | Fixed (bf115b9) |
| Spurious exit              | A Wayland read that found no complete message (`EAGAIN`) ended the bar with "Resource temporarily unavailable"; it happened reliably when a popup was dismissed. The read is retried.                                                                                                                                                                 | medium   | Fixed (11cf32b) |
| Script `icon` ignored      | EXEC.md documents an `icon` field for module scripts; it was parsed and dropped. It is now shown before the text, or alone when the text is empty, and left out when it can't be found.                                                                                                                                                               | medium   | Fixed (4b4ae16) |
| Theme `[workspaces]`       | `active_color`, `inactive_color` and `visibility_mode` did nothing: the example scripts read `ABAR_WS_*_COLOR`, which nothing set, and the formatter meant for them was never called. The script now gets `ABAR_WS_ACTIVE_COLOR`, `ABAR_WS_INACTIVE_COLOR` and `ABAR_WS_VISIBILITY`; the sway, Hyprland and niri scripts honour all three.            | medium   | Fixed (e17fb59) |
| Config errors              | A config or theme with one typo was replaced by the defaults (an empty bar), with only a log line. Unknown keys were ignored silently, including in module tables (swallowed by the flattened event handlers). abar now exits with file, line and column; unknown keys are warnings; a missing `--config` file is an error.                           | medium   | Fixed (3ac193b) |
| Tray status                | One item with a status other than Active, Passive or NeedsAttention made the whole tray line unreadable, so the tray stayed stale. Unknown statuses are shown like Active.                                                                                                                                                                            | low      | Fixed (e817f04) |
| Missing icons stay missing | An icon that wasn't found once (theme installed later, tray icon file written after the name was set) was never looked up again. Misses are retried after 10 s.                                                                                                                                                                                       | low      | Fixed (f3549d5) |
| `font_size`                | 0, negative or NaN were accepted. Must be in (0, 512].                                                                                                                                                                                                                                                                                                | low      | Fixed (3ac193b) |
| Output hot-plug            | A bar appears on a new output; 6 unplugs and 5 plugs while running: no warnings, no leaked buffers.                                                                                                                                                                                                                                                   | —        | OK              |
| Click commands             | Each `on_*` command is awaited by the runtime: 20 clicks left no zombies.                                                                                                                                                                                                                                                                             | —        | OK              |

## 4. Performance

Release builds in the headless sway, the same test config before and after.

| Measurement                                  | Before                          | After                           |
| -------------------------------------------- | ------------------------------- | ------------------------------- |
| Compositor shared memory, 400 hover repaints | +89 MB, never freed             | +0 MB                           |
| CPU for 400 hover repaints                   | 170 ms                          | 180 ms                          |
| 3000-line script burst                       | 130 ms CPU, ends on "n2985"     | 30 ms CPU, ends on "FINAL"      |
| Idle, 10 s                                   | 10 ms CPU, 17 MB RSS, 3 threads | 10 ms CPU, 17 MB RSS, 3 threads |
| Tray icon after a tray app starts            | —                               | 0.17 s (incl. Python start)     |

A label that didn't change no longer repaints (af2f8db). `%H:%M` still wakes
the bar once a minute; formats with seconds once a second.

## 5. UI usability and design

| Area           | Finding                                                                                                                                                        | Status          |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- |
| Submenus       | Close on an outside click or Esc; placed under their segment, kept on screen by the compositor; keyboard focus only while open.                                | Fixed (c498778) |
| Scrolling      | One notch, one step, on wheels, hi-res wheels and touchpads (§3).                                                                                              | Fixed (bfc51aa) |
| Clock          | Right click cycles formats, scrolling cycles timezones; seconds tick.                                                                                          | Fixed (bf115b9) |
| Script icons   | Icon + text segments (e.g. a player icon before the title).                                                                                                    | Fixed (4b4ae16) |
| Error messages | A broken config names file, line and column instead of showing an empty bar; typos in keys are named.                                                          | Fixed (3ac193b) |
| Logs           | Went to stdout with ANSI colours even into files; now stderr, coloured only on a terminal.                                                                     | Fixed (4f305bb) |
| HiDPI          | Buffers are drawn at scale 1; on a scale-2 output the compositor upscales them (blurry text). The author's monitor is scale 1, so this could not be seen here. | Left open (§8)  |

## 6. Code quality, tests and project hygiene

| Area      | Finding                                                                                                                                                                                                                                              | Status                   |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ |
| Dead code | The workspaces label formatter, click-to-workspace hit testing and display state were never called (with their tests); the tray had its own copy of the exec reader. Removed.                                                                        | Fixed (e17fb59, af2f8db) |
| Tests     | 101 → 115 tests: mailbox bursts, scroll accumulation, restart delay, process-group cleanup (own test binary), single-line text, clock cycling, icon+text layout, config errors and unknown keys (all shipped examples checked), quoting, icon cache. | Fixed                    |
| Coverage  | 49.0% → 52.7% lines. `wayland/mod.rs` (the event loop, ~1100 lines) stays at 0%: it needs a compositor, and was covered by the live runs instead.                                                                                                    | Accepted                 |
| Gates     | fmt, typos, clippy `-D warnings` at three feature levels, tests, docs: all pass on every audit commit (checked one by one, §9).                                                                                                                      | OK                       |
| Manifests | Cargo warned about `homepage` repeating `repository` and the unused `workspace.package.keywords`. `homepage` is gone; both crates inherit the keywords, trimmed to the 5 crates.io accepts (6 would fail to publish).                                | Fixed (f0e76cc)          |
| Semver    | libabar's public API changed (`run_exec_handler`, `DisplayMode::IconText`, `TrayItemStatus::Unknown`, removed workspace helpers), so the crates go to 0.3.0.                                                                                         | Fixed                    |
| Docs      | README (said theme `Inherits=` was not followed, which 0.2.0 had fixed), EXEC.md, the example config ("the panic is expected") and CHANGELOG updated.                                                                                                | Fixed (3c8359d)          |

## 7. Features compared with other bars

| Feature                                 | abar                                                          |
| --------------------------------------- | ------------------------------------------------------------- |
| One bar per output, hot-plug            | Yes                                                           |
| Modules from scripts (JSON lines)       | Yes; text, Pango-style markup, icon (since 4b4ae16)           |
| Clock with timezones and formats        | Yes (format cycling since bf115b9)                            |
| Tray                                    | Via trayd + an exec script; menus through `trayctl` and dmenu |
| Popup menus                             | Static submenus from config                                   |
| Tooltips                                | No (trayd sends tooltip text; abar doesn't show it)           |
| HiDPI / fractional scaling              | No                                                            |
| Click/scroll per module, hover feedback | Yes                                                           |
| Built-in modules (battery, network, …)  | No, by design: scripts                                        |

## 8. Left open

- **Release**: both crates are now 0.3.0 (libabar's API changed); set the date
  in CHANGELOG.md when tagging. Nothing was published or pushed. trayd's new
  version isn't released either; the tray was tested against the local build.
- **HiDPI**: honour `wl_output.scale` / `wp_fractional_scale_v1` and draw at
  the output's scale.
- **Tooltips**: show the tray's (and scripts') tooltip text on hover.
- **CI**: the full `cargo deny check` job was not run on GitHub here; watch
  its first run.
- **Event loop coverage**: `wayland/mod.rs` could be driven by tests against
  a headless compositor in CI.

## 9. Re-running the audit

```sh
# Gates (docs/ARCHITECTURE.md §8)
cargo fmt --all -- --check
typos
cargo deny check
for f in --no-default-features "" --all-features; do
  cargo clippy --workspace --all-targets $f -- -D warnings
  cargo test --workspace $f
done
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Coverage (system LLVM tools on Arch: LLVM_COV / LLVM_PROFDATA)
cargo llvm-cov --workspace --all-features --summary-only
```

A compositor to test in, without touching the running session:

```sh
WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 sway -c test-sway.conf
# then, with WAYLAND_DISPLAY set to its socket:
abar --config test-config.toml
grim -g "0,0 1280x45" bar.png
```

Each audit commit on its own (from a scratch worktree):

```sh
for c in $(git rev-list --reverse b484a0a..HEAD); do
  git checkout -q "$c" && cargo fmt --all -- --check &&
    cargo clippy --workspace --all-targets --all-features -- -D warnings &&
    cargo test --workspace --all-features || echo "FAILED: $c"
done
```
