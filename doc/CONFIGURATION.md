# Hyshot Configuration

Hyshot stores its configuration at `~/.config/hyshot/config.toml`.

```bash
hyshot --init-config
hyshot --show-config
hyshot --set record.fps 60
```

## Capture and editor

```toml
[paths]
screenshots_dir = "~/Pictures/Screenshots"

[capture]
notification = true
notification_timeout = 3000
save_file = false
file_type = "png"
png_level = 6

[advanced]
freeze_on_area = true
freeze_on_annotate = false
delay_ms = 0

[editor]
active_tool = "MarkerPen"
auto_activate_default_tool = false
auto_deactivate_tool_after_draw = true
exit_after_copy = true
exit_after_save = true
```

`freeze_on_area` applies only to `hyshot area`. `freeze_on_annotate` applies to
the built-in `annotate` workflow. Saved editor tool styles are kept separately
in `~/.config/hyshot/editor-state.toml`; no Annotator config is read.

## External tools

External tools are named TOML tables. `hyshot external NAME` captures a PNG,
substitutes `{path}`, and runs the configured command. Its standard output is
copied to the clipboard and reported through Hyshot.

```toml
[external.ocr]
command = "nbocr recognize -l chinese -d v6-medium -m ~/.local/share/nbocr/models {path} -f text -t 8"
freeze = false

[external.translate]
command = "your-ocr-command {path} | trans -b -s auto -t zh-CN"
freeze = false
```

The supported placeholders are `{path}`, `{x}`, `{y}`, `{w}`, `{h}`,
`{monitor}`, and `{scale}`. Set tools from the command line with:

```bash
hyshot --set external.balabala.command "your-command {path}"
hyshot --set external.balabala.freeze false
```

## Longshot and recording

```toml
[longshot]
fps = 12
sad_threshold = 8.0
max_skip = 6
target_overlap = 0.30

[record]
hide_cursor = true
fps = 30
quality = "balanced"
audio = false
save_dir = "~/Videos/Screenrecords"
format = "mp4"
```

Recording quality is `compact`, `balanced`, or `high`. Longshot retains bounded
frame history and has explicit image-memory limits; see [ARCHITECTURE.md](ARCHITECTURE.md).
