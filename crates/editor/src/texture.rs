use egui::TextureOptions;

pub(crate) fn set_texture_limit(input: &mut egui::RawInput, maximum: u32) {
    input.max_texture_side = Some(maximum as usize);
}

pub fn screenshot_texture_options() -> TextureOptions {
    TextureOptions::NEAREST
}

#[cfg(test)]
mod tests {
    #[test]
    fn wide_and_tall_images_load_after_each_input_reset() {
        let context = egui::Context::default();
        for size in [[2246, 612], [2238, 811], [612, 2246]] {
            let mut input = egui::RawInput::default();
            super::set_texture_limit(&mut input, 4096);
            let _ = context.run_ui(input, |ui| {
                let ctx = ui.ctx();
                assert_eq!(ctx.input(|i| i.max_texture_side), 4096);
                let texture = ctx.load_texture(
                    "background-image",
                    egui::ColorImage::filled(size, egui::Color32::WHITE),
                    super::screenshot_texture_options(),
                );
                assert_eq!(texture.size(), size);
            });
        }
    }
}
