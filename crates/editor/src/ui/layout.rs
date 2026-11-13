use crate::annotations::ToolKind;

// Automatic size constraints matching original annotator layout
pub const FLOATING_MARGIN: f32 = 4.0;
pub const FLOATING_GAP: f32 = 2.0;
pub const TOOLBAR_HEIGHT: f32 = 42.0;
pub const SECONDARY_TOOLBAR_HEIGHT: f32 = 34.0;
pub const PRIMARY_TOOLBAR_WIDTH: f32 = 496.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockLayout {
    pub window_width: f32,
    pub window_height: f32,
    pub primary_width: f32,
    pub primary_height: f32,
    pub primary_x: f32,
    pub primary_y: f32,
    pub secondary_width: f32,
    pub secondary_height: f32,
    pub secondary_x: f32,
    pub secondary_y: f32,
    pub primary_visible: bool,
    pub secondary_visible: bool,
}

pub fn secondary_toolbar_width_for_tool(tool: Option<ToolKind>) -> f32 {
    let Some(tool) = tool else {
        return 0.0;
    };
    let show_font_size = tool == ToolKind::Text;
    let show_palette = matches!(
        tool,
        ToolKind::Rectangle
            | ToolKind::Ellipse
            | ToolKind::Line
            | ToolKind::Arrow
            | ToolKind::Freehand
            | ToolKind::Highlight
            | ToolKind::Text
            | ToolKind::Badge
    );

    if !show_font_size && !show_palette {
        return 0.0;
    }

    let mut total_w = 20.0f32; // px(10.0) padding left + right
    let mut groups = 0;
    if show_font_size {
        // 5 font size buttons (26px each) + 4 gaps (3px each) = 142px + 4px buffer = 146px
        total_w += 146.0;
        groups += 1;
    }
    if show_palette {
        // 1 preview (20px) + 10 swatches (20px each) + 10 gaps (6px each) = 280px + 4px safety buffer
        total_w += 284.0;
        groups += 1;
    }
    if groups > 1 {
        total_w += (groups - 1) as f32 * 10.0;
    }
    total_w.ceil()
}

pub fn dock_layout(img_w: f32, img_h: f32, secondary_width: f32, expanded: bool) -> DockLayout {
    let primary_width = PRIMARY_TOOLBAR_WIDTH;
    let max_toolbar_width = primary_width.max(secondary_width);
    let fits = img_w >= max_toolbar_width + FLOATING_MARGIN * 2.0
        && img_h >= TOOLBAR_HEIGHT + FLOATING_MARGIN * 2.0;
    let primary_visible = expanded && fits;
    let secondary_visible = primary_visible && secondary_width > 0.0;
    let window_width = img_w.max(max_toolbar_width + FLOATING_MARGIN * 2.0);
    let primary_y = img_h + FLOATING_MARGIN;
    let secondary_y = primary_y + TOOLBAR_HEIGHT + FLOATING_GAP;

    let window_height = if !primary_visible {
        img_h
    } else {
        img_h
            + TOOLBAR_HEIGHT
            + FLOATING_MARGIN * 2.0
            + if secondary_visible {
                SECONDARY_TOOLBAR_HEIGHT + FLOATING_GAP
            } else {
                0.0
            }
    };

    DockLayout {
        window_width: if !primary_visible { img_w } else { window_width },
        window_height,
        primary_width,
        primary_height: TOOLBAR_HEIGHT,
        primary_x: (window_width - (FLOATING_MARGIN + primary_width)).max(0.0),
        primary_y,
        secondary_width,
        secondary_height: SECONDARY_TOOLBAR_HEIGHT,
        secondary_x: (window_width - (FLOATING_MARGIN + secondary_width)).max(0.0),
        secondary_y,
        primary_visible,
        secondary_visible,
    }
}

pub fn fit_image_to_screen(
    image_width: f32,
    image_height: f32,
    screen_width: f32,
    screen_height: f32,
) -> (f32, f32, f32) {
    let max_display_w = (screen_width - 60.0).max(320.0);
    let max_display_h = (screen_height
        - (60.0 + TOOLBAR_HEIGHT + SECONDARY_TOOLBAR_HEIGHT + FLOATING_MARGIN * 2.0 + FLOATING_GAP))
        .max(1.0);

    let scale_w = if image_width > max_display_w {
        max_display_w / image_width
    } else {
        1.0
    };
    let scale_h = if image_height > max_display_h {
        max_display_h / image_height
    } else {
        1.0
    };
    let fit_scale = scale_w.min(scale_h);
    (
        (image_width * fit_scale).round(),
        (image_height * fit_scale).round(),
        fit_scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_aligns_to_bottom_right() {
        let dock = dock_layout(900.0, 500.0, 518.0, true);
        assert_eq!(dock.window_width, 900.0);
        assert_eq!(dock.window_height, 586.0);
        assert_eq!(dock.primary_x + dock.primary_width, dock.window_width - FLOATING_MARGIN);
        assert_eq!(dock.secondary_x + dock.secondary_width, dock.window_width - FLOATING_MARGIN);
        assert_eq!(dock.primary_y, 500.0 + FLOATING_MARGIN);
    }

    #[test]
    fn dock_collapsing_releases_secondary_space() {
        let dock_expanded = dock_layout(900.0, 500.0, 518.0, true);
        assert_eq!(dock_expanded.window_height, 586.0);
        let dock_collapsed = dock_layout(900.0, 500.0, 518.0, false);
        assert_eq!(dock_collapsed.window_height, 500.0);
        assert!(!dock_collapsed.primary_visible);
    }
}
