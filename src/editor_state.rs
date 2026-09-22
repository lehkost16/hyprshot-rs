//! Host-owned style memory. Editing never rewrites explicit config.toml.
use anyhow::{Context, Result};
use hyshot_editor::EditorToolState;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn path() -> Result<PathBuf> {
    Ok(crate::config::Config::config_dir()?.join("editor-state.toml"))
}

pub fn load(path: &Path) -> Result<EditorToolState> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .with_context(|| format!("Invalid editor state: {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(EditorToolState::default())
        }
        Err(error) => Err(error).context("Cannot read editor state"),
    }
}

pub fn save_changes(
    path: &Path,
    initial: &EditorToolState,
    updated: &EditorToolState,
) -> Result<()> {
    if initial == updated {
        return Ok(());
    }
    let parent = path.parent().context("Editor state path has no parent")?;
    fs::create_dir_all(parent)?;
    let _lock = crate::capture_session::SessionLock::acquire(path)?;
    // Merge only tools changed in this session, preserving other open editors' changes.
    let mut current = load(path)?;
    for (name, style) in &updated.tool_settings {
        if initial.tool_settings.get(name) != Some(style) {
            current.tool_settings.insert(name.clone(), style.clone());
        }
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(toml::to_string_pretty(&current)?.as_bytes())?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist(path).context("Cannot publish editor state")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyshot_editor::ToolSettings;

    #[test]
    fn style_updates_merge_without_touching_explicit_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("editor-state.toml");
        let config = dir.path().join("config.toml");
        fs::write(
            &config,
            "# user config\n[editor]\nactive_tool = 'Rectangle'\n",
        )
        .unwrap();
        let initial = load(&path).unwrap();
        let mut first = initial.clone();
        first.tool_settings.insert(
            "Pencil".into(),
            ToolSettings {
                stroke_width: Some(5.0),
                stroke_color_rgba: Some([10, 20, 30, 112]),
                ..Default::default()
            },
        );
        save_changes(&path, &initial, &first).unwrap();
        let mut second = initial.clone();
        second.tool_settings.insert(
            "Rectangle".into(),
            ToolSettings {
                stroke_width: Some(3.0),
                ..Default::default()
            },
        );
        save_changes(&path, &initial, &second).unwrap();
        let final_state = load(&path).unwrap();
        assert_eq!(final_state.tool_settings.len(), 2);
        assert_eq!(
            final_state.tool_settings["Pencil"],
            first.tool_settings["Pencil"]
        );
        assert_eq!(
            fs::read_to_string(config).unwrap(),
            "# user config\n[editor]\nactive_tool = 'Rectangle'\n"
        );
    }

    #[test]
    fn malformed_state_is_not_silently_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("editor-state.toml");
        fs::write(&path, "bad = [").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "bad = [");
    }
}
