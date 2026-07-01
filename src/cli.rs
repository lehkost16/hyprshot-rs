use chrono::{DateTime, Local};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;

use crate::config;

#[derive(Parser)]
#[command(
    name = "shot",
    about = "Utility to easily take screenshots in Hyprland"
)]
pub struct Args {
    #[command(subcommand)]
    pub subcommand: Option<Subcommands>,

    #[arg(short, long, help = "Directory to save screenshot")]
    pub output_folder: Option<PathBuf>,

    #[arg(short, long, help = "Filename of the screenshot")]
    pub filename: Option<String>,

    #[arg(short = 'D', long, help = "Delay before taking screenshot (seconds)")]
    pub delay: Option<u64>,

    #[arg(long, help = "Freeze the screen on initialization")]
    pub freeze: bool,

    #[arg(short, long, help = "Print debug information")]
    pub debug: bool,

    #[arg(short, long, help = "Don't send notification")]
    pub silent: bool,

    #[arg(short, long, help = "Output raw image data to stdout")]
    pub raw: bool,

    #[arg(short, long, help = "Notification timeout (ms)")]
    pub notif_timeout: Option<u32>,

    #[arg(long, help = "Copy to clipboard and don't save to disk")]
    pub clipboard_only: bool,

    #[arg(long, help = "Initialize default config file")]
    pub init_config: bool,

    #[arg(long, help = "Show current configuration")]
    pub show_config: bool,

    #[arg(long, help = "Show path to config file")]
    pub config_path: bool,

    #[arg(
        long,
        value_names = ["KEY", "VALUE"],
        num_args = 2,
        help = "Set config value (e.g., --set paths.screenshots_dir ~/Screenshots)"
    )]
    pub set: Option<Vec<String>>,

    #[arg(
        long,
        help = "Don't load configuration file (use defaults and CLI args only)"
    )]
    pub no_config: bool,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Subcommands {
    #[command(about = "Screenshot of current monitor")]
    Now,
    #[command(about = "Screenshot of active or selected window")]
    Win,
    #[command(about = "Screenshot of selected region")]
    Area,
    #[command(about = "Screenshot of selected region and edit with satty annotation tool")]
    Satty,
    #[command(about = "Screenshot of selected region and perform OCR")]
    Ocr,
    #[command(about = "Screenshot of current monitor after 5 seconds delay")]
    In5,
    #[command(about = "Screenshot of current monitor after 10 seconds delay")]
    In10,
    #[command(about = "Scroll recording and stitch long screenshot")]
    Longshot,
    #[command(about = "Record a selected screen region to video (toggle start/stop)")]
    Record,
    #[command(hide = true)]
    Overlay {
        #[arg(long)]
        x: i32,
        #[arg(long)]
        y: i32,
        #[arg(long)]
        w: i32,
        #[arg(long)]
        h: i32,
        #[arg(long)]
        scale: f64,
        #[arg(long)]
        monitor: String,
        #[arg(long)]
        ox: i32,
        #[arg(long)]
        oy: i32,
    },
}

impl std::fmt::Debug for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Args")
            .field("subcommand", &self.subcommand)
            .field("output_folder", &self.output_folder)
            .field("filename", &self.filename)
            .field("delay", &self.delay)
            .field("freeze", &self.freeze)
            .field("debug", &self.debug)
            .field("silent", &self.silent)
            .field("raw", &self.raw)
            .field("notif_timeout", &self.notif_timeout)
            .field("clipboard_only", &self.clipboard_only)
            .finish()
    }
}

pub fn resolve_notif_timeout(args: &Args, config: &config::Config) -> u32 {
    args.notif_timeout
        .unwrap_or(config.capture.notification_timeout)
}

pub fn resolve_delay(args: &Args, config: &config::Config) -> Duration {
    if let Some(d) = args.delay {
        Duration::from_secs(d)
    } else if config.advanced.delay_ms > 0 {
        Duration::from_millis(config.advanced.delay_ms as u64)
    } else {
        Duration::from_secs(0)
    }
}

pub fn default_filename(now: DateTime<Local>) -> String {
    let config = crate::config::Config::load().unwrap_or_default();
    let ext = match config.capture.file_type.as_str() {
        "jpeg" => "jpeg",
        "jpg" => "jpg",
        "ppm" => "ppm",
        _ => "png",
    };
    format!(
        "{}-{:03}_shot.{}",
        now.format("%Y-%m-%d-%H%M%S"),
        now.timestamp_subsec_millis(),
        ext
    )
}
