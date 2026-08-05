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
  - Employs lossless RGB video capture for intermediate frames to ensure maximum stitching quality and accuracy
- **Region Screen Recording (Record)**
  - `hyshot record` — Toggle start/stop to record a selected region to a modern WebM video file (`.webm` using VP9 codec)
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

### Via Cargo:
```bash
cargo install hyshot
```
Selector functionality is provided natively via `slurp-rs`, so no external `slurp` binary is strictly required for screenshots.

### Via AUR (Arch Linux):
```bash
yay -S hyshot
```

### Runtime Dependencies
**Required:**
- `wl-clipboard` — for clipboard operations
- A Wayland compositor (Hyprland or Sway)

**For Record / Longshot:**
- `wf-recorder` — required to capture screen feeds for recording and stitching

---

## Usage

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
* **`delay_ms`** (integer) — Global delay before capturing in milliseconds.
  * *Default:* `0`

#### `[annotate]`
* **`command`** (string) — External command to execute for annotations when using `hyshot annotate`. `{path}` is replaced with the screenshot path.
  * *Default:* `"satty --filename {path}"`

#### `[ocr]`
* **`command`** (string) — External OCR execution command used when running `hyshot ocr`. `{path}` is replaced with the screenshot path.
  * *Default:* `"nbocr recognize -l chinese -d v6-tiny {path} -f text -t 8"`

#### `[longshot]`
* **`fps`** (integer) — Frame rate for capturing scrolling screenshot feed.
  * *Default:* `30`
* **`match_threshold`** (float) — Match threshold for stitching vertical scrolled frames (range `0.0` to `1.0`).
  * *Default:* `0.8`
* **`min_movement`** (integer) — Minimum scrolled distance in pixels to trigger next stitch step.
  * *Default:* `2`
* **`static_threshold`** (float) — Difference threshold (L1 norm) to detect static frames and stop scroll capture.
  * *Default:* `1.0`

#### `[record]`
* **`fps`** (integer) — Frame rate for screen recording.
  * *Default:* `30`
* **`crf`** (integer) — Constant Rate Factor (CRF) quality setting.
  * *Default:* `25`
* **`save_dir`** (string) — Directory to save recorded videos.
  * *Default:* `"~/Videos/record"`
* **`codec`** (string) — Video encoder codec.
  * *Default:* `"libvpx-vp9"`
* **`format`** (string) — Video format (file extension).
  * *Default:* `"webm"`
* **`hwaccel`** (string) — GPU hardware acceleration API. Options: `"none"`, `"vaapi"`, `"nvenc"`.
  * *Default:* `"none"`

## License

[GPL-3.0](LICENSE.md)
