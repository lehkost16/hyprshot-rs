use gpui::SharedString;
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "assets/icons"]
pub struct EditorAssets;

impl gpui::AssetSource for EditorAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let clean_path = path.trim_start_matches('/');
        if let Some(file) = Self::get(clean_path) {
            Ok(Some(file.data))
        } else {
            Ok(None)
        }
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let clean_path = path.trim_start_matches('/');
        Ok(Self::iter()
            .filter(|p| p.starts_with(clean_path))
            .map(|p| SharedString::from(p.to_string()))
            .collect())
    }
}
