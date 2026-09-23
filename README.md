# Hyshot

Hyshot is a GPL-3.0 Wayland capture tool maintained by shiyuqi. It combines
screenshots, a built-in image editor, scrolling screenshots, region recording,
OCR, and configurable external tools in one native Rust application for
Hyprland and Sway.

This project is a derivative work of `hyprshot-rs` and `annotator`. Their
copyright notices and GPL-3.0 terms are preserved; see [NOTICE.md](NOTICE.md).

## Features

- Capture an output, window, or selected region.
- Edit captures and existing images in the built-in Hyshot editor.
- Create scrolling screenshots by recording and stitching a selected region.
- Record a selected region to MP4, WebM, MKV, or GIF.
- Run configurable external tools on a selected PNG, including OCR and
  translation.
- Keep capture, editor, recording, and external-tool settings in
  `~/.config/hyshot/config.toml`.

## Runtime Dependencies

- A Wayland compositor, tested with Hyprland and Sway.
- `wl-clipboard` for clipboard operations.
- `wl-screenrec` and `ffmpeg` for recording and long screenshots.
- `nbocr` only when configuring OCR.
- Translate Shell (`trans`) only when configuring translation.

## Build

Use the repository toolchain:

```bash
cargo build --release
install -m 755 target/release/hyshot ~/.local/bin/hyshot
```

## Commands

```bash
hyshot now                         # Active output
hyshot win                         # Active or selected window
hyshot area                        # Selected region
hyshot annotate                    # Selected region in the built-in editor
hyshot edit image.png              # Open an existing image in the editor
hyshot longshot                    # Start/stop scrolling screenshot capture
hyshot longshot --edit             # Open the completed long screenshot
hyshot record                      # Start/stop region recording
hyshot external ocr                # Run configured OCR on a selected region
hyshot external translate          # Run configured translation on a region
```

`annotate` captures once and transfers original pixels to the built-in editor.
`edit` opens an existing image and never captures the desktop. The desktop entry
opens images with `hyshot edit %F`; no separate Annotator binary or configuration
is required.

## Configuration

Hyshot reads `~/.config/hyshot/config.toml`.

```bash
hyshot --init-config
hyshot --show-config
hyshot --interactive
hyshot --set paths.screenshots_dir ~/Pictures/Screenshots
hyshot --set record.fps 60
hyshot --set external.ocr.freeze false
```

### Capture and editor

```toml
[capture]
notification = true
save_file = false
file_type = "png"
png_level = 6

[advanced]
freeze_on_area = true
freeze_on_annotate = false

[editor]
active_tool = "MarkerPen"
exit_after_copy = true
exit_after_save = true
```

`freeze_on_area` controls normal area screenshots. `freeze_on_annotate` controls
the built-in editor capture. Each external tool has its own `freeze` setting.

### External tools

Each external tool gets a command and a selection-freeze setting. Hyshot creates
a temporary PNG and replaces `{path}` in the command. Standard output is copied
to the clipboard, shown in a notification, and printed when Hyshot is run from a
terminal.

```toml
[external.ocr]
command = "nbocr recognize -l chinese -d v6-medium -m ~/.local/share/nbocr/models {path} -f text -t 8"
freeze = false

[external.translate]
command = "nbocr recognize -l chinese -d v6-medium -m ~/.local/share/nbocr/models {path} -f text -t 8 2>/dev/null | sed -E '/^(CPU Group:|The device )/d; s/^\\[[0-9]+\\] //; s/ \\([0-9]+%\\)$//' | trans -b -s auto -t en -no-ansi -no-warn"
freeze = false
```

The translation example sends recognized text to Translate Shell's configured
online translation engine. Do not configure it for sensitive text unless that
data flow is acceptable.

### Longshot and recording

```toml
[longshot]
fps = 12
sad_threshold = 8.0
max_skip = 6
target_overlap = 0.30

[record]
hide_cursor = true
fps = 30
quality = "balanced" # compact, balanced, high
audio = false
save_dir = "~/Videos/Screenrecords"
format = "mp4"
```

## Desktop entry

Install `resources/site.nullable.annotator.desktop` to open supported image files
with `hyshot edit`. The retained desktop ID avoids breaking existing file
associations while the visible application name is Hyshot.

## License

Hyshot is distributed under [GPL-3.0](LICENSE.md). When distributing binaries,
make the corresponding source available under the same license.
