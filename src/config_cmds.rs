use anyhow::{Context, Result};
use dialoguer::{Confirm, Input, Select};

use crate::config;

pub fn handle_init_config() -> Result<()> {
    let config_path = config::Config::config_path()?;

    if config_path.exists() {
        println!("Config file already exists at: {}", config_path.display());
        println!("Use --show-config to view current configuration");
        return Ok(());
    }

    let config = config::Config::default();
    config.save().context("Failed to save config file")?;

    println!("Config file created at: {}", config_path.display());
    println!("\nDefault configuration:");
    println!("Screenshots directory: {}", config.paths.screenshots_dir);
    println!("\nYou can edit this file manually or use:");
    println!("hyshot --set KEY VALUE");
    println!("\nExample:");
    println!("hyshot --set paths.screenshots_dir ~/Documents/Screenshots");

    Ok(())
}

pub fn handle_show_config() -> Result<()> {
    let config = config::Config::load().context("Failed to load config")?;
    let config_path = config::Config::config_path()?;

    println!("Configuration file: {}", config_path.display());
    println!(
        "\n{}",
        toml::to_string_pretty(&config).context("Failed to serialize config")?
    );

    Ok(())
}

pub fn handle_config_path() -> Result<()> {
    let config_path = config::Config::config_path()?;
    println!("{}", config_path.display());
    Ok(())
}

pub fn handle_set_config(args: &[String]) -> Result<()> {
    if args.len() != 2 {
        return Err(anyhow::anyhow!(
            "--set requires exactly 2 arguments: KEY VALUE"
        ));
    }

    let key = &args[0];
    let value = &args[1];

    let mut config = if config::Config::exists() {
        config::Config::load().context("Failed to load config")?
    } else {
        println!("Config file doesn't exist, creating new one...");
        config::Config::default()
    };

    set_config_value(&mut config, key, value)?;
    config.record.sanitize();

    config.save().context("Failed to save config")?;

    let config_path = config::Config::config_path()?;
    println!("Configuration updated: {} = {}", key, value);
    println!("Config file: {}", config_path.display());

    Ok(())
}

fn set_config_value(config: &mut config::Config, key: &str, value: &str) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();

    if parts.len() == 3 && parts[0] == "external" {
        let tool = config.external.entry(parts[1].to_string()).or_insert_with(|| config::ExternalToolConfig { command: String::new(), freeze: false });
        match parts[2] {
            "command" => tool.command = value.to_string(),
            "freeze" => tool.freeze = value.parse().context("Value must be 'true' or 'false'")?,
            _ => return Err(anyhow::anyhow!("Unknown external tool field: {}", parts[2])),
        }
        return Ok(());
    }
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "Invalid key format. Expected 'section.field', got '{}'",
            key
        ));
    }

    let section = parts[0];
    let field = parts[1];

    match (section, field) {
        // [paths] section
        ("paths", "screenshots_dir") => {
            config.paths.screenshots_dir = value.to_string();
        }

        // [capture] section
        ("capture", "notification") => {
            config.capture.notification =
                value.parse().context("Value must be 'true' or 'false'")?;
        }
        ("capture", "notification_timeout") => {
            config.capture.notification_timeout = value
                .parse()
                .context("Value must be a number (milliseconds)")?;
        }
        ("capture", "save_file") => {
            config.capture.save_file = value.parse().context("Value must be 'true' or 'false'")?;
        }
        ("capture", "upload_command") => {
            config.capture.upload_command = value.to_string();
        }

        // [advanced] section
        ("advanced", "freeze_on_area") => {
            config.advanced.freeze_on_area =
                value.parse().context("Value must be 'true' or 'false'")?;
        }
        ("advanced", "freeze_on_external") => {
            config.advanced.freeze_on_external =
                value.parse().context("Value must be 'true' or 'false'")?;
        }
        ("advanced", "delay_ms") => {
            config.advanced.delay_ms = value
                .parse()
                .context("Value must be a number (milliseconds)")?;
        }

        ("editor", field) => {
            let mut settings = toml::Value::try_from(&config.editor)?;
            let slot = settings
                .get_mut(field)
                .with_context(|| format!("Unknown editor setting: {field}"))?;
            *slot = match slot {
                toml::Value::Boolean(_) => {
                    toml::Value::Boolean(value.parse().context("Value must be 'true' or 'false'")?)
                }
                toml::Value::String(_) => toml::Value::String(value.to_owned()),
                _ => anyhow::bail!("Edit per-tool styles through the editor or TOML configuration"),
            };
            config.editor = settings.try_into()?;
        }

        // [longshot] section
        ("longshot", "fps") => {
            config.longshot.fps = value.parse().context("Value must be a positive integer")?;
        }
        ("longshot", "sad_threshold") => {
            config.longshot.sad_threshold =
                value.parse().context("Value must be a float (e.g. 8.0)")?;
        }
        ("longshot", "max_skip") => {
            config.longshot.max_skip =
                value.parse().context("Value must be an integer (e.g. 6)")?;
        }
        ("longshot", "target_overlap") => {
            config.longshot.target_overlap =
                value.parse().context("Value must be a float (e.g. 0.30)")?;
        }

        // [record] section
        ("record", "hide_cursor") => {
            config.record.hide_cursor = value.parse().context("Value must be 'true' or 'false'")?;
        }
        ("record", "fps") => {
            config.record.fps = value.parse().context("Value must be a positive integer")?;
        }
        ("record", "quality") => {
            config.record.quality = config::normalize_record_quality(value);
        }
        ("record", "audio") => {
            config.record.audio = value.parse().context("Value must be 'true' or 'false'")?;
        }
        ("record", "crf") => {
            config.record.crf = value
                .parse()
                .context("Value must be a positive integer (0-63)")?;
        }
        ("record", "save_dir") => {
            config.record.save_dir = value.to_string();
        }
        ("record", "codec") => {
            config.record.codec = value.to_string();
        }
        ("record", "format") => {
            config.record.format = config::normalize_record_format(value);
        }
        ("record", "hwaccel") => {
            config.record.hwaccel = value.to_string();
        }
        ("record", "command_args") => {
            if let Ok(args) = serde_json::from_str::<Vec<String>>(value) {
                config.record.command_args = args;
            } else {
                config.record.command_args = value.split_whitespace().map(String::from).collect();
            }
        }

        _ => {
            return Err(anyhow::anyhow!(
                "Unknown config key: {}.{}\n\nAvailable keys:\n\
                 Paths:\n\
                   - paths.screenshots_dir\n\
                 Capture:\n\
                   - capture.notification (true, false)\n\
                   - capture.notification_timeout (milliseconds)\n\
                   - capture.save_file (true, false)\n\
                 Advanced:\n\
                   - advanced.freeze_on_area (true, false)\n\
                   - advanced.freeze_on_external (true, false)\n\
                   - advanced.delay_ms (milliseconds)\n\
                  Annotate:\n\
                  OCR:\n\
                    - ocr.command\n\
                 Longshot:\n\
                   - longshot.fps (integer)\n\
                   - longshot.sad_threshold (float)\n\
                   - longshot.max_skip (integer)\n\
                   - longshot.target_overlap (float)\n\
                 Record:\n\
                   - record.fps (integer)\n\
                   - record.quality (compact, balanced, high)\n\
                   - record.audio (true, false)\n\
                   - record.save_dir (string)\n\
                   - record.format (webm, mp4, gif, mkv)\n\
                 Advanced record compatibility:\n\
                   - record.crf (deprecated integer, retained for old configs)\n\
                   - record.codec (string, e.g. vp9, avc)\n\
                   - record.hwaccel (string, e.g. none, vaapi, nvenc)\n\
                   - record.command_args (array or space-separated string)",
                section,
                field
            ));
        }
    }

    Ok(())
}

pub fn handle_print_binds() -> Result<()> {
    println!(
        r#"============================================================
Hyprland Keyboard Shortcut Binds for Hyshot
============================================================
Copy and paste the following lines into your ~/.config/hypr/hyprland.conf:

# --- Screenshot Binds ---
# Capture active monitor
bind = , Print, exec, hyshot now
# Capture selected window
bind = ALT, Print, exec, hyshot win
# Capture custom region (Area)
bind = SHIFT, Print, exec, hyshot area
# Capture region and open in annotation tool
bind = SUPER, Print, exec, hyshot annotate
# Capture region and perform OCR text recognition
bind = SUPER SHIFT, Print, exec, hyshot ocr

# --- Delay Binds ---
# Capture monitor after 5 seconds delay
bind = CTRL, Print, exec, hyshot in5
# Capture monitor after 10 seconds delay
bind = CTRL SHIFT, Print, exec, hyshot in10

# --- Scrolling & Recording Binds ---
# Toggle scrolling screenshot (Longshot)
bind = SUPER, L, exec, hyshot longshot
# Toggle region screen recording (Record)
bind = SUPER, R, exec, hyshot record
============================================================
"#
    );
    Ok(())
}

pub fn handle_interactive_config() -> Result<()> {
    let mut config = if config::Config::exists() {
        config::Config::load().context("Failed to load config")?
    } else {
        println!("Config file doesn't exist, initializing default config...");
        config::Config::default()
    };

    loop {
        let sections = &[
            "[paths]     - Paths (screenshots save directory)",
            "[capture]   - Capture settings (notifications, format, quality)",
            "[record]    - Screen recording settings (fps, quality, format, audio)",
            "[longshot]  - Longshot stitching settings (fps, match threshold)",
            "[advanced]  - Advanced settings (delay, screen freeze)",
            "[tools]     - External tools (annotate, OCR commands)",
            "Save & Exit",
            "Exit without saving",
        ];

        println!("\n--- hyshot Interactive Configuration ---");
        let selection = Select::new()
            .with_prompt("Select a section to configure")
            .default(0)
            .items(sections)
            .interact()?;

        match selection {
            0 => configure_paths(&mut config)?,
            1 => configure_capture(&mut config)?,
            2 => configure_record(&mut config)?,
            3 => configure_longshot(&mut config)?,
            4 => configure_advanced(&mut config)?,
            5 => configure_tools(&mut config)?,
            6 => {
                config.save().context("Failed to save config")?;
                println!("Configuration saved successfully!");
                break;
            }
            7 => {
                println!("Exited without saving.");
                break;
            }
            _ => unreachable!(),
        }
    }

    Ok(())
}

fn configure_paths(config: &mut config::Config) -> Result<()> {
    loop {
        let fields = &[
            &format!(
                "screenshots_dir (current: {})",
                config.paths.screenshots_dir
            ),
            "< Back to main menu",
        ];

        let selection = Select::new()
            .with_prompt("Select path field to edit")
            .default(0)
            .items(fields)
            .interact()?;

        if selection == 1 {
            break;
        }

        let input: String = Input::new()
            .with_prompt("Enter screenshots directory path")
            .default(config.paths.screenshots_dir.clone())
            .interact_text()?;
        config.paths.screenshots_dir = input;
    }
    Ok(())
}

fn configure_capture(config: &mut config::Config) -> Result<()> {
    loop {
        let fields = &[
            &format!("notification (current: {})", config.capture.notification),
            &format!(
                "notification_timeout (current: {} ms)",
                config.capture.notification_timeout
            ),
            &format!("save_file (current: {})", config.capture.save_file),
            &format!("file_type (current: {})", config.capture.file_type),
            &format!("jpeg_quality (current: {})", config.capture.jpeg_quality),
            &format!("png_level (current: {})", config.capture.png_level),
            &format!(
                "upload_command (current: {})",
                config.capture.upload_command
            ),
            "< Back to main menu",
        ];

        let selection = Select::new()
            .with_prompt("Select capture field to edit")
            .default(0)
            .items(fields)
            .interact()?;

        match selection {
            0 => {
                config.capture.notification = Confirm::new()
                    .with_prompt("Enable notifications?")
                    .default(config.capture.notification)
                    .interact()?;
            }
            1 => {
                config.capture.notification_timeout = Input::new()
                    .with_prompt("Notification timeout (ms)")
                    .default(config.capture.notification_timeout)
                    .interact_text()?;
            }
            2 => {
                config.capture.save_file = Confirm::new()
                    .with_prompt("Save screenshots to disk by default?")
                    .default(config.capture.save_file)
                    .interact()?;
            }
            3 => {
                let types = &["png", "jpeg", "ppm"];
                let idx = types
                    .iter()
                    .position(|&t| t == config.capture.file_type)
                    .unwrap_or(0);
                let selected_type = Select::new()
                    .with_prompt("Choose image format")
                    .default(idx)
                    .items(types)
                    .interact()?;
                config.capture.file_type = types[selected_type].to_string();
            }
            4 => {
                config.capture.jpeg_quality = Input::new()
                    .with_prompt("JPEG quality (0-100)")
                    .default(config.capture.jpeg_quality)
                    .interact_text()?;
            }
            5 => {
                config.capture.png_level = Input::new()
                    .with_prompt("PNG compression level (0-9)")
                    .default(config.capture.png_level)
                    .interact_text()?;
            }
            6 => {
                config.capture.upload_command = Input::new()
                    .with_prompt("Upload command (e.g. curl -F 'file=@{path}' https://tmp.link/)")
                    .default(config.capture.upload_command.clone())
                    .interact_text()?;
            }
            7 => break,
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn configure_record(config: &mut config::Config) -> Result<()> {
    loop {
        let fields = &[
            &format!("hide_cursor (current: {})", config.record.hide_cursor),
            &format!("audio (current: {})", config.record.audio),
            &format!("fps (current: {})", config.record.fps),
            &format!("quality (current: {})", config.record.quality),
            &format!("format (current: {})", config.record.format),
            &format!("save_dir (current: {})", config.record.save_dir),
            "< Back to main menu",
        ];

        let selection = Select::new()
            .with_prompt("Select record field to edit")
            .default(0)
            .items(fields)
            .interact()?;

        match selection {
            0 => {
                let options = &[
                    "false (show cursor in video)",
                    "true (hide cursor in video)",
                ];
                let idx = if config.record.hide_cursor { 1 } else { 0 };
                let selected = Select::new()
                    .with_prompt("Hide cursor during screen recording?")
                    .default(idx)
                    .items(options)
                    .interact()?;
                config.record.hide_cursor = selected == 1;
            }
            1 => {
                let options = &["false (no audio)", "true (record default audio source)"];
                let idx = if config.record.audio { 1 } else { 0 };
                let selected = Select::new()
                    .with_prompt("Record audio?")
                    .default(idx)
                    .items(options)
                    .interact()?;
                config.record.audio = selected == 1;
            }
            2 => {
                config.record.fps = Input::new()
                    .with_prompt("Recording FPS")
                    .default(config.record.fps)
                    .interact_text()?;
            }
            3 => {
                let qualities = &["compact", "balanced", "high"];
                let idx = qualities
                    .iter()
                    .position(|&q| q == config.record.quality)
                    .unwrap_or(1);
                let selected_quality = Select::new()
                    .with_prompt("Choose recording quality")
                    .default(idx)
                    .items(qualities)
                    .interact()?;
                config.record.quality = qualities[selected_quality].to_string();
            }
            4 => {
                let formats = &["webm", "mp4", "gif", "mkv"];
                let idx = formats
                    .iter()
                    .position(|&f| f == config.record.format)
                    .unwrap_or(0);
                let selected_fmt = Select::new()
                    .with_prompt("Choose video format")
                    .default(idx)
                    .items(formats)
                    .interact()?;

                let new_fmt = formats[selected_fmt].to_string();
                config.record.format = new_fmt.clone();

                if new_fmt == "mp4" {
                    config.record.codec = "avc".to_string();
                    println!("Applied MP4 preset: codec=avc");
                } else if new_fmt == "webm" {
                    config.record.codec = "vp9".to_string();
                    println!("Applied WebM preset: codec=vp9");
                } else if new_fmt == "gif" {
                    config.record.codec = "avc".to_string();
                    println!("Applied GIF intermediate preset: codec=avc");
                } else if new_fmt == "mkv" {
                    config.record.codec = "vp9".to_string();
                    println!("Applied MKV preset: codec=vp9");
                }
            }
            5 => {
                config.record.save_dir = Input::new()
                    .with_prompt("Recording save directory")
                    .default(config.record.save_dir.clone())
                    .interact_text()?;
            }
            6 => break,
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn configure_longshot(config: &mut config::Config) -> Result<()> {
    loop {
        let fields = &[
            &format!("fps (current: {})", config.longshot.fps),
            &format!("sad_threshold (current: {})", config.longshot.sad_threshold),
            &format!("max_skip (current: {})", config.longshot.max_skip),
            &format!(
                "target_overlap (current: {})",
                config.longshot.target_overlap
            ),
            "< Back to main menu",
        ];

        let selection = Select::new()
            .with_prompt("Select longshot field to edit")
            .default(0)
            .items(fields)
            .interact()?;

        match selection {
            0 => {
                config.longshot.fps = Input::new()
                    .with_prompt("Longshot FPS")
                    .default(config.longshot.fps)
                    .interact_text()?;
            }
            1 => {
                config.longshot.sad_threshold = Input::new()
                    .with_prompt("Column SAD threshold (lower = stricter, 5.0–15.0)")
                    .default(config.longshot.sad_threshold)
                    .interact_text()?;
            }
            2 => {
                config.longshot.max_skip = Input::new()
                    .with_prompt("Max frames to skip when velocity is high (1–20)")
                    .default(config.longshot.max_skip)
                    .interact_text()?;
            }
            3 => {
                config.longshot.target_overlap = Input::new()
                    .with_prompt("Target overlap fraction (0.15–0.50)")
                    .default(config.longshot.target_overlap)
                    .interact_text()?;
            }
            4 => break,
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn configure_advanced(config: &mut config::Config) -> Result<()> {
    loop {
        let fields = &[
            &format!(
                "freeze_on_area (current: {})",
                config.advanced.freeze_on_area
            ),
            &format!(
                "freeze_on_external (current: {})",
                config.advanced.freeze_on_external
            ),
            &format!("delay_ms (current: {} ms)", config.advanced.delay_ms),
            "< Back to main menu",
        ];

        let selection = Select::new()
            .with_prompt("Select advanced field to edit")
            .default(0)
            .items(fields)
            .interact()?;

        match selection {
            0 => {
                config.advanced.freeze_on_area = Confirm::new()
                    .with_prompt("Freeze desktop during area screenshot selection?")
                    .default(config.advanced.freeze_on_area)
                    .interact()?;
            }
            1 => {
                config.advanced.freeze_on_external = Confirm::new()
                    .with_prompt("Freeze desktop during annotation and OCR selection?")
                    .default(config.advanced.freeze_on_external)
                    .interact()?;
            }
            2 => {
                config.advanced.delay_ms = Input::new()
                    .with_prompt("Default delay before capture (ms)")
                    .default(config.advanced.delay_ms)
                    .interact_text()?;
            }
            3 => break,
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn configure_tools(_config: &mut config::Config) -> Result<()> {
    println!("Configure external tools in [external.<name>] or with --set external.<name>.command.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_commands_are_configurable_by_name() {
        let mut config = config::Config::default();
        assert!(set_config_value(&mut config, "annotate.command", "external-editor").is_err());
        set_config_value(&mut config, "external.ocr.command", "nbocr recognize {path}").unwrap();
        assert_eq!(config.external["ocr"].command, "nbocr recognize {path}");
    }

    #[test]
    fn editor_settings_use_existing_config_command() {
        let mut config = config::Config::default();
        set_config_value(&mut config, "editor.exit_after_copy", "true").unwrap();
        set_config_value(&mut config, "editor.active_tool", "Pencil").unwrap();
        assert!(config.editor.exit_after_copy);
        assert_eq!(config.editor.active_tool, "Pencil");
        assert!(set_config_value(&mut config, "editor.exit_after_save", "yes").is_err());
        assert!(set_config_value(&mut config, "editor.unknown", "true").is_err());
    }
}
