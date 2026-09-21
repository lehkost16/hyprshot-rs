use egui::{Color32, Frame, Margin, Stroke};

pub const BACKGROUND: Color32 = Color32::from_rgb(28, 28, 30);
pub const SURFACE: Color32 = Color32::from_rgb(38, 38, 41);
pub const BORDER: Color32 = Color32::from_rgb(76, 76, 82);
pub const HOVER: Color32 = Color32::from_rgb(58, 58, 63);
pub const ACCENT: Color32 = Color32::from_rgb(8, 96, 242);

pub fn frame(secondary: bool) -> Frame {
    Frame::new()
        .fill(if secondary { SURFACE } else { BACKGROUND })
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(8)
        .outer_margin(Margin::same(1))
        .inner_margin(Margin::symmetric(10, 5))
}

pub fn configure(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    *visuals = egui::Visuals::dark();
    visuals.panel_fill = SURFACE;
    visuals.window_fill = SURFACE;
    visuals.selection.bg_fill = ACCENT;
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.active.bg_fill = ACCENT;
}
