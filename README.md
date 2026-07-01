[![Crates.io Version](https://img.shields.io/crates/v/hyprshot-rs.svg)](https://crates.io/crates/hyprshot-rs) [![Crates.io Downloads](https://img.shields.io/crates/d/hyprshot-rs.svg)](https://crates.io/crates/hyprshot-rs) [![AUR version](https://img.shields.io/aur/version/hyprshot-rs)](https://aur.archlinux.org/packages/hyprshot-rs) [![Crates.io License](https://img.shields.io/crates/l/hyprshot-rs.svg)](https://crates.io/crates/hyprshot-rs)

---

# Shot (hyprshot-rs)

<p align="center">
  <img src="img/logo.svg" alt="Hyprshot-rs logo" width="200" />
</p>

A modern, fast, and feature-rich screenshot and screen recording utility for Wayland (highly optimized for Hyprland and Sway), written in pure Rust.

Unlike original projects that use shell wrappers, `shot` compiles to a single native binary, providing instant execution, region freezing, scroll stitching, and screen recording capabilities.

## Features

- **Screenshot Capture**
  - `shot now` — Capture the current active monitor
  - `shot win` — Capture the active or a selected window (via compositor tree traversal)
  - `shot area` — Capture a selected screen region
  - `shot satty` — Capture a selected region and edit it immediately using the Satty annotation tool
  - `shot ocr` — Capture a selected region and perform OCR text recognition
  - `shot in5` / `shot in10` — Capture the active monitor after a 5 or 10-second countdown delay
- **Scrolling Screenshot (Longshot)**
  - `shot longshot` — Toggle start/stop to capture a region and vertically stitch scrolled content into a single long image
  - Employs lossless RGB video capture for intermediate frames to ensure maximum stitching quality and accuracy
- **Region Screen Recording (Record)**
  - `shot record` — Toggle start/stop to record a selected region to a modern WebM video file (`.webm` using VP9 codec)
  - A flashing neon-red selection overlay is automatically displayed to mark the recording area
  - Automatically copies the saved video path to the clipboard on completion
- **Screen Freezing**
  - Smooth interactive selection over a frozen desktop state (enabled by default, can be toggled via config)
- **Save & Clipboard**
  - Saves captures to your configured screenshots directory (defaults to `~/Pictures` for images, `~/Videos/record` for recordings)
  - Use `--clipboard-only` to copy directly to the clipboard instead of writing to disk
- **Configuration System**
  - TOML-based configuration (`~/.config/hyprshot-rs/config.toml`)
  - Persistent settings for paths, notifications, satty/ocr commands, longshot, and recording configurations

## Installation

### Via Cargo:
```bash
cargo install hyprshot-rs
```
Selector functionality is provided natively via `slurp-rs`, so no external `slurp` binary is strictly required for screenshots.

### Via AUR (Arch Linux):
```bash
yay -S hyprshot-rs
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
shot [options ..] <command>
```

### Subcommands

- Capture the active monitor:
  ```bash
  shot now
  ```

- Capture a window:
  ```bash
  shot win
  ```

- Capture a custom region:
  ```bash
  shot area
  ```

- Capture a region and edit with Satty:
  ```bash
  shot satty
  ```

- Capture a region and perform OCR:
  ```bash
  shot ocr
  ```

- Scrolling Screenshot (Longshot):
  Start capture:
  ```bash
  shot longshot
  ```
  Scroll down the target window/page, then run the command again to stop and save the stitched PNG:
  ```bash
  shot longshot
  ```

- Region Screen Recording (Record):
  Start recording:
  ```bash
  shot record
  ```
  Perform your actions, then run the command again to stop. The WebM video will be saved in `~/Videos/record/` and its path will be copied to your clipboard:
  ```bash
  shot record
  ```

---

## Configuration

Initialize default config:
```bash
shot --init-config
```

View current configuration:
```bash
shot --show-config
```

Set configuration values:
```bash
shot --set paths.screenshots_dir ~/Pictures/Screenshots
shot --set record.fps 60
shot --set record.crf 30
```

The configuration is saved in `~/.config/hyprshot-rs/config.toml`.

## License

[GPL-3.0](LICENSE.md)
