use anyhow::Result;
use chrono::Local;
use notify_rust::Notification;
use std::thread::sleep;
use std::time::Duration;

use crate::capture;
use crate::cli::{Args, Subcommands, default_filename, resolve_delay, resolve_notif_timeout};
use crate::config;
use crate::config_cmds::{
    handle_config_path, handle_init_config, handle_interactive_config, handle_print_binds,
    handle_set_config, handle_show_config,
};
use crate::freeze;
use crate::longshot;
use crate::record;
use crate::save::{self, SaveOptions};
use crate::utils;
use crate::workflow::{self, ScreenshotAction};

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

    if args.interactive {
        return handle_interactive_config();
    }

    if args.print_binds {
        return handle_print_binds();
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
        debug,
    }) = &args.subcommand
    {
        return longshot::overlay::run_overlay(longshot::overlay::OverlayOptions {
            x: *x,
            y: *y,
            w: *w,
            h: *h,
            scale: *scale,
            monitor: monitor.clone(),
            output_x: *ox,
            output_y: *oy,
            debug: *debug,
        });
    }

    // Load config
    let config = if args.no_config {
        if args.debug {
            eprintln!("Config loading disabled (--no-config flag)");
        }
        config::Config::default()
    } else {
        config::Config::load()?
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
            use clap::CommandFactory;
            Args::command().print_help()?;
            println!();
            return Ok(());
        }
    };

    match subcommand {
        Subcommands::Edit { images } => workflow::edit_files(images, &args, &config),
        Subcommands::Annotate => workflow::screenshot(ScreenshotAction::Annotate, &args, &config),
        Subcommands::Ocr => workflow::screenshot(ScreenshotAction::Ocr, &args, &config),
        Subcommands::Longshot { edit } => longshot::handle_longshot(&args, &config, edit),
        Subcommands::Stitch {
            input,
            output,
            edit,
        } => longshot::handle_stitch(input, output, edit, &args, &config),
        Subcommands::Record => record::handle_record(&args, &config),
        Subcommands::Now
        | Subcommands::Win
        | Subcommands::Area
        | Subcommands::In5
        | Subcommands::In10 => {
            run_screenshot_capture(subcommand, &args, &config, silent, notif_timeout)
        }
        Subcommands::Overlay { .. } => unreachable!(),
    }
}

fn run_screenshot_capture(
    subcommand: Subcommands,
    args: &Args,
    config: &config::Config,
    silent: bool,
    notif_timeout: u32,
) -> Result<()> {
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
            let delay = resolve_delay(args, config);
            if delay > Duration::from_secs(0) {
                sleep(delay);
            }
        }
    }

    let mut hyprctl_cache = capture::HyprctlCache::new();

    // Start freeze overlay if region mode
    let is_region = matches!(subcommand, Subcommands::Area);
    let freeze = is_region && (args.freeze || config.advanced.freeze_on_region);

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

    let image_bytes = if freeze_guard.is_some() {
        if debug {
            eprintln!(
                "Capture region BEFORE stopping freeze overlay to preserve transient windows (like tooltips)"
            );
        }
        let bytes = crate::utils::capture_region_with_grim_cli(&geometry)?;
        Some(bytes)
    } else {
        None
    };

    if let Some(guard) = freeze_guard {
        guard.stop()?;
    }

    let save_dir = config::get_screenshots_dir(args.output_folder.clone(), config, debug)?;
    let save_dir = if !clipboard_only && !raw {
        config::ensure_directory(&save_dir.to_string_lossy())?
    } else {
        save_dir
    };
    let filename = args
        .filename
        .clone()
        .unwrap_or_else(|| default_filename(Local::now()));
    let save_fullpath = save_dir.join(&filename);

    save::save_geometry(
        &geometry,
        &save_fullpath,
        SaveOptions {
            clipboard_only,
            raw,
            command: None, // custom external command is run in Edit mode
            silent,
            notif_timeout,
            debug,
            image_bytes,
            upload: args.upload,
            upload_command: config.capture.upload_command.clone(),
        },
    )?;

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
