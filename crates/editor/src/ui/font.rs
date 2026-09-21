use std::sync::{Arc, OnceLock};

pub fn setup_chinese_fonts(ctx: &egui::Context) {
    static SYSTEM_FONT: OnceLock<Option<Arc<egui::FontData>>> = OnceLock::new();
    let font = SYSTEM_FONT.get_or_init(|| {
        let output = std::process::Command::new("fc-match")
            .args(["--format=%{file}", "sans-serif:lang=zh-cn"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8(output.stdout).ok()?;
        let bytes = std::fs::read(path.trim()).ok()?;
        Some(Arc::new(egui::FontData::from_owned(bytes)))
    });
    let mut fonts = egui::FontDefinitions::default();
    if let Some(font) = font {
        fonts.font_data.insert("system-cjk".into(), font.clone());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "system-cjk".into());
    } else {
        log::warn!("No system CJK font found; install fontconfig and a Chinese font");
    }
    ctx.set_fonts(fonts);
}
