use crate::annotator::{AnnotatorState, ApplyExtraZoomFactor, SharedAnnotatorState};
use crate::context::Command;
use crate::global::ReadGlobal;
use crate::platform::application::Application;
use crate::platform::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
use crate::platform::view::{View, ViewId};
use crate::platform::window::AppWindow;
use crate::ui::annotator_panel::create_annotator_panel;
use crate::ui::annotator_panel_shadow::create_annotator_panel_shadow;
use crate::ui::primary_toolbar::create_primary_toolbar;
use crate::ui::secondly_toolbar::create_secondly_toolbar;
use egui::{Color32, Context, Frame, FullOutput, RawInput};
use image::RgbaImage;
use sctk::shell::WaylandSurface;
use std::sync::Arc;

// Automatic size constraints do not change the user's expanded/collapsed preference.
const FLOATING_MARGIN: u32 = 4;
const FLOATING_GAP: u32 = 2;

const SHADOW_SIZE: u32 = 3;
const TOOLBAR_HEIGHT: u32 = 42;
const SECONDARY_TOOLBAR_HEIGHT: u32 = 34;
const PRIMARY_TOOLBAR_WIDTH: u32 = 532;
pub(crate) const FULL_TOOLBAR_WIDTH: u32 = PRIMARY_TOOLBAR_WIDTH;

#[derive(Debug)]
pub(crate) struct DockLayout {
    pub window_size: LogicalSize<u32>,
    pub primary_size: LogicalSize<u32>,
    pub primary_position: LogicalPosition<i32>,
    pub secondary_position: LogicalPosition<i32>,
    pub primary_visible: bool,
    pub secondary_visible: bool,
}

pub(crate) fn dock_layout(
    image: LogicalSize<u32>,
    expanded: bool,
    secondary_width: u32,
) -> DockLayout {
    let fits = image.width >= FULL_TOOLBAR_WIDTH.max(secondary_width) + FLOATING_MARGIN * 2
        && image.height >= TOOLBAR_HEIGHT + FLOATING_MARGIN * 2;
    let primary_visible = expanded && fits;
    let primary_width = FULL_TOOLBAR_WIDTH;
    let secondary_visible = primary_visible && secondary_width > 0;
    let window_width = image.width.max(primary_width + FLOATING_MARGIN * 2);
    let primary_y = image.height + FLOATING_MARGIN;
    let secondary_y = primary_y + TOOLBAR_HEIGHT + FLOATING_GAP;
    DockLayout {
        window_size: if !primary_visible {
            image
        } else {
            LogicalSize::new(
                window_width,
                image.height
                    + TOOLBAR_HEIGHT
                    + FLOATING_MARGIN * 2
                    + if secondary_visible {
                        SECONDARY_TOOLBAR_HEIGHT + FLOATING_GAP
                    } else {
                        0
                    },
            )
        },
        primary_size: LogicalSize::new(primary_width, TOOLBAR_HEIGHT),
        primary_position: LogicalPosition::new(
            window_width.saturating_sub(FLOATING_MARGIN + primary_width) as i32,
            primary_y as i32,
        ),
        secondary_position: LogicalPosition::new(
            window_width.saturating_sub(FLOATING_MARGIN + secondary_width) as i32,
            secondary_y as i32,
        ),
        primary_visible,
        secondary_visible,
    }
}

pub fn initial_window_size_for_image(
    image_width: u32,
    image_height: u32,
    screen_width: u32,
    screen_height: u32,
    scale_factor: f64,
) -> LogicalSize<u32> {
    let (panel_size, _) = fit_image_to_screen(
        image_width,
        image_height,
        screen_width,
        screen_height,
        scale_factor,
        false,
    );
    dock_layout(panel_size, true, 0).window_size
}

pub(crate) fn fit_image_to_screen(
    image_width: u32,
    image_height: u32,
    screen_width: u32,
    screen_height: u32,
    scale_factor: f64,
    _include_secondary_toolbar: bool,
) -> (LogicalSize<u32>, f32) {
    let scale_factor = scale_factor.max(1.0) as f32;
    let img_logical_w = image_width as f32 / scale_factor;
    let img_logical_h = image_height as f32 / scale_factor;
    let max_display_w = screen_width.saturating_sub(60).max(320) as f32;
    let max_display_h = screen_height
        .saturating_sub(
            60 + TOOLBAR_HEIGHT + SECONDARY_TOOLBAR_HEIGHT + FLOATING_MARGIN * 2 + FLOATING_GAP,
        )
        .max(1) as f32;

    let scale_w = if img_logical_w > max_display_w {
        max_display_w / img_logical_w
    } else {
        1.0
    };
    let scale_h = if img_logical_h > max_display_h {
        max_display_h / img_logical_h
    } else {
        1.0
    };
    let fit_scale = scale_w.min(scale_h);
    (
        LogicalSize::new(
            (img_logical_w * fit_scale).round() as u32,
            (img_logical_h * fit_scale).round() as u32,
        ),
        fit_scale,
    )
}

pub fn build_annotator(
    input: RawInput,
    egui_ctx: &mut Context,
    app: &mut Application,
    window: &mut AppWindow,
    image: Arc<RgbaImage>,
    current_view: &mut dyn View,
) -> FullOutput {
    // 标注面板的ID
    let annotator_panel_id = AnnotatorState::annotator_panel_id();
    // 主工具条的ID
    let primary_toolbar_id = AnnotatorState::primary_toolbar_id();
    // 次工具的ID
    let secondly_toolbar_id = AnnotatorState::secondly_toolbar_id();
    // 标注面板的阴影
    let annotator_shadow_panel_id: ViewId = "annotator_panel_shadow".into();

    // fit_scale: 初始适应屏幕的缩放比例（只在面板未创建时有意义）
    let annotator_panel_created = window.views.contains_key(&annotator_panel_id);

    let config = app.session.settings();

    let scale_factor = egui_ctx.pixels_per_point() as f64;
    let (screen_w, screen_h) = app.screen_size();
    let include_secondary_toolbar = config.auto_activate_default_tool;
    let (_, fit_scale) = fit_image_to_screen(
        image.width(),
        image.height(),
        screen_w,
        screen_h,
        scale_factor,
        include_secondary_toolbar,
    );

    let extra_zoom_factor = if annotator_panel_created
        && window
            .window_context
            .globals_by_type
            .has_global::<SharedAnnotatorState>()
    {
        window
            .window_context
            .globals_by_type
            .require_ref::<SharedAnnotatorState>()
            .borrow()
            .extra_zoom_factor
    } else {
        fit_scale
    };

    let correct_panel_size = PhysicalSize::new(image.width(), image.height())
        .to_logical(scale_factor)
        .apply_extra_zoom_factor(extra_zoom_factor);

    let (annotator_panel_size, initial_fit_scale) = if annotator_panel_created {
        let current_size = {
            let annotator_panel = window
                .views
                .get(&annotator_panel_id)
                .unwrap()
                .as_ref()
                .unwrap();
            annotator_panel.get_view_ref().size()
        };

        // scale 变化时 panel 逻辑尺寸需要随之调整
        if current_size != correct_panel_size {
            window
                .window_context
                .commands
                .push_back(Command::ResizeView(
                    annotator_panel_id.clone(),
                    correct_panel_size,
                ));
        }
        (correct_panel_size, 1.0f32)
    } else {
        (correct_panel_size, fit_scale)
    };
    let (toolbar_enabled, secondary_width) = if window
        .window_context
        .globals_by_type
        .has_global::<SharedAnnotatorState>()
    {
        let annotator_state = window
            .window_context
            .globals_by_type
            .require_ref::<SharedAnnotatorState>()
            .borrow();
        (
            annotator_state.toolbar_visible,
            crate::ui::secondly_toolbar::preferred_width(&annotator_state),
        )
    } else {
        (true, 0)
    };
    let dock = dock_layout(annotator_panel_size, toolbar_enabled, secondary_width);
    let secondly_toolbar_visible = dock.secondary_visible;
    let primary_toolbar_size = dock.primary_size;

    let secondly_toolbar_size = LogicalSize::new(secondary_width.max(1), SECONDARY_TOOLBAR_HEIGHT);

    let annotator_panel_position = LogicalPosition::new(0, 0);
    let (shadow_panel_size, shadow_panel_position) =
        get_panel_shadow_position_and_size(annotator_panel_size, annotator_panel_position);

    let primary_toolbar_position = dock.primary_position;

    if !window.views.contains_key(&annotator_panel_id) {
        create_annotator_panel_shadow(
            annotator_shadow_panel_id.clone(),
            app,
            window,
            shadow_panel_size,
            shadow_panel_position,
        );

        create_annotator_panel(
            annotator_panel_id.clone(),
            app,
            window,
            image,
            annotator_panel_position,
            initial_fit_scale,
            egui_ctx.pixels_per_point() as f64,
        );

        create_primary_toolbar(
            primary_toolbar_id.clone(),
            app,
            window,
            primary_toolbar_size,
            primary_toolbar_position,
        );

        let secondly_toolbar_position = dock.secondary_position;
        create_secondly_toolbar(
            secondly_toolbar_id.clone(),
            app,
            window,
            secondly_toolbar_size,
            secondly_toolbar_position,
        );
    }

    let main_window_size = dock.window_size;
    let current_main_window_size = current_view.size();
    if current_main_window_size != main_window_size {
        window
            .window_context
            .commands
            .push_back(Command::ResizeView(current_view.id(), main_window_size));

        // 鼠标穿透
        let qh = &app.global_state.queue_handle;
        let empty_region = app
            .global_state
            .compositor_state
            .wl_compositor()
            .create_region(qh, ());
        window.xdg_window().set_input_region(Some(&empty_region));
    }

    let annotator_panel_position = LogicalPosition::new(0, 0);
    let (shadow_panel_size, shadow_panel_position) =
        get_panel_shadow_position_and_size(annotator_panel_size, annotator_panel_position);
    let annotator_panel_shadow = window
        .views
        .get(&annotator_shadow_panel_id)
        .unwrap()
        .as_ref()
        .unwrap();

    let panel_shadow_current_size = annotator_panel_shadow.get_view_ref().size();
    let panel_shadow_current_position = annotator_panel_shadow.get_view_ref().position().unwrap();
    if panel_shadow_current_size != shadow_panel_size {
        window
            .window_context
            .commands
            .push_back(Command::ResizeView(
                annotator_shadow_panel_id,
                shadow_panel_size,
            ));
    } else if panel_shadow_current_position != shadow_panel_position {
        window
            .window_context
            .commands
            .push_back(Command::RepositionSubView(
                annotator_shadow_panel_id,
                shadow_panel_position,
            ));
    }

    let annotator_panel = window
        .views
        .get(&annotator_panel_id)
        .unwrap()
        .as_ref()
        .unwrap();
    let annotator_panel_current_position = annotator_panel.get_view_ref().position().unwrap();
    if annotator_panel_current_position != annotator_panel_position {
        window
            .window_context
            .commands
            .push_back(Command::RepositionSubView(
                annotator_panel_id,
                annotator_panel_position,
            ));
    }

    let primary_toolbar_position = dock.primary_position;
    let primary_toolbar = window
        .views
        .get(&primary_toolbar_id)
        .unwrap()
        .as_ref()
        .unwrap();
    let primary_toolbar_current_size = primary_toolbar.get_view_ref().size();
    let primary_toolbar_current_position = primary_toolbar.get_view_ref().position().unwrap();
    if primary_toolbar_current_size != primary_toolbar_size {
        window
            .window_context
            .commands
            .push_back(Command::ResizeView(
                primary_toolbar_id.clone(),
                primary_toolbar_size,
            ));
    }
    if primary_toolbar_current_position != primary_toolbar_position {
        window
            .window_context
            .commands
            .push_back(Command::RepositionSubView(
                primary_toolbar_id,
                primary_toolbar_position,
            ));
    }

    let secondly_toolbar_position = dock.secondary_position;
    let secondly_toolbar = window
        .views
        .get(&secondly_toolbar_id)
        .unwrap()
        .as_ref()
        .unwrap();
    let secondly_toolbar_current_size = secondly_toolbar.get_view_ref().size();
    let secondly_toolbar_current_position = secondly_toolbar.get_view_ref().position().unwrap();
    if secondly_toolbar_current_size != secondly_toolbar_size {
        window
            .window_context
            .commands
            .push_back(Command::ResizeView(
                secondly_toolbar_id.clone(),
                secondly_toolbar_size,
            ));
    }
    if secondly_toolbar_current_position != secondly_toolbar_position {
        window
            .window_context
            .commands
            .push_back(Command::RepositionSubView(
                secondly_toolbar_id,
                secondly_toolbar_position,
            ));
    }

    window.set_view_visible(&AnnotatorState::primary_toolbar_id(), dock.primary_visible);

    window.set_view_visible(
        &AnnotatorState::secondly_toolbar_id(),
        secondly_toolbar_visible,
    );

    egui_ctx.run_ui(input, move |ctx| {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(Color32::TRANSPARENT))
            .show(ctx, |_ui| {});
    })
}

fn get_panel_shadow_position_and_size(
    annotator_panel_size: LogicalSize<u32>,
    annotator_panel_position: LogicalPosition<i32>,
) -> (LogicalSize<u32>, LogicalPosition<i32>) {
    let shadow_panel_size = LogicalSize::new(
        annotator_panel_size.width + SHADOW_SIZE * 2,
        annotator_panel_size.height + SHADOW_SIZE * 2,
    );
    let shadow_panel_position = LogicalPosition::new(
        annotator_panel_position.x - SHADOW_SIZE as i32,
        annotator_panel_position.y - SHADOW_SIZE as i32,
    );
    (shadow_panel_size, shadow_panel_position)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_never_overlaps_image_at_any_size() {
        for width in [1, 40, 200, 571, 572, 900, 2000] {
            for height in [1, 20, 500, 1200] {
                for expanded in [false, true] {
                    for secondary in [0, 300, 518] {
                        let image = LogicalSize::new(width, height);
                        let dock = dock_layout(image, expanded, secondary);
                        if !dock.primary_visible {
                            assert_eq!(dock.window_size, image);
                            assert!(!dock.secondary_visible);
                            continue;
                        }
                        assert!(dock.primary_position.y >= height as i32);
                        assert!(dock.secondary_position.y >= height as i32);
                        assert!(dock.primary_position.x >= 0);
                        assert_eq!(
                            dock.primary_position.x as u32 + dock.primary_size.width,
                            dock.window_size.width - FLOATING_MARGIN
                        );
                        assert!(
                            dock.primary_position.x as u32 + dock.primary_size.width
                                <= dock.window_size.width
                        );
                        assert!(
                            dock.primary_position.y as u32 + dock.primary_size.height
                                <= dock.window_size.height
                        );
                        if dock.secondary_visible {
                            assert_eq!(
                                dock.secondary_position.x as u32 + secondary,
                                dock.window_size.width - FLOATING_MARGIN
                            );
                            assert!(
                                dock.secondary_position.x as u32 + secondary
                                    <= dock.window_size.width
                            );
                            assert!(
                                dock.secondary_position.y as u32 + SECONDARY_TOOLBAR_HEIGHT
                                    <= dock.window_size.height
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn collapsing_releases_secondary_space_without_changing_image() {
        let image = LogicalSize::new(900, 500);
        assert_eq!(
            dock_layout(image, true, 518).window_size,
            LogicalSize::new(900, 586)
        );
        assert_eq!(
            dock_layout(image, false, 518).window_size,
            LogicalSize::new(900, 500)
        );
        assert_eq!(
            dock_layout(LogicalSize::new(200, 20), true, 518)
                .primary_size
                .width,
            FULL_TOOLBAR_WIDTH
        );
        assert_eq!(
            dock_layout(LogicalSize::new(900, 20), true, 518)
                .primary_size
                .width,
            FULL_TOOLBAR_WIDTH
        );
    }

    #[test]
    fn fractional_scale_does_not_enlarge_small_images_for_toolbar() {
        let (size, zoom) = fit_image_to_screen(200, 100, 2048, 1280, 1.25, true);
        assert_eq!(size, LogicalSize::new(160, 80));
        assert_eq!(zoom, 1.0);
        assert_eq!(
            dock_layout(size, true, 518).window_size,
            LogicalSize::new(160, 80)
        );
    }

    #[test]
    fn small_windows_hide_every_bottom_control_and_restore_by_preference() {
        for (width, height) in [(1, 1), (200, 150), (539, 500), (900, 49)] {
            let image = LogicalSize::new(width, height);
            let dock = dock_layout(image, true, 518);
            assert!(!dock.primary_visible);
            assert!(!dock.secondary_visible);
            assert_eq!(dock.window_size, image);
        }
        let image = LogicalSize::new(540, 50);
        assert!(dock_layout(image, true, 518).primary_visible);
        assert!(dock_layout(image, true, 518).secondary_visible);
        assert!(!dock_layout(image, false, 518).primary_visible);
        assert_eq!(dock_layout(image, false, 518).window_size, image);
    }

    #[test]
    fn large_image_fit_reserves_space_for_both_bars() {
        let (size, _) = fit_image_to_screen(4000, 8000, 2048, 1280, 1.25, true);
        assert!(dock_layout(size, true, 518).window_size.height <= 1280 - 60);
    }
}
