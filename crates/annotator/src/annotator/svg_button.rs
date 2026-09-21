use crate::dpi::LogicalSize;
use egui::{Color32, Id, Image, Response, Sense, Stroke, StrokeKind, Ui, Widget, vec2};
use std::time::{Duration, Instant};

pub struct SvgButton {
    /// 一个id，用于跨帧追踪一些状态
    id: Id,
    /// svg图片
    image: Image<'static>,
    /// 图片显示的大小
    size: LogicalSize<f32>,
    /// 是否已禁用
    disabled: bool,
    /// 是否可勾选
    checkable: bool,
    /// 是否已勾选
    checked: bool,
}

impl SvgButton {
    pub fn new(
        id: Id,
        image: Image<'static>,
        size: LogicalSize<f32>,
        disabled: bool,
        checkable: bool,
        checked: bool,
    ) -> Self {
        Self {
            id,
            image,
            size,
            disabled,
            checkable,
            checked,
        }
    }
}

impl Widget for SvgButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = vec2(self.size.width, self.size.height);
        if self.disabled {
            let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
            self.image
                .tint(Color32::from_white_alpha(80))
                .paint_at(ui, rect);
            return response;
        }
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let bg_color_hover = crate::toolbar_style::HOVER;
        let bg_color_active = crate::toolbar_style::SURFACE;
        let bg_color_checked = crate::toolbar_style::ACCENT;
        let corner_radius = 7.0;

        if !self.disabled {
            if response.clicked() {
                ui.ctx().memory_mut(|memory| {
                    memory.data.insert_temp(self.id, Instant::now());
                });
            }

            let instant_clicked = ui
                .ctx()
                .memory(|memory| memory.data.get_temp::<Instant>(self.id));

            if let Some(instant_clicked) = instant_clicked {
                let duration = Instant::now().checked_duration_since(instant_clicked);
                if let Some(duration) = duration {
                    if duration < Duration::from_millis(300) {
                        ui.painter().rect(
                            rect.scale_from_center(0.92),
                            corner_radius,
                            bg_color_active,
                            Stroke::new(1_f32, Color32::TRANSPARENT),
                            StrokeKind::Middle,
                        );
                    } else {
                        ui.ctx().memory_mut(|memory| {
                            memory.data.remove_by_type::<Instant>();
                        });
                    }
                }
            } else if response.hovered() {
                ui.painter().rect(
                    rect,
                    corner_radius,
                    bg_color_hover,
                    Stroke::new(1_f32, crate::toolbar_style::BORDER),
                    StrokeKind::Inside,
                );
            }

            if self.checkable && self.checked {
                ui.painter().rect(
                    rect,
                    corner_radius,
                    bg_color_checked,
                    Stroke::new(1_f32, Color32::from_hex("#60a5fa").unwrap()),
                    StrokeKind::Inside,
                );
            }
        }

        self.image.paint_at(ui, rect);
        response
    }
}
