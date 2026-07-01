use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

/// Main configuration structure for hyprshot-rs
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub advanced: AdvancedConfig,
    #[serde(default)]
    pub satty: SattyConfig,
    #[serde(default)]
    pub ocr: OcrConfig,
    #[serde(default)]
    pub longshot: LongshotConfig,
    #[serde(default)]
    pub record: RecordConfig,
}

/// Configuration for paths
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathsConfig {
    /// Directory where screenshots will be saved
    /// Default: ~/Pictures
    #[serde(default = "default_screenshots_dir")]
    pub screenshots_dir: String,
}

/// Configuration for capture settings
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CaptureConfig {
    /// Show notifications after capture
    /// Default: true
    #[serde(default = "default_notification")]
    pub notification: bool,

    /// Notification timeout in milliseconds
    /// Default: 3000
    #[serde(default = "default_notification_timeout")]
    pub notification_timeout: u32,

    /// Whether to save screenshots to disk by default
    /// Default: true
    #[serde(default = "default_save_file")]
    pub save_file: bool,

    /// Output file type: "png", "jpeg", or "ppm"
    /// Default: "png"
    #[serde(default = "default_file_type")]
    pub file_type: String,

    /// JPEG quality (0-100)
    /// Default: 100
    #[serde(default = "default_jpeg_quality")]
    pub jpeg_quality: u32,

    /// PNG compression level (0-9)
    /// Default: 6
    #[serde(default = "default_png_level")]
    pub png_level: u32,
}

/// Advanced configuration options
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdvancedConfig {
    /// Freeze screen when selecting region
    /// Default: true
    #[serde(default = "default_freeze")]
    pub freeze_on_region: bool,

    /// Delay before capture in milliseconds
    /// Default: 0
    #[serde(default)]
    pub delay_ms: u32,
}

/// Configuration for screenshot editing tool
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SattyConfig {
    /// Command to edit/annotate screenshot
    #[serde(default = "default_satty_command")]
    pub command: String,
}

/// Configuration for OCR tool
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OcrConfig {
    /// Command to perform OCR on screenshot
    #[serde(default = "default_ocr_command")]
    pub command: String,
}

/// Configuration for longshot (scrolling screenshot) settings
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LongshotConfig {
    /// Frame rate for recording longshot
    /// Default: 15
    #[serde(default = "default_longshot_fps")]
    pub fps: u32,

    /// Match threshold for image stitching (0.0 to 1.0)
    /// Default: 0.8
    #[serde(default = "default_longshot_match_threshold")]
    pub match_threshold: f32,

    /// Minimum movement in pixels to consider as new scrolled content
    /// Default: 5
    #[serde(default = "default_longshot_min_movement")]
    pub min_movement: i32,

    /// L1 difference threshold to detect static frames
    /// Default: 1.0
    #[serde(default = "default_longshot_static_threshold")]
    pub static_threshold: f32,
}

// Default value functions for serde
fn default_satty_command() -> String {
    "satty --filename {path}".to_string()
}

fn default_ocr_command() -> String {
    "nbocr recognize -l chinese -d v6-tiny {path} -f text -t 8".to_string()
}

fn default_screenshots_dir() -> String {
    "~/Pictures".to_string()
}

fn default_notification() -> bool {
    true
}

fn default_notification_timeout() -> u32 {
    3000
}

fn default_freeze() -> bool {
    true
}

fn default_save_file() -> bool {
    true
}

fn default_longshot_fps() -> u32 {
    30
}

fn default_longshot_match_threshold() -> f32 {
    0.8
}

fn default_longshot_min_movement() -> i32 {
    2
}

fn default_longshot_static_threshold() -> f32 {
    1.0
}

fn default_record_fps() -> u32 {
    30
}
fn default_record_crf() -> u32 {
    25
}

fn default_file_type() -> String {
    "png".to_string()
}

fn default_jpeg_quality() -> u32 {
    100
}

fn default_png_level() -> u32 {
    6
}

/// Configuration for region screen recording
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecordConfig {
    /// Recording frame rate. Default: 30
    #[serde(default = "default_record_fps")]
    pub fps: u32,
    /// libvpx-vp9 CRF quality (0=lossless, 63=worst). Default: 25
    #[serde(default = "default_record_crf")]
    pub crf: u32,
}

impl Default for RecordConfig {
    fn default() -> Self {
        Self {
            fps: default_record_fps(),
            crf: default_record_crf(),
        }
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            screenshots_dir: default_screenshots_dir(),
        }
    }
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            notification: default_notification(),
            notification_timeout: default_notification_timeout(),
            save_file: default_save_file(),
            file_type: default_file_type(),
            jpeg_quality: default_jpeg_quality(),
            png_level: default_png_level(),
        }
    }
}

impl Default for LongshotConfig {
    fn default() -> Self {
        Self {
            fps: default_longshot_fps(),
            match_threshold: default_longshot_match_threshold(),
            min_movement: default_longshot_min_movement(),
            static_threshold: default_longshot_static_threshold(),
        }
    }
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            freeze_on_region: default_freeze(),
            delay_ms: 0,
        }
    }
}

impl Default for SattyConfig {
    fn default() -> Self {
        Self {
            command: default_satty_command(),
        }
    }
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            command: default_ocr_command(),
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Config {
    fn default() -> Self {
        Self {
            paths: PathsConfig::default(),
            capture: CaptureConfig::default(),
            advanced: AdvancedConfig::default(),
            satty: SattyConfig::default(),
            ocr: OcrConfig::default(),
            longshot: LongshotConfig::default(),
            record: RecordConfig::default(),
        }
    }
}

// Utility functions for path expansion and validation

/// Expand path with support for:
/// - `~` → home directory
/// - `$HOME` → home directory
/// - `$XDG_PICTURES_DIR` → Pictures directory from environment or XDG config
/// - Other `$VAR` → environment variables
pub fn expand_path(path: &str) -> Result<PathBuf> {
    let path = path.trim();

    // Handle empty path
    if path.is_empty() {
        return Ok(PathBuf::from("."));
    }

    // Expand ~ at the beginning
    let path = if path.starts_with("~/") || path == "~" {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        if path == "~" {
            home
        } else {
            home.join(&path[2..])
        }
    } else {
        PathBuf::from(path)
    };

    let path_str = path.to_string_lossy();
    let mut result = String::new();
    let mut chars = path_str.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            let mut var_name = String::new();
            while let Some(&next_ch) = chars.peek() {
                if next_ch.is_alphanumeric() || next_ch == '_' {
                    var_name.push(chars.next().unwrap());
                } else {
                    break;
                }
            }

            if var_name == "XDG_PICTURES_DIR" {
                if let Some(pictures_dir) = dirs::picture_dir() {
                    result.push_str(&pictures_dir.to_string_lossy());
                } else {
                    // $HOME/Pictures
                    if let Some(home) = dirs::home_dir() {
                        result.push_str(&home.join("Pictures").to_string_lossy());
                    } else {
                        result.push_str("Pictures");
                    }
                }
            } else if !var_name.is_empty() {
                if let Ok(value) = env::var(&var_name) {
                    result.push_str(&value);
                } else {
                    // original $VAR if not found
                    result.push('$');
                    result.push_str(&var_name);
                }
            } else {
                result.push('$');
            }
        } else {
            result.push(ch);
        }
    }

    Ok(PathBuf::from(result))
}

/// Validate and prepare directory for saving screenshots
/// - Expands path variables
/// - Creates directory if it doesn't exist
/// - Returns error if path is not writable
pub fn ensure_directory(path: &str) -> Result<PathBuf> {
    let expanded_path = expand_path(path)?;

    let existed = expanded_path.exists();
    if !existed {
        fs::create_dir_all(&expanded_path).context(format!(
            "Failed to create directory: {}",
            expanded_path.display()
        ))?;
    }

    if !expanded_path.is_dir() {
        return Err(anyhow::anyhow!(
            "Path exists but is not a directory: {}",
            expanded_path.display()
        ));
    }

    if !existed {
        let test_file = expanded_path.join(".hyprshot_test");
        match fs::write(&test_file, b"test") {
            Ok(_) => {
                let _ = fs::remove_file(&test_file);
                Ok(expanded_path)
            }
            Err(e) => Err(anyhow::anyhow!(
                "Directory is not writable: {} - {}",
                expanded_path.display(),
                e
            )),
        }
    } else {
        Ok(expanded_path)
    }
}

/// Get screenshot save directory with priority:
/// 1. CLI argument (if provided)
/// 2. Environment variable HYPRSHOT_DIR
/// 3. Config file value
/// 4. Default ~/Pictures
pub fn get_screenshots_dir(
    cli_path: Option<PathBuf>,
    config: &Config,
    debug: bool,
) -> Result<PathBuf> {
    if let Some(path) = cli_path {
        if debug {
            eprintln!("Using screenshot directory from CLI: {}", path.display());
        }
        return Ok(path);
    }

    if let Ok(env_path) = env::var("HYPRSHOT_DIR") {
        let expanded = expand_path(&env_path)?;
        if debug {
            eprintln!(
                "Using screenshot directory from HYPRSHOT_DIR: {}",
                expanded.display()
            );
        }
        return Ok(expanded);
    }

    let config_path = expand_path(&config.paths.screenshots_dir)?;
    if debug {
        eprintln!(
            "Using screenshot directory from config: {}",
            config_path.display()
        );
    }
    Ok(config_path)
}

impl Config {
    /// Get the path to the configuration file
    /// Returns ~/.config/hyprshot-rs/config.toml
    pub fn config_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("", "", "hyprshot-rs")
            .context("Failed to determine config directory")?;

        let config_dir = proj_dirs.config_dir();
        Ok(config_dir.join("config.toml"))
    }

    /// Get the configuration directory
    /// Returns ~/.config/hyprshot-rs/
    pub fn config_dir() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("", "", "hyprshot-rs")
            .context("Failed to determine config directory")?;

        Ok(proj_dirs.config_dir().to_path_buf())
    }

    /// Load configuration from file
    /// If file doesn't exist, returns default configuration
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            // Config doesn't exist, return default
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_path).context(format!(
            "Failed to read config file: {}",
            config_path.display()
        ))?;

        let config: Config =
            toml::from_str(&content).context("Failed to parse config file. Check TOML syntax.")?;

        Ok(config)
    }

    /// Save configuration to file
    /// Creates config directory if it doesn't exist
    pub fn save(&self) -> Result<()> {
        let config_dir = Self::config_dir()?;
        let config_path = Self::config_path()?;

        if !config_dir.exists() {
            fs::create_dir_all(&config_dir).context(format!(
                "Failed to create config directory: {}",
                config_dir.display()
            ))?;
        }

        let toml_string =
            toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;

        let commented_toml = Self::add_comments(&toml_string);

        fs::write(&config_path, commented_toml).context(format!(
            "Failed to write config file: {}",
            config_path.display()
        ))?;

        Ok(())
    }

    /// Initialize config with default values and save to file
    /// This creates the config directory and file if they don't exist
    #[allow(dead_code)]
    pub fn init() -> Result<Self> {
        let config = Self::default();
        config.save()?;
        Ok(config)
    }

    /// Check if config file exists
    pub fn exists() -> bool {
        Self::config_path().map(|p| p.exists()).unwrap_or(false)
    }

    /// Add helpful comments to the TOML configuration
    fn add_comments(toml: &str) -> String {
        let header = "# hyprshot-rs configuration file\n\
                      # This file is automatically generated. Edit with care.\n\
                      # For more information, see: https://github.com/vremyavnikuda/hyprshot-rs\n\n";

        let mut result = String::from(header);

        for line in toml.lines() {
            // Add section comments
            if line.starts_with("[paths]") {
                result.push_str("# Paths configuration\n");
            } else if line.starts_with("[capture]") {
                result.push_str("\n# Capture settings\n");
            } else if line.starts_with("[advanced]") {
                result.push_str("\n# Advanced settings\n");
            } else if line.starts_with("[satty]") {
                result.push_str("\n# Satty annotation tool settings\n");
            } else if line.starts_with("[ocr]") {
                result.push_str("\n# OCR tool settings\n");
            } else if line.starts_with("[longshot]") {
                result.push_str("\n# Longshot (scrolling screenshot) settings\n");
            } else if line.starts_with("[record]") {
                result.push_str("\n# Screen recording settings\n");
            }

            result.push_str(line);
            result.push('\n');
        }

        result
    }
}
