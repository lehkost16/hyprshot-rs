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

/// Editor preferences; storage and output policy belong to the host application.
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
