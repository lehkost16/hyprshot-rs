use gpui::{
    canvas, div, px, rgba, Bounds, Context, ElementInputHandler,
    EntityInputHandler, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, Render, Styled, UTF16Selection, Window,
};
use std::ops::Range;

pub struct TextEditor {
    pub focus_handle: FocusHandle,
    pub content: String,
    pub selected_range: Range<usize>,
    pub marked_range: Option<Range<usize>>,
    pub color: [u8; 4],
    pub font_size: f32,
}

impl TextEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: String::new(),
            selected_range: 0..0,
            marked_range: None,
            color: [255, 69, 58, 255],
            font_size: 18.0,
        }
    }

    pub fn reset(&mut self, initial: &str) {
        self.content = initial.to_string();
        self.selected_range = self.content.len()..self.content.len();
        self.marked_range = None;
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

impl EntityInputHandler for TextEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content.get(range)?.to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());

        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len());
        self.content.replace_range(start..end, new_text);
        self.selected_range = start + new_text.len()..start + new_text.len();
        self.marked_range = None;
        window.invalidate_character_coordinates();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());

        let start = range.start.min(self.content.len());
        let end = range.end.min(self.content.len());
        self.content.replace_range(start..end, new_text);

        if !new_text.is_empty() {
            self.marked_range = Some(start..start + new_text.len());
        } else {
            self.marked_range = None;
        }

        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|new_r| new_r.start + start..new_r.end + start)
            .unwrap_or_else(|| start + new_text.len()..start + new_text.len());

        window.invalidate_character_coordinates();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.content.len()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEditorEvent {
    Commit,
    Cancel,
}

impl gpui::EventEmitter<TextEditorEvent> for TextEditor {}

impl Focusable for TextEditor {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window);
        }
        let is_focused = self.focus_handle.is_focused(window);
        let display_text = if self.content.is_empty() {
            if is_focused {
                "|".to_string()
            } else {
                "".to_string()
            }
        } else {
            let mut s = self.content.clone();
            if is_focused {
                s.push('|');
            }
            s
        };

        let entity = cx.entity().clone();
        let focus_handle = self.focus_handle.clone();
        let color_u32 = ((self.color[0] as u32) << 24)
            | ((self.color[1] as u32) << 16)
            | ((self.color[2] as u32) << 8)
            | (self.color[3] as u32);

        div()
            .id("text-editor-container")
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, _cx| {
                    this.focus_handle.focus(window);
                    window.prevent_default();
                }),
            )
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, window, cx| {
                let ctrl = ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform;
                if ctrl && ev.keystroke.key.to_lowercase() == "v" {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        this.content.push_str(&text);
                        this.selected_range = this.content.len()..this.content.len();
                        window.invalidate_character_coordinates();
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                }
                match ev.keystroke.key.as_str() {
                    "enter" | "Return" => {
                        cx.stop_propagation();
                        if ev.keystroke.modifiers.shift {
                            this.content.push('\n');
                            this.selected_range = this.content.len()..this.content.len();
                            window.invalidate_character_coordinates();
                            cx.notify();
                        } else {
                            cx.emit(TextEditorEvent::Commit);
                        }
                    }
                    "escape" | "Escape" => {
                        cx.stop_propagation();
                        cx.emit(TextEditorEvent::Cancel);
                    }
                    "backspace" | "Backspace" => {
                        this.content.pop();
                        this.selected_range = this.content.len()..this.content.len();
                        this.marked_range = None;
                        window.invalidate_character_coordinates();
                        cx.stop_propagation();
                        cx.notify();
                    }
                    _ => {}
                }
            }))
            .min_w(px(4.0))
            .min_h(px(self.font_size))
            .p(px(0.0))
            .m(px(0.0))
            .border_0()
            .line_height(px(self.font_size))
            .text_size(px(self.font_size))
            .text_color(rgba(color_u32))
            .child(display_text)
            .child(
                canvas(
                    move |_bounds, _window, _cx| (),
                    move |bounds, (), window, cx| {
                        window.handle_input(
                            &focus_handle,
                            ElementInputHandler::new(bounds, entity),
                            cx,
                        );
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}
