use anyhow::Result;
use chrono::Local;
use notify_rust::Notification;
use std::thread::sleep;
use std::time::Duration;

use crate::capture;
use crate::cli::{Args, Subcommands, default_filename, resolve_delay, resolve_notif_timeout};
use crate::config;
use crate::config_cmds::{
    handle_config_path, handle_init_config, handle_set_config, handle_show_config,
};
use crate::external;
use crate::freeze;
use crate::longshot;
use crate::record;
use crate::save;
use crate::utils;

pub fn run(mut args: Args) -> Result<()> {
    // Handle config management commands first
    if args.init_config {
        return handle_init_config();
    }

    if args.show_config {
        return handle_show_config();
    }

    if args.config_path {
        return handle_config_path();
    }

    if let Some(ref set_args) = args.set {
        return handle_set_config(set_args);
    }

    // If overlay subcommand, run it directly without loading config or other logic
    if let Some(Subcommands::Overlay {
        x,
        y,
        w,
        h,
        scale,
        monitor,
        ox,
        oy,
    }) = &args.subcommand
    {
        return longshot::overlay::run_overlay(*x, *y, *w, *h, *scale, monitor, *ox, *oy);
    }

    // Load config
    let config = if args.no_config {
        if args.debug {
            eprintln!("Config loading disabled (--no-config flag)");
        }
        config::Config::default()
    } else {
        config::Config::load().unwrap_or_else(|e| {
            if args.debug {
                eprintln!("Failed to load config, using defaults: {}", e);
            }
            config::Config::default()
        })
    };

    let silent = if args.silent {
        true
    } else {
        !config.capture.notification
    };
    let notif_timeout = resolve_notif_timeout(&args, &config);

    // Dispatch subcommands
    let subcommand = match args.subcommand.take() {
        Some(cmd) => cmd,
        None => {
            print_help();
            return Ok(());
        }
    };

    match subcommand {
        Subcommands::Satty => {
            return external::run_external_screenshot_tool(&args, &config, false);
        }
        Subcommands::Ocr => {
            return external::run_external_screenshot_tool(&args, &config, true);
        }
        Subcommands::Longshot => {
            return longshot::handle_longshot(&args, &config);
        }
        Subcommands::Record => {
            return record::handle_record(&args, &config);
        }
        Subcommands::Now
        | Subcommands::Win
        | Subcommands::Area
        | Subcommands::In5
        | Subcommands::In10 => {
            let debug = args.debug;
            let clipboard_only = args.clipboard_only || !config.capture.save_file;
            let raw = args.raw;

            // Handle countdown / delay
            match subcommand {
                Subcommands::In5 => {
                    countdown(5, silent);
                }
                Subcommands::In10 => {
                    countdown(10, silent);
                }
                _ => {
                    let delay = resolve_delay(&args, &config);
                    if delay > Duration::from_secs(0) {
                        sleep(delay);
                    }
                }
            }

            let mut hyprctl_cache = capture::HyprctlCache::new();

            // Start freeze overlay if region mode
            let is_region = matches!(subcommand, Subcommands::Area);
            let freeze = is_region && (args.freeze || config.advanced.freeze_on_region);

            let (_monitor_name, _, _, _) =
                external::get_active_monitor_info(debug).unwrap_or(("".to_string(), 1.0, 0, 0));

            let freeze_guard = if freeze {
                if debug {
                    eprintln!("Freeze requested: starting overlay thread");
                }
                let guard = freeze::start_freeze(None, debug)?;
                if debug {
                    eprintln!("Freeze guard acquired");
                }
                Some(guard)
            } else {
                None
            };

            let geometry = match subcommand {
                Subcommands::Now | Subcommands::In5 | Subcommands::In10 => {
                    capture::grab_active_output(debug, &mut hyprctl_cache)?
                }
                Subcommands::Area => match capture::grab_region(debug) {
                    Ok(geo) => geo,
                    Err(err) => {
                        if !silent && capture::is_region_selection_cancelled(&err) {
                            let _ = Notification::new()
                                .summary("Region mode")
                                .body("Drag to select an area.")
                                .appname("Shot")
                                .timeout(notif_timeout as i32)
                                .show();
                        }
                        return Err(err);
                    }
                },
                Subcommands::Win => {
                    let geo = capture::grab_window(debug, &mut hyprctl_cache)?;
                    utils::trim(&geo, debug)?
                }
                _ => unreachable!(),
            };

            let save_dir = config::get_screenshots_dir(args.output_folder.clone(), &config, debug)?;
            let save_dir = if !clipboard_only && !raw {
                config::ensure_directory(&save_dir.to_string_lossy())?
            } else {
                save_dir
            };
            let filename = args
                .filename
                .unwrap_or_else(|| default_filename(Local::now()));
            let save_fullpath = save_dir.join(&filename);

            save::save_geometry(
                &geometry,
                &save_fullpath,
                clipboard_only,
                raw,
                None, // custom external command is run in Edit mode
                silent,
                notif_timeout,
                debug,
            )?;

            if let Some(guard) = freeze_guard {
                guard.stop()?;
                std::thread::sleep(std::time::Duration::from_millis(150));
            } else {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
        Subcommands::Overlay { .. } => unreachable!(),
    }

    Ok(())
}

fn countdown(seconds: u64, silent: bool) {
    if silent {
        sleep(Duration::from_secs(seconds));
        return;
    }
    for i in (1..=seconds).rev() {
        let _ = Notification::new()
            .summary("准备截图")
            .body(&format!("倒计时: {} 秒", i))
            .timeout(1000)
            .appname("Shot")
            .show();
        sleep(Duration::from_secs(1));
    }
}

fn print_help() {
    println!(
        r#"
Usage: shot [options ..] <command>

Shot is a pure Rust screenshot utility for Wayland/Hyprland.

Commands:
  now           Take a screenshot of the current monitor
  win           Take a screenshot of a window
  area          Take a screenshot of a selected region
  edit          Take a screenshot of a selected region and open in editor
  ocr           Take a screenshot of a selected region and perform OCR
  in5           Take a screenshot of the current monitor after 5s countdown
  in10          Take a screenshot of the current monitor after 10s countdown
  longshot      Start/stop a scrolling screenshot
  record        Record a selected region of the screen (toggle start/stop)

Options:
  -h, --help                show help message
  -o, --output-folder       directory in which to save screenshot
  -f, --filename            the file name of the resulting screenshot
  -D, --delay               how long to delay taking the screenshot after selection (seconds)
  --freeze                  freeze the screen on initialization
  -d, --debug               print debug information
  -s, --silent              don't send notification
  -r, --raw                 output raw image data to stdout
  -n, --notif-timeout       notification timeout in milliseconds
  --clipboard-only          copy screenshot to clipboard and don't save to disk

Config Management:
  --init-config             initialize default config file
  --show-config             show current configuration
  --config-path             show path to config file
  --set KEY VALUE           set config value
"#
    );
}
