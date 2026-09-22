//! Capture workflows connect acquisition to an editor or an external processor.
//! Neither the editor nor process adapters own screen-selection lifecycle.

use anyhow::Result;
use hyshot_editor::{EditorInput, EditorOptions};
use std::path::PathBuf;

use crate::cli::{Args, resolve_notif_timeout};
use crate::config::{Config, get_screenshots_dir};
use crate::external::{self, ExternalOptions, ExternalTool};
use crate::geometry::Geometry;

pub enum ScreenshotAction {
    Annotate,
    Ocr,
}

struct SelectedCapture {
    geometry: Geometry,
    png: Vec<u8>,
}

impl SelectedCapture {
    fn acquire(args: &Args, config: &Config) -> Result<Self> {
        let guard = if args.freeze || config.advanced.freeze_on_external {
            Some(crate::freeze::start_freeze(None, args.debug)?)
        } else {
            None
        };
        let geometry = crate::selector::select_region(args.debug)?;
        let png = crate::utils::capture_region_with_grim_cli(&geometry)?;
        if let Some(guard) = guard {
            guard.stop()?;
        }
        Ok(Self { geometry, png })
    }
}

pub fn screenshot(action: ScreenshotAction, args: &Args, config: &Config) -> Result<()> {
    let capture = SelectedCapture::acquire(args, config)?;
    let tool = match action {
        ScreenshotAction::Annotate if config.annotate.command.trim() == "builtin" => {
            return edit(EditorInput::Png(capture.png), args, config);
        }
        ScreenshotAction::Annotate => ExternalTool::Annotate(&config.annotate.command),
        ScreenshotAction::Ocr => ExternalTool::Ocr(&config.ocr.command),
    };
    external::run(
        tool,
        &capture.png,
        capture.geometry,
        ExternalOptions {
            debug: args.debug,
            silent: args.silent || !config.capture.notification,
            notification_timeout: resolve_notif_timeout(args, config),
        },
    )
}

pub fn edit_files(paths: Vec<PathBuf>, args: &Args, config: &Config) -> Result<()> {
    edit(EditorInput::Files(paths), args, config)
}

fn edit(input: EditorInput, args: &Args, config: &Config) -> Result<()> {
    let state_path = crate::editor_state::path()?;
    let initial = if args.no_config {
        Default::default()
    } else {
        crate::editor_state::load(&state_path)?
    };
    let state = hyshot_editor::run(
        input,
        EditorOptions {
            preferences: config.editor.clone(),
            tool_state: initial.clone(),
            output_directory: get_screenshots_dir(args.output_folder.clone(), config, args.debug)?,
            notifications: !args.silent && config.capture.notification,
            notification_timeout: resolve_notif_timeout(args, config),
        },
    )?;
    if !args.no_config {
        crate::editor_state::save_changes(&state_path, &initial, &state)?;
    }
    Ok(())
}
