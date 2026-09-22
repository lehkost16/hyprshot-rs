# Configuration Guide - hyshot

Minimal configuration reference aligned with actual behavior.

## Built-in editor

The editor shares hyshot's config. It does not read or migrate
`~/.config/annotator/config.toml`.

`annotate` always uses the built-in editor. The old `[annotate]` command section
is no longer used and should be removed from existing configuration files.
`[ocr].command` remains independently configurable for nbocr.

```toml
[editor]
marker_pen_straight_mode = true
auto_activate_default_tool = false
auto_deactivate_tool_after_draw = false
active_tool = "Rectangle"
exit_after_copy = false
exit_after_save = false
```

Explicit preferences support `hyshot --set editor.exit_after_copy true` and are
never rewritten by editor interaction. `active_tool` is the configured startup
tool when `auto_activate_default_tool` is enabled, not the last selected tool.
`marker_pen_straight_mode` is likewise the explicit startup preference.

Remembered widths and RGBA colors are stored separately in
`~/.config/hyshot/editor-state.toml` on normal editor exit:

```toml
[tool_settings.Pencil]
stroke_width = 5.0
stroke_color_rgba = [255, 69, 58, 255]
```

Concurrent sessions merge changed tools under a file lock; for the same tool,
the last saved style wins. No per-frame disk access. `--no-config` uses defaults
without loading or writing style memory. Invalid TOML is an error. Old
`[editor.tool_settings.*]` entries must be moved explicitly into the state file
as `[tool_settings.*]`; they are not silently ignored or auto-migrated.

Editor Save creates a new PNG in `--output-folder`, `HYSHOT_DIR`, or
`paths.screenshots_dir`, in that order. The old editor `save_directory` is not
used. Explicit Save/Copy are separate actions, independent of capture's
automatic `save_file` policy. Notifications respect `--silent` and
`capture.notification`.

## Overview

- Config is a TOML file.
- Priority: CLI args > `HYSHOT_DIR` env > config file > defaults.
- CLI config management is documented in `doc/CLI.md`.

## Configuration File Location

Default path:

```
~/.config/hyshot/config.toml
```

Get the active path:

```bash
hyshot --config-path
```

## Configuration Structure

```toml
[paths]
[hotkeys]
[capture]
[advanced]
```

### Default Configuration (current)

```toml
[paths]
screenshots_dir = "~/Pictures"

[hotkeys]
window = "SUPER, Print"
region = "SUPER SHIFT, Print"
output = "SUPER CTRL, Print"
active_output = ", Print"

[capture]
notification = true
notification_timeout = 3000

[advanced]
freeze_on_region = true
freeze_on_external = false
delay_ms = 0
```

## Section: Paths

### `screenshots_dir`

- Directory for saved screenshots.
- Used when `--clipboard-only` is not set.
- Created if missing; must be writable.

Path expansion:
- `~` and `$HOME` are expanded.
- `$XDG_PICTURES_DIR` is expanded if available.
- Other `$VAR` are expanded if set.
- Undefined variables are left as-is.
- Relative paths stay relative (no canonicalization).

Priority for save directory:
1. `-o/--output-folder`
2. `HYSHOT_DIR`
3. `paths.screenshots_dir`
4. `~/Pictures`

## Section: Hotkeys

These values are **only for Hyprland config generation and the hotkey wizard**.
They do not change runtime behavior by themselves.

For working examples, see `doc/HOTKEYS.md`.

## Section: Capture

### `notification`

- When `true`, a desktop notification is attempted after capture.
- Notification failures are logged but do not abort the capture.
- `--silent` forces notifications off.

### `notification_timeout`

- Timeout for notifications in milliseconds.

## Section: Advanced

### `freeze_on_region`

- Enables `--freeze` by default.
- Applies to normal region capture.
- If the compositor lacks required Wayland protocols, freeze is skipped with a warning.

### `freeze_on_external`

- Enables freeze by default for `annotate` and `ocr`.
- Defaults to `false` so external tools receive the sharpest direct screenshot on fractional-scale displays.

### `delay_ms`

- Delay before capture in milliseconds.

## Managing Configuration

See `doc/CLI.md` for:
- `--init-config`
- `--show-config`
- `--config-path`
- `--set`
- `--no-config`
