//! Capture workflows connect acquisition to an editor or an external processor.
//! Neither the editor nor process adapters own screen-selection lifecycle.

use anyhow::Result;
use hyshot_core::{ImageDocument, ImageSource};
use hyshot_editor::{EditorInput, EditorOptions};
use std::path::PathBuf;

use crate::cli::{Args, resolve_notif_timeout};
use crate::config::{Config, get_screenshots_dir};
use crate::geometry::Geometry;
use crate::ocr::{self, OcrOptions};

#[derive(Clone, Copy)]
pub enum ScreenshotAction {
    Annotate,
    Ocr,
}

struct SelectedCapture {
    geometry: Geometry,
    png: Vec<u8>,
}

impl SelectedCapture {
    fn acquire(action: ScreenshotAction, args: &Args, config: &Config) -> Result<Self> {
        let guard = if should_freeze(action, &config.advanced) {
            Some(crate::freeze::start_freeze(None, args.debug)?)
        } else {
            None
        };
        let geometry = crate::selector::select_region(args.debug)?;
        let png = crate::utils::capture_region_png_with_grim_cli(&geometry)?;
        if let Some(guard) = guard {
            guard.stop()?;
        }
        Ok(Self { geometry, png })
    }
}

fn should_freeze(action: ScreenshotAction, advanced: &crate::config::AdvancedConfig) -> bool {
    match action {
        ScreenshotAction::Annotate => advanced.freeze_on_annotate,
        ScreenshotAction::Ocr => advanced.freeze_on_ocr,
    }
}

pub fn screenshot(action: ScreenshotAction, args: &Args, config: &Config) -> Result<()> {
    let capture = SelectedCapture::acquire(action, args, config)?;
    match action {
        ScreenshotAction::Annotate => {
            let document =
                ImageDocument::from_png(&capture.png, ImageSource::Capture(capture.geometry))?;
            drop(capture.png);
            return edit_document(document, args, config);
        }
        ScreenshotAction::Ocr => {}
    }
    ocr::run(
        &config.ocr.command,
        &capture.png,
        capture.geometry,
        OcrOptions {
            debug: args.debug,
            silent: args.silent || !config.capture.notification,
            notification_timeout: resolve_notif_timeout(args, config),
        },
    )
}

pub fn edit_files(paths: Vec<PathBuf>, args: &Args, config: &Config) -> Result<()> {
    edit(EditorInput::Files(paths), args, config)
}

pub fn edit_document(document: ImageDocument, args: &Args, config: &Config) -> Result<()> {
    edit(EditorInput::Document(document), args, config)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_selected_capture_uses_its_own_freeze_setting() {
        let advanced = crate::config::AdvancedConfig {
            freeze_on_area: true,
            freeze_on_annotate: true,
            freeze_on_ocr: false,
            delay_ms: 0,
        };
        assert!(should_freeze(ScreenshotAction::Annotate, &advanced));
        assert!(!should_freeze(ScreenshotAction::Ocr, &advanced));
    }
}
