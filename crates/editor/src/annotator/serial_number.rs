use crate::annotator::cursor::{CustomCursor, SerialNumber, SerialNumberStyle};
use crate::annotator::{
    ActivationSupport, Annotation, AnnotationActivationSupport, AnnotatorState,
    ApplyExtraZoomFactor, FillColorSupport, FontColorSupport, RemoveExtraZoomFactor,
    StrokeColorSupport, StrokeType, StrokeTypeSupport, StrokeWidthSupport,
    UnsubmittedAnnotationHandler,
};
use crate::{
    declare_not_support_font_color, declare_not_support_stroke_color,
    declare_not_support_stroke_type, declare_not_support_stroke_width,
};
use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Response, Sense, Ui, Widget};
use std::cell::RefCell;
use std::rc::Weak;

#[derive(Clone)]
pub struct SerialNumberAnnotation {
    serial_number: SerialNumber,
    activation: ActivationSupport,
}

impl SerialNumberAnnotation {
    pub fn new(center_pos: Pos2, number: u32, style: SerialNumberStyle) -> SerialNumberAnnotation {
        Self {
            serial_number: SerialNumber::new(center_pos, number, style),
            activation: ActivationSupport::NotSupported,
        }
    }
}

impl AnnotationActivationSupport for SerialNumberAnnotation {
    fn activation(&self) -> &ActivationSupport {
        &self.activation
    }

    fn activation_mut(&mut self) -> &mut ActivationSupport {
        &mut self.activation
    }
}

impl Widget for &mut SerialNumberAnnotation {
    fn ui(self, ui: &mut Ui) -> Response {
        let serial_number = self
            .serial_number
            .apply_extra_zoom_factor_with_ctx(ui.ctx());
        let rect = serial_number.rect();
        let response = ui.allocate_rect(rect, Sense::hover());
        serial_number.paint_with(ui.painter());
        response
    }
}

declare_not_support_stroke_width!(SerialNumberAnnotation);
declare_not_support_stroke_color!(SerialNumberAnnotation);
declare_not_support_stroke_type!(SerialNumberAnnotation);

impl FillColorSupport for SerialNumberAnnotation {
    fn supports_get_fill_color(&self) -> bool {
        true
    }

    fn fill_color(&self) -> Option<Color32> {
        Some(self.serial_number.style().fill_color)
    }

    fn supports_set_fill_color(&self) -> bool {
        true
    }

    fn set_fill_color(&mut self, color: Color32) {
        self.serial_number.style_mut().fill_color = color;
    }
}

impl From<SerialNumberAnnotation> for Annotation {
    fn from(val: SerialNumberAnnotation) -> Self {
        Annotation::SerialNumber(val)
    }
}

struct SerialNumberToolState {
    style: SerialNumberStyle,
}
impl SerialNumberToolState {
    fn new(style: SerialNumberStyle) -> Self {
        Self { style }
    }
}

impl Default for SerialNumberToolState {
    fn default() -> Self {
        let style = SerialNumberStyle {
            draw_rect_stroke: false,
            ..Default::default()
        };
        Self::new(style)
    }
}

pub struct SerialNumberTool {
    annotator_state: Weak<RefCell<AnnotatorState>>,
    tool_state: SerialNumberToolState,
}

impl SerialNumberTool {
    pub fn new(annotator_state: Weak<RefCell<AnnotatorState>>) -> Self {
        Self {
            annotator_state,
            tool_state: Default::default(),
        }
    }

    fn update_cursor(&mut self, ui: &mut Ui) {
        let pointer_pos = ui.ctx().input(|i| i.pointer.hover_pos());
        let Some(pointer_pos) = pointer_pos else {
            return;
        };

        ui.ctx().set_cursor_icon(CursorIcon::None);
        let mut style = self.tool_state.style.clone();
        style.draw_rect_stroke = true;

        // 预览数字也从栈中实时计算
        let annotator_state = self.annotator_state.upgrade().unwrap();
        let max_existing = annotator_state
            .borrow()
            .annotations_stack
            .iter()
            .filter_map(|a| match a {
                Annotation::SerialNumber(s) => Some(s.serial_number.number()),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let preview_number = (max_existing + 1).min(MAX_NUMBER);

        SerialNumber::new(pointer_pos, preview_number, style).paint_with(ui.painter());
    }
}

declare_not_support_stroke_width!(SerialNumberTool);
declare_not_support_stroke_color!(SerialNumberTool);
declare_not_support_stroke_type!(SerialNumberTool);
declare_not_support_font_color!(SerialNumberTool);

impl FillColorSupport for SerialNumberTool {
    fn supports_get_fill_color(&self) -> bool {
        true
    }
    fn fill_color(&self) -> Option<Color32> {
        Some(self.tool_state.style.fill_color)
    }
    fn supports_set_fill_color(&self) -> bool {
        true
    }
    fn set_fill_color(&mut self, color: Color32) {
        self.tool_state.style.fill_color = color;
    }
}

impl UnsubmittedAnnotationHandler for SerialNumberTool {}

const MAX_NUMBER: u32 = 99;

impl Widget for &mut SerialNumberTool {
    fn ui(self, ui: &mut Ui) -> Response {
        let sense_area = Rect::from_min_size(Pos2::ZERO, ui.available_size());
        let response = ui.allocate_rect(sense_area, Sense::click());

        self.update_cursor(ui);

        if response.clicked_by(PointerButton::Primary) {
            let pointer_pos = ui.ctx().input(|i| i.pointer.hover_pos());
            let pointer_pos = pointer_pos
                .unwrap()
                .remove_extra_zoom_factor_with_ctx(ui.ctx());

            // 从当前标注栈中实时计算下一个序号（undo/redo 后也能正确计数）
            let annotator_state = self.annotator_state.upgrade().unwrap();
            let max_existing = annotator_state
                .borrow()
                .annotations_stack
                .iter()
                .filter_map(|a| match a {
                    Annotation::SerialNumber(s) => Some(s.serial_number.number()),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            let number = (max_existing + 1).min(MAX_NUMBER);

            let annotation =
                SerialNumberAnnotation::new(pointer_pos, number, self.tool_state.style.clone());

            annotator_state
                .borrow_mut()
                .submit_annotation(annotation.into());
        }

        response
    }
}
