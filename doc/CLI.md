# Hyshot CLI Reference

```bash
hyshot [OPTIONS] <COMMAND>
```

## Commands

| Command | Behavior |
| --- | --- |
| `now` | Capture the active output. |
| `win` | Capture the active or selected window. |
| `area` | Capture a selected region. |
| `annotate` | Capture a selected region and open the built-in editor. |
| `edit [IMAGE...]` | Open existing images, or the file picker without paths. |
| `external NAME` | Capture a selected region and run `[external.NAME]`. |
| `in5`, `in10` | Capture the active output after a fixed delay. |
| `longshot [--edit]` | Toggle scrolling capture and stitching. |
| `stitch INPUT [--output PATH] [--edit]` | Stitch an existing video. |
| `record` | Toggle region recording. |

## Common options

| Option | Description |
| --- | --- |
| `-o`, `--output-folder PATH` | Override the output directory. |
| `-f`, `--filename NAME` | Set a screenshot filename. |
| `-D`, `--delay SECONDS` | Delay normal screenshot capture. |
| `-s`, `--silent` | Disable notifications. |
| `-r`, `--raw` | Write image bytes to standard output. |
| `--clipboard-only` | Copy instead of saving a normal screenshot. |
| `-u`, `--upload` | Run the configured upload command after saving. |
| `-d`, `--debug` | Print diagnostic information. |

## Configuration commands

```bash
hyshot --init-config
hyshot --show-config
hyshot --config-path
hyshot --interactive
hyshot --print-binds
hyshot --set external.ocr.freeze false
```

`--no-config` uses built-in defaults and does not load `config.toml` or editor
style memory.

See [CONFIGURATION.md](CONFIGURATION.md) for the configuration model and
[HOTKEYS.md](HOTKEYS.md) for Hyprland examples.
