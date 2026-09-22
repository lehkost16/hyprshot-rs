# CLI Reference - hyshot

This document lists CLI flags and mode semantics. For an overview and general examples, see [README.md](../README.md).

## Basic Usage

```bash
hyshot [OPTIONS] -m <MODE> [-- COMMAND]
```

## Editor And Longshot

```bash
hyshot annotate
hyshot edit image.png
hyshot longshot --edit
# Scroll, then stop; the saved session remembers --edit:
hyshot longshot
hyshot stitch recording.mp4 --output result.png --edit
```

`--edit` is optional. Longshot/stitch always save the PNG before opening the
built-in editor. If the editor cannot open the image, the saved PNG remains.
Width/height/scale overrides are not supported: video dimensions come from
ffprobe. Existing output files are never overwritten.

Recording and longshot require XDG_RUNTIME_DIR and Linux pidfd support. A failed
conversion retains the raw recording and session state; invoking the same toggle
again retries finalization. Do not delete state while a capture is running. Runtime
state is not migrated from older builds: stop existing recordings before switching
versions. Session locks reject concurrent toggles instead of spawning duplicates.

## Screenshot Mode Selection

Specify one or more modes with `-m` / `--mode`:

| Mode          | Description                                                             | Example                           |
| ------------- | ----------------------------------------------------------------------- | --------------------------------- |
| `region`      | Select a region with your mouse                                         | `hyshot -m region`           |
| `window`      | Select a window                                                         | `hyshot -m window`           |
| `output`      | Select a monitor                                                        | `hyshot -m output`           |
| `active`      | Modifier: capture active window/monitor (use with `output` or `window`) | `hyshot -m window -m active` |
| `OUTPUT_NAME` | Capture specific monitor by name                                        | `hyshot -m output -m DP-1`   |

Notes:
- `active` must be combined with `output` or `window`.
- If multiple base modes are provided (`output`, `window`, `region`), the last one wins.
- `region` and `output` are Wayland‑wide (no `hyprctl` required).
- `window`/`active` are supported on Hyprland and Sway only.

To list available monitor names (Hyprland):

```bash
hyprctl monitors
```

## Options

### Output Options

| Option             | Short | Description                      | Example                |
| ------------------ | ----- | -------------------------------- | ---------------------- |
| `--output-folder`  | `-o`  | Directory to save screenshot     | `-o ~/Screenshots`     |
| `--filename`       | `-f`  | Custom filename                  | `-f my_screenshot.png` |
| `--raw`            | `-r`  | Output raw PNG to stdout         | `-r > output.png`      |
| `--clipboard-only` |       | Copy to clipboard without saving | `--clipboard-only`     |

`--raw` disables saving, clipboard copy, and notifications.

### Capture Options

| Option     | Short | Description                           | Example |
| ---------- | ----- | ------------------------------------- | ------- |
| `--delay`  | `-D`  | Delay before capture (seconds)        | `-D 3`  |
| `--freeze` |       | Freeze screen during capture/selection | `--freeze` |

Note: `--freeze` does not require extra tools. If the compositor doesn't support freeze, it will be skipped.

### Notification Options

| Option            | Short | Description               | Example   |
| ----------------- | ----- | ------------------------- | --------- |
| `--silent`        | `-s`  | Don't show notifications  | `-s`      |
| `--notif-timeout` | `-n`  | Notification timeout (ms) | `-n 5000` |

### Other Options

| Option    | Short | Description             |
| --------- | ----- | ----------------------- |
| `--debug` | `-d`  | Print debug information |
| `--help`  | `-h`  | Show help message       |

## Configuration Commands

- `--init-config` initializes a default config file.
- `--show-config` prints the current config.
- `--config-path` prints the config file path.
- `--set KEY VALUE` updates a single config value.
- `--no-config` disables loading the config file.

For configuration fields, defaults, and path expansion details, see [CONFIGURATION.md](CONFIGURATION.md).

## Hyprland Integration Commands

- `--generate-hyprland-config` prints keybindings for Hyprland.
- `--install-binds` installs keybindings into `hyprland.conf` (creates a backup).
- `--with-clipboard` adds clipboard-only bindings (used with the two commands above).
- `--setup-hotkeys` runs the interactive hotkey wizard.

## Post-Capture Command

Run a command after capture:

```bash
hyshot -m region -- <command>
```

The command is only executed when a file is saved (not with `--raw` or `--clipboard-only`).

## See Also

- [README.md](../README.md) - Project overview and general examples
- [CONFIGURATION.md](CONFIGURATION.md) - Configuration reference
- [HOTKEYS.md](HOTKEYS.md) - Keybinding setup and examples
