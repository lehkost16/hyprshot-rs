use std::sync::Arc;

pub fn setup_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "Source Han Sans CN".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "/usr/share/fonts/adobe-source-han-sans/SourceHanSansCN-Regular.otf"
        ))),
    );

    fonts.font_data.insert(
        "Noto Sans CJK SC".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"
        ))),
    );

    let proportional_family = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();

    proportional_family.insert(0, "Source Han Sans CN".to_owned());
    proportional_family.insert(1, "Noto Sans CJK SC".to_owned());

    ctx.set_fonts(fonts);
}
