use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSettings {
    pub stroke_width: Option<f32>,
    pub stroke_color_rgba: Option<[u8; 4]>,
    pub fill_color_rgba: Option<[u8; 4]>,
    pub font_color_rgba: Option<[u8; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnnotatorConfig {
    pub marker_pen_straight_mode: bool,
    pub auto_activate_default_tool: bool,
    pub auto_deactivate_tool_after_draw: bool,
    pub active_tool: String,
    pub exit_after_copy: bool,
    pub exit_after_save: bool,
    pub save_directory: String,
    pub tool_settings: HashMap<String, ToolSettings>,
}

impl Default for AnnotatorConfig {
    fn default() -> Self {
        Self {
            marker_pen_straight_mode: true,
            auto_activate_default_tool: false,
            auto_deactivate_tool_after_draw: false,
            active_tool: "Rectangle".to_string(),
            exit_after_copy: false,
            exit_after_save: false,
            save_directory: Self::default_save_directory()
                .to_string_lossy()
                .into_owned(),
            tool_settings: HashMap::new(),
        }
    }
}

impl AnnotatorConfig {
    pub fn default_save_directory() -> PathBuf {
        let pictures_directory = env::var_os("XDG_PICTURES_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join("Pictures")))
            .unwrap_or_else(|| PathBuf::from("Pictures"));
        pictures_directory.join("annotator")
    }

    fn config_path() -> Option<PathBuf> {
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

        Some(config_home.join("annotator").join("config.toml"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::config_path()
            && path.exists()
            && let Ok(content) = fs::read_to_string(&path)
            && let Some(config) = Self::from_toml(&content)
        {
            return config;
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(path, self.to_toml());
        }
    }

    fn from_toml(content: &str) -> Option<Self> {
        let mut config = Self::default();
        for line in content.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            let value = value.trim();

            if let Some(tool_key) = key.strip_prefix("tool.") {
                let (tool_name, field) = tool_key.split_once('.')?;
                let settings =
                    config
                        .tool_settings
                        .entry(tool_name.to_string())
                        .or_insert(ToolSettings {
                            stroke_width: None,
                            stroke_color_rgba: None,
                            fill_color_rgba: None,
                            font_color_rgba: None,
                        });
                match field {
                    "stroke_width" => settings.stroke_width = Some(value.parse().ok()?),
                    "stroke_color_rgba" => settings.stroke_color_rgba = Some(parse_rgba(value)?),
                    "fill_color_rgba" => settings.fill_color_rgba = Some(parse_rgba(value)?),
                    "font_color_rgba" => settings.font_color_rgba = Some(parse_rgba(value)?),
                    _ => {}
                }
                continue;
            }

            match key {
                "marker_pen_straight_mode" => {
                    config.marker_pen_straight_mode = value.parse().ok()?
                }
                "auto_activate_default_tool" => {
                    config.auto_activate_default_tool = value.parse().ok()?
                }
                "auto_deactivate_tool_after_draw" => {
                    config.auto_deactivate_tool_after_draw = value.parse().ok()?
                }
                "active_tool" => {
                    config.active_tool = value.trim_matches('"').to_string();
                }
                "exit_after_copy" => config.exit_after_copy = value.parse().ok()?,
                "exit_after_save" => config.exit_after_save = value.parse().ok()?,
                "save_directory" => config.save_directory = value.trim_matches('"').to_string(),
                _ => {}
            }
        }

        Some(config)
    }

    fn to_toml(&self) -> String {
        let mut output = format!(
            concat!(
                "marker_pen_straight_mode = {}\n",
                "auto_activate_default_tool = {}\n",
                "auto_deactivate_tool_after_draw = {}\n",
                "active_tool = \"{}\"\n",
                "exit_after_copy = {}\n",
                "exit_after_save = {}\n",
                "save_directory = \"{}\"\n",
            ),
            self.marker_pen_straight_mode,
            self.auto_activate_default_tool,
            self.auto_deactivate_tool_after_draw,
            self.active_tool.replace('"', "\\\""),
            self.exit_after_copy,
            self.exit_after_save,
            self.save_directory.replace('"', "\\\""),
        );
        let mut tools = self.tool_settings.iter().collect::<Vec<_>>();
        tools.sort_by(|(left, _), (right, _)| left.cmp(right));
        for (name, settings) in tools {
            if let Some(width) = settings.stroke_width {
                output.push_str(&format!("tool.{name}.stroke_width = {width}\n"));
            }
            if let Some(color) = settings.stroke_color_rgba {
                output.push_str(&format!(
                    "tool.{name}.stroke_color_rgba = [{}, {}, {}, {}]\n",
                    color[0], color[1], color[2], color[3]
                ));
            }
            if let Some(color) = settings.fill_color_rgba {
                output.push_str(&format!(
                    "tool.{name}.fill_color_rgba = [{}, {}, {}, {}]\n",
                    color[0], color[1], color[2], color[3]
                ));
            }
            if let Some(color) = settings.font_color_rgba {
                output.push_str(&format!(
                    "tool.{name}.font_color_rgba = [{}, {}, {}, {}]\n",
                    color[0], color[1], color[2], color[3]
                ));
            }
        }
        output
    }
}

fn parse_rgba(value: &str) -> Option<[u8; 4]> {
    let value = value.strip_prefix('[')?.strip_suffix(']')?;
    let mut parts = value.split(',').map(|part| part.trim().parse::<u8>().ok());
    Some([
        parts.next()??,
        parts.next()??,
        parts.next()??,
        parts.next()??,
    ])
}

#[cfg(test)]
mod tests {
    use super::AnnotatorConfig;

    #[test]
    fn parses_per_tool_settings() {
        let config = AnnotatorConfig::from_toml(
            "tool.Pencil.stroke_width = 5\n\
             tool.Pencil.stroke_color_rgba = [1, 2, 3, 255]\n\
             exit_after_save = true\n\
             save_directory = \"/tmp/annotator\"\n\
             tool.MarkerPen.stroke_width = 18\n",
        )
        .unwrap();

        let pencil = config.tool_settings.get("Pencil").unwrap();
        assert_eq!(pencil.stroke_width, Some(5.0));
        assert_eq!(pencil.stroke_color_rgba, Some([1, 2, 3, 255]));
        assert_eq!(
            config.tool_settings.get("MarkerPen").unwrap().stroke_width,
            Some(18.0)
        );
        assert!(config.exit_after_save);
        assert_eq!(config.save_directory, "/tmp/annotator");
    }
}
