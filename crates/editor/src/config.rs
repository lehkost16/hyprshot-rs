use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

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

impl EditorSettings {
    pub fn from_parts(preferences: EditorPreferences, state: EditorToolState) -> Self {
        Self {
            marker_pen_straight_mode: preferences.marker_pen_straight_mode,
            auto_activate_default_tool: preferences.auto_activate_default_tool,
            auto_deactivate_tool_after_draw: preferences.auto_deactivate_tool_after_draw,
            active_tool: preferences.active_tool,
            exit_after_copy: preferences.exit_after_copy,
            exit_after_save: preferences.exit_after_save,
            tool_settings: state.tool_settings,
        }
    }

    pub fn tool_state(self) -> EditorToolState {
        EditorToolState {
            tool_settings: self.tool_settings,
        }
    }
}

/// Runtime settings; storage and output policy belong to the host application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    pub marker_pen_straight_mode: bool,
    pub auto_activate_default_tool: bool,
    pub auto_deactivate_tool_after_draw: bool,
    pub active_tool: String,
    pub exit_after_copy: bool,
    pub exit_after_save: bool,
    pub tool_settings: BTreeMap<String, ToolSettings>,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            marker_pen_straight_mode: true,
            auto_activate_default_tool: false,
            auto_deactivate_tool_after_draw: false,
            active_tool: "Rectangle".into(),
            exit_after_copy: false,
            exit_after_save: false,
            tool_settings: BTreeMap::new(),
        }
    }
}

/// One in-memory settings store shared by all windows in an editor session.
#[derive(Clone)]
pub struct EditorSession(Rc<RefCell<EditorSettings>>);

impl EditorSession {
    pub fn new(settings: EditorSettings) -> Self {
        Self(Rc::new(RefCell::new(settings)))
    }

    pub fn settings(&self) -> EditorSettings {
        self.0.borrow().clone()
    }

    pub fn replace(&self, settings: EditorSettings) {
        *self.0.borrow_mut() = settings;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_styles_do_not_override_explicit_startup_tool() {
        let prefs = EditorPreferences {
            active_tool: "Pencil".into(),
            auto_activate_default_tool: true,
            ..Default::default()
        };
        let mut runtime = EditorSettings::from_parts(prefs.clone(), EditorToolState::default());
        runtime.active_tool = "Rectangle".into();
        runtime.marker_pen_straight_mode = false;
        let reopened = EditorSettings::from_parts(prefs, runtime.tool_state());
        assert_eq!(reopened.active_tool, "Pencil");
        assert!(reopened.auto_activate_default_tool);
        assert!(reopened.marker_pen_straight_mode);
    }

    #[test]
    fn windows_share_preferences_without_disk_io() {
        let first = EditorSession::new(EditorSettings::default());
        let second = first.clone();
        let mut settings = first.settings();
        settings.active_tool = "Pencil".into();
        first.replace(settings);
        assert_eq!(second.settings().active_tool, "Pencil");
        assert_eq!(
            EditorSession::new(EditorSettings::default())
                .settings()
                .active_tool,
            "Rectangle"
        );
    }
}
