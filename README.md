---

# Hyshot (hyshot)

<p align="center">
  <img src="img/logo.svg" alt="Hyshot logo" width="200" />
</p>

A modern, fast, and feature-rich screenshot and screen recording utility for Wayland (highly optimized for Hyprland and Sway), written in pure Rust.

Unlike original projects that use shell wrappers, `hyshot` compiles to a single native binary, providing instant execution, region freezing, scroll stitching, and screen recording capabilities.

## Features

- **Screenshot Capture**
  - `hyshot now` — Capture the current active monitor
  - `hyshot win` — Capture the active or a selected window (via compositor tree traversal)
  - `hyshot area` — Capture a selected screen region
  - `hyshot annotate` — Capture a selected region and open it immediately using the configured annotation tool
  - `hyshot ocr` — Capture a selected region and perform OCR text recognition
  - `hyshot in5` / `hyshot in10` — Capture the active monitor after a 5 or 10-second countdown delay
- **Scrolling Screenshot (Longshot)**
  - `hyshot longshot` — Toggle start/stop to capture a region and vertically stitch scrolled content into a single long image
  - Uses a lower frame-rate recording tuned for scrolling, then stitches sampled frames into one image
- **Region Screen Recording (Record)**
  - `hyshot record` — Toggle start/stop to record a selected region to WebM, MP4, GIF, or MKV
  - A flashing neon-red selection overlay is automatically displayed to mark the recording area
  - Automatically copies the saved video path to the clipboard on completion
- **Screen Freezing**
  - Smooth interactive selection over a frozen desktop state (enabled by default, can be toggled via config)
- **Save & Clipboard**
  - Saves captures to your configured screenshots directory (defaults to `~/Pictures` for images, `~/Videos/record` for recordings)
  - Use `--clipboard-only` to copy directly to the clipboard instead of writing to disk
- **Configuration System**
  - TOML-based configuration (`~/.config/hyshot/config.toml`)
  - Persistent settings for paths, notifications, annotate/ocr commands, longshot, and recording configurations

## Installation


```

### Runtime Dependencies
**Required:**
- `wl-clipboard` — for clipboard operations
- A Wayland compositor (Hyprland or Sway)

**For Record / Longshot:**
- `wl-screenrec` — required to capture screen feeds for recording and stitching

---

## Usage

### Integrated annotation

Build with the repository's nightly Rust toolchain:

```bash
cargo build --release
hyshot --set annotate.command builtin
hyshot annotate
hyshot edit /path/to/image.png
```

`hyshot annotate` captures a region and opens the built-in editor when
`annotate.command = "builtin"`. `hyshot edit` opens existing images (or a
file picker with no paths), without taking another screenshot. The editor
retains the toolbar expand/collapse control. Preferences now live in hyshot's
`[editor]` section; `~/.config/annotator/config.toml` is not read or auto-migrated.
Edited images follow hyshot's screenshot output-directory rules.
Existing external annotation commands remain supported.

The editor is the `hyshot-editor` library in `crates/editor`, not a separately
launched executable. Captured PNG data is passed in memory. Chinese fonts are
resolved at runtime with fontconfig (`fc-match`); install a CJK font on the target
system. See [Architecture](doc/ARCHITECTURE.md) for module boundaries.

### Command Syntax
```bash
hyshot [options ..] <command>
```

### Subcommands

- Capture the active monitor:
  ```bash
  hyshot now
  ```

- Capture a window:
  ```bash
  hyshot win
  ```

- Capture a custom region:
  ```bash
  hyshot area
  ```

- Capture a region and open in annotation tool:
  ```bash
  hyshot annotate
  ```

- Capture a region and perform OCR:
  ```bash
  hyshot ocr
  ```

- Scrolling Screenshot (Longshot):
  Start capture:
  ```bash
  hyshot longshot
  ```
  Scroll down the target window/page, then run the command again to stop and save the stitched PNG:
  ```bash
  hyshot longshot
  ```

- Region Screen Recording (Record):
  Start recording:
  ```bash
  hyshot record
  ```
  Perform your actions, then run the command again to stop. The WebM video will be saved in `~/Videos/record/` and its path will be copied to your clipboard:
  ```bash
  hyshot record
  ```

---

## Configuration

The configuration file is located at `~/.config/hyshot/config.toml`. You can initialize a default configuration, display the current configuration, or edit values.

### Commands

- **Initialize default configuration**:
  ```bash
  hyshot --init-config
  ```

- **Show current configuration**:
  ```bash
  hyshot --show-config
  ```

- **Launch interactive configuration menu**:
  ```bash
  hyshot -i
  # or
  hyshot --interactive
  ```

- **Set a configuration value**:
  ```bash
  hyshot --set <key> <value>
  ```
  Example:
  ```bash
  hyshot --set paths.screenshots_dir ~/Pictures/Screenshots
  hyshot --set capture.jpeg_quality 95
  hyshot --set record.fps 60
  ```

---

### Configuration Reference

Here is a complete list of all available configuration sections and options:

#### `[paths]`
* **`screenshots_dir`** (string) — Directory where screenshots will be saved.
  * *Default:* `"~/Pictures"`

#### `[capture]`
* **`notification`** (boolean) — Show system notifications after screen capture.
  * *Default:* `true`
* **`notification_timeout`** (integer) — Notification display duration in milliseconds.
  * *Default:* `3000`
* **`save_file`** (boolean) — Whether to save screenshots to disk by default. If `false`, copies to clipboard only.
  * *Default:* `true`
* **`file_type`** (string) — Output format for screen captures. Options: `"png"`, `"jpeg"`, or `"ppm"`.
  * *Default:* `"png"`
* **`jpeg_quality`** (integer) — Quality of JPEG captures (from `0` to `100`).
  * *Default:* `100` (max quality)
* **`png_level`** (integer) — PNG zlib compression level (from `0` to `9`). Higher values take more CPU but yield smaller files.
  * *Default:* `6`

#### `[advanced]`
* **`freeze_on_region`** (boolean) — Freeze the desktop screen during region selection.
  * *Default:* `true`
* **`freeze_on_external`** (boolean) — Freeze the desktop for `annotate` and `ocr` region selection.
  * *Default:* `false`
* **`delay_ms`** (integer) — Global delay before capturing in milliseconds.
  * *Default:* `0`

#### `[annotate]`
* **`command`** (string) — `builtin` selects the integrated editor. Other values explicitly select an external command, with `{path}` replaced by the screenshot path.
  * *Default:* `"builtin"` opens the integrated annotator. Existing custom commands remain supported.

#### `[ocr]`
* **`command`** (string) — External OCR execution command used when running `hyshot ocr`. `{path}` is replaced with the screenshot path.
  * *Default:* `"nbocr recognize -l chinese -d v6-tiny {path} -f text -t 8"`

#### `[longshot]`
* **`fps`** (integer) — Frame rate for capturing scrolling screenshot feed.
  * *Default:* `12`
* **`sad_threshold`** (float) — Column match threshold for stitching; lower is stricter.
  * *Default:* `8.0`
* **`max_skip`** (integer) — Maximum frames to skip when scroll velocity is high.
  * *Default:* `6`
* **`target_overlap`** (float) — Target overlap between matched frames.
  * *Default:* `0.30`

#### `[record]`
* **`fps`** (integer) — Frame rate for screen recording.
  * *Default:* `30`
* **`quality`** (string) — Simple quality preset: `"compact"`, `"balanced"`, or `"high"`.
  * *Default:* `"balanced"`
* **`audio`** (boolean) — Record the default audio source.
  * *Default:* `false`
* **`save_dir`** (string) — Directory to save recorded videos.
  * *Default:* `"~/Videos/record"`
* **`format`** (string) — Video format (file extension).
  * *Default:* `"webm"`

## License

[GPL-3.0](LICENSE.md)
