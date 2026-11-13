use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolSettings {
    pub stroke_width: Option<f32>,
    pub stroke_color_rgba: Option<[u8; 4]>,
    pub fill_color_rgba: Option<[u8; 4]>,
    pub font_color_rgba: Option<[u8; 4]>,
}

/// Explicit startup preferences, never changed by remembered tool selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorPreferences {
    pub marker_pen_straight_mode: bool,
    pub auto_activate_default_tool: bool,
    pub auto_deactivate_tool_after_draw: bool,
    pub active_tool: String,
    pub exit_after_copy: bool,
    pub exit_after_save: bool,
}

impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            marker_pen_straight_mode: true,
            auto_activate_default_tool: false,
            auto_deactivate_tool_after_draw: false,
            active_tool: "Rectangle".into(),
            exit_after_copy: false,
            exit_after_save: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorToolState {
    pub tool_settings: BTreeMap<String, ToolSettings>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preferences_match_expected() {
        let prefs = EditorPreferences::default();
        assert_eq!(prefs.active_tool, "Rectangle");
        assert!(prefs.marker_pen_straight_mode);
        assert!(!prefs.auto_activate_default_tool);
        assert!(!prefs.auto_deactivate_tool_after_draw);
        assert!(!prefs.exit_after_copy);
        assert!(!prefs.exit_after_save);
    }
}
