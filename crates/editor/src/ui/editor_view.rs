use crate::annotations::{Annotation, StrokeType, ToolKind};
use crate::clipboard::copy_to_clipboard;
use crate::config::{EditorPreferences, EditorToolState, ToolSettings};
use crate::image_save::save_png;
use crate::notify;
use crate::renderer::{
    fast_copy_rgba_to_pixmap, pixmap_to_render_image, render_annotation, render_composed_image,
};
use crate::ui::layout::{dock_layout, DockLayout};
use crate::ui::text_input::{TextEditor, TextEditorEvent};
use gpui::{
    div, img, prelude::*, px, rgb, rgba, size, svg, App, Context, CursorStyle, Entity,
    FocusHandle, Focusable, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, RenderImage, ScrollWheelEvent, Window,
};
use image::RgbaImage;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tiny_skia::Pixmap;

pub struct EditorView {
    focus_handle: FocusHandle,
    base_image: Arc<RgbaImage>,
    committed_pixmap: Pixmap,
    cached_render_image: Arc<RenderImage>,
    cache_dirty: bool,
    annotations: Vec<Annotation>,
    redo_stack: Vec<Annotation>,
    current_drawing: Option<Annotation>,

    // Tool & appearance state
    active_tool: Option<ToolKind>,
    current_color: [u8; 4],
    stroke_width: f32,
    is_filled: bool,
    marker_straight_mode: bool,

    text_editor: Entity<TextEditor>,
    is_text_typing: bool,
    text_origin: Point<f32>,

    // Window Zoom (Scales image and resizes the floating window)
    zoom: f32,
    initial_zoom: f32,
    toolbar_visible: bool,

    output_dir: PathBuf,
    pub preferences: EditorPreferences,
    pub tool_state: EditorToolState,
    final_tool_state: Option<Arc<Mutex<EditorToolState>>>,
    notifications: bool,
    notification_timeout: u32,
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EditorView {
    pub fn new(
        cx: &mut Context<Self>,
        base_image: Arc<RgbaImage>,
        output_dir: PathBuf,
        preferences: EditorPreferences,
        tool_state: EditorToolState,
        notifications: bool,
        notification_timeout: u32,
        initial_zoom: f32,
        final_tool_state: Option<Arc<Mutex<EditorToolState>>>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let width = base_image.width();
        let height = base_image.height();
        let mut committed_pixmap =
            Pixmap::new(width, height).expect("Failed to create tiny-skia pixmap");
        fast_copy_rgba_to_pixmap(&base_image, &mut committed_pixmap);
        let cached_render_image = pixmap_to_render_image(committed_pixmap.clone());

        let default_tool = ToolKind::from_config_value(&preferences.active_tool)
            .unwrap_or(ToolKind::Rectangle);
        let active_tool = if preferences.auto_activate_default_tool {
            Some(default_tool)
        } else {
            None
        };
        let marker_straight_mode = preferences.marker_pen_straight_mode;

        // Restore remembered settings for tool if available
        let (initial_width, initial_color, is_filled) = if let Some(ts) = tool_state.tool_settings.get(default_tool.name()) {
            let w = ts.stroke_width.unwrap_or(3.0);
            let c = ts.stroke_color_rgba.unwrap_or([255, 69, 58, 255]);
            let filled = ts.fill_color_rgba.is_some_and(|f| f[3] > 0);
            (w, c, filled)
        } else {
            (3.0, [255, 69, 58, 255], false)
        };

        let text_editor = cx.new(|cx| TextEditor::new(cx));
        cx.subscribe(&text_editor, |this: &mut EditorView, _, event: &TextEditorEvent, cx| {
            match event {
                TextEditorEvent::Commit => {
                    this.commit_text(cx);
                }
                TextEditorEvent::Cancel => {
                    this.cancel_text(cx);
                }
            }
        }).detach();

        let mut this = Self {
            focus_handle,
            base_image,
            committed_pixmap,
            cached_render_image,
            cache_dirty: false,
            annotations: Vec::new(),
            redo_stack: Vec::new(),
            current_drawing: None,

            active_tool,
            current_color: initial_color,
            stroke_width: initial_width,
            is_filled,
            marker_straight_mode,

            text_editor,
            is_text_typing: false,
            text_origin: Point::new(0.0, 0.0),

            zoom: initial_zoom,
            initial_zoom,
            toolbar_visible: true,

            output_dir,
            preferences,
            tool_state,
            final_tool_state,
            notifications,
            notification_timeout,
        };
        this.save_active_tool_settings();
        this
    }

    pub fn save_active_tool_settings(&mut self) {
        if let Some(tool) = self.active_tool {
            self.tool_state.tool_settings.insert(
                tool.name().to_string(),
                ToolSettings {
                    stroke_width: Some(self.stroke_width),
                    stroke_color_rgba: Some(self.current_color),
                    fill_color_rgba: if self.is_filled {
                        Some(self.current_color)
                    } else {
                        Some([0, 0, 0, 0])
                    },
                    font_color_rgba: Some(self.current_color),
                },
            );
        }
        if let Some(shared) = &self.final_tool_state {
            if let Ok(mut lock) = shared.lock() {
                *lock = self.tool_state.clone();
            }
        }
    }

    pub fn apply_tool_settings(&mut self, tool: ToolKind) {
        if let Some(ts) = self.tool_state.tool_settings.get(tool.name()) {
            if let Some(c) = ts.stroke_color_rgba {
                self.current_color = c;
            }
            if tool == ToolKind::Text {
                self.stroke_width = ts.stroke_width.unwrap_or(18.0).max(14.0);
            } else {
                self.stroke_width = 3.0;
            }
        } else if tool == ToolKind::Text {
            self.stroke_width = 18.0;
        } else {
            self.stroke_width = 3.0;
        }
        self.is_filled = false;
    }

    pub fn toggle_tool(&mut self, tool: ToolKind, window: &mut Window) {
        let new_tool = if self.active_tool == Some(tool) {
            None
        } else {
            Some(tool)
        };
        self.set_active_tool(new_tool, window);
    }

    pub fn set_active_tool(&mut self, new_tool: Option<ToolKind>, window: &mut Window) {
        self.save_active_tool_settings();
        self.active_tool = new_tool;
        if let Some(tool) = new_tool {
            self.apply_tool_settings(tool);
        }
        self.sync_window_size(window);
    }

    pub fn current_tool_state(&self) -> EditorToolState {
        let mut state = self.tool_state.clone();
        if let Some(tool) = self.active_tool {
            state.tool_settings.insert(
                tool.name().to_string(),
                ToolSettings {
                    stroke_width: Some(self.stroke_width),
                    stroke_color_rgba: Some(self.current_color),
                    fill_color_rgba: if self.is_filled {
                        Some(self.current_color)
                    } else {
                        Some([0, 0, 0, 0])
                    },
                    font_color_rgba: Some(self.current_color),
                },
            );
        }
        state
    }

    pub fn secondary_toolbar_width(&self) -> f32 {
        crate::ui::layout::secondary_toolbar_width_for_tool(self.active_tool)
    }

    pub fn current_dock(&self) -> DockLayout {
        let img_w = self.base_image.width() as f32 * self.zoom;
        let img_h = self.base_image.height() as f32 * self.zoom;
        let sec_w = self.secondary_toolbar_width();
        dock_layout(img_w, img_h, sec_w, self.toolbar_visible)
    }

    pub fn sync_window_size(&self, window: &mut Window) {
        let dock = self.current_dock();
        window.resize(size(px(dock.window_width), px(dock.window_height)));
    }

    fn update_committed(&mut self) {
        let width = self.base_image.width();
        let height = self.base_image.height();
        let mut pixmap =
            Pixmap::new(width, height).expect("Failed to create tiny-skia pixmap");
        fast_copy_rgba_to_pixmap(&self.base_image, &mut pixmap);
        for ann in &self.annotations {
            render_annotation(&mut pixmap, &self.base_image, ann);
        }
        self.committed_pixmap = pixmap;
        self.cache_dirty = true;
    }

    fn rebuild_cache(&mut self) {
        let mut pixmap = self.committed_pixmap.clone();
        if let Some(cur) = &self.current_drawing {
            render_annotation(&mut pixmap, &self.base_image, cur);
        }
        self.cached_render_image = pixmap_to_render_image(pixmap);
        self.cache_dirty = false;
    }

    pub fn commit_text(&mut self, cx: &mut Context<Self>) {
        if !self.is_text_typing {
            return;
        }
        let text = self.text_editor.read(cx).content.trim().to_string();
        let font_sz = if self.stroke_width >= 12.0 {
            self.stroke_width
        } else {
            18.0
        };
        let color = self.text_editor.read(cx).color;
        if !text.is_empty() {
            self.annotations.push(Annotation::Text {
                pos: self.text_origin,
                text,
                color,
                font_size: font_sz,
            });
            self.redo_stack.clear();
            self.update_committed();
        }
        self.is_text_typing = false;
        self.text_editor.update(cx, |ed, _| ed.reset(""));
        self.cache_dirty = true;
        cx.notify();
    }

    pub fn cancel_text(&mut self, cx: &mut Context<Self>) {
        self.is_text_typing = false;
        self.text_editor.update(cx, |ed, _| ed.reset(""));
        self.cache_dirty = true;
        cx.notify();
    }

    fn to_image_coords(&self, pos: Point<Pixels>) -> Point<f32> {
        let pos_x: f32 = pos.x.into();
        let pos_y: f32 = pos.y.into();
        Point::new(pos_x / self.zoom, pos_y / self.zoom)
    }

    pub fn undo(&mut self) {
        if let Some(last) = self.annotations.pop() {
            self.redo_stack.push(last);
            self.update_committed();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.annotations.push(next);
            self.update_committed();
        }
    }

    pub fn copy_and_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_text_typing {
            self.commit_text(cx);
        }
        let final_image = render_composed_image(&self.base_image, &self.annotations);
        if let Ok(()) = copy_to_clipboard(&final_image) {
            if self.notifications {
                notify::copied(self.notification_timeout);
            }
        }
        if self.preferences.exit_after_copy {
            window.remove_window();
        }
    }

    pub fn save_and_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_text_typing {
            self.commit_text(cx);
        }
        let final_image = render_composed_image(&self.base_image, &self.annotations);
        let mut png_bytes = Vec::new();
        if final_image
            .write_to(
                &mut std::io::Cursor::new(&mut png_bytes),
                image::ImageFormat::Png,
            )
            .is_ok()
        {
            if let Ok(saved_path) = save_png(&png_bytes, &self.output_dir) {
                let _ = copy_to_clipboard(&final_image);
                if self.notifications {
                    notify::saved(saved_path.display(), self.notification_timeout);
                }
            }
        }
        if self.preferences.exit_after_save {
            window.remove_window();
        }
    }
}

const PALETTE: &[[u8; 4]] = &[
    [255, 69, 58, 255],   // 鲜红 #ff453a
    [255, 159, 10, 255],  // 活力橙 #ff9f0a
    [255, 214, 10, 255],  // 明黄 #ffd60a
    [48, 209, 88, 255],   // 翠绿 #30d158
    [100, 210, 255, 255], // 青天蓝 #64d2ff
    [10, 132, 255, 255],  // 极速蓝 #0a84ff
    [191, 90, 242, 255],  // 魅紫 #bf5af2
    [255, 55, 95, 255],   // 玫瑰粉 #ff375f
    [255, 255, 255, 255], // 纯白 #ffffff
    [28, 28, 30, 255],    // 深黑 #1c1c1e
];

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.cache_dirty {
            self.rebuild_cache();
        }

        let img_w = self.base_image.width() as f32 * self.zoom;
        let img_h = self.base_image.height() as f32 * self.zoom;
        let dock = self.current_dock();

        let active_tool = self.active_tool;
        let cur_color = self.current_color;
        let stroke_width = self.stroke_width;
        let show_font_size = active_tool == Some(ToolKind::Text);
        let show_palette = matches!(
            active_tool,
            Some(ToolKind::Rectangle)
                | Some(ToolKind::Ellipse)
                | Some(ToolKind::Line)
                | Some(ToolKind::Arrow)
                | Some(ToolKind::Freehand)
                | Some(ToolKind::Highlight)
                | Some(ToolKind::Text)
                | Some(ToolKind::Badge)
        );
        let canvas_cursor = if self.active_tool.is_none() || self.active_tool == Some(ToolKind::Select) {
            CursorStyle::OpenHand
        } else if self.active_tool == Some(ToolKind::Text) {
            CursorStyle::IBeam
        } else {
            CursorStyle::Crosshair
        };

        let is_text_typing = self.is_text_typing;
        if !is_text_typing && !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window);
        }

        div()
            .when(!is_text_typing, |d| d.track_focus(&self.focus_handle))
            .relative()
            .size_full()
            .cursor(canvas_cursor)
            .bg(rgba(0x00000000))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |_this, _ev: &MouseDownEvent, window, _cx| {
                    window.start_window_move();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _ev: &MouseDownEvent, window, cx| {
                    if this.current_drawing.is_some() {
                        this.current_drawing = None;
                        this.cache_dirty = true;
                        cx.notify();
                        return;
                    }
                    if this.is_text_typing {
                        this.cancel_text(cx);
                        return;
                    }
                    if this.active_tool.is_some() {
                        this.set_active_tool(None, window);
                        cx.notify();
                        return;
                    }
                    window.remove_window();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    if ev.modifiers.alt
                        || this.active_tool == Some(ToolKind::Select)
                        || this.active_tool.is_none()
                    {
                        window.start_window_move();
                        return;
                    }

                    let p = this.to_image_coords(ev.position);

                    if this.is_text_typing && this.active_tool != Some(ToolKind::Text) {
                        this.commit_text(cx);
                    }

                    match this.active_tool {
                        Some(ToolKind::Rectangle) => {
                            this.current_drawing = Some(Annotation::Rectangle {
                                start: p,
                                end: p,
                                color: this.current_color,
                                width: 3.0,
                                stroke_type: StrokeType::SolidLine,
                                fill_color: None,
                            });
                        }
                        Some(ToolKind::Ellipse) => {
                            this.current_drawing = Some(Annotation::Ellipse {
                                start: p,
                                end: p,
                                color: this.current_color,
                                width: 3.0,
                                stroke_type: StrokeType::SolidLine,
                                fill_color: None,
                            });
                        }
                        Some(ToolKind::Line) => {
                            this.current_drawing = Some(Annotation::Line {
                                start: p,
                                end: p,
                                color: this.current_color,
                                width: 3.0,
                                stroke_type: StrokeType::SolidLine,
                            });
                        }
                        Some(ToolKind::Arrow) => {
                            this.current_drawing = Some(Annotation::Arrow {
                                start: p,
                                end: p,
                                color: this.current_color,
                                width: 3.0,
                            });
                        }
                        Some(ToolKind::Freehand) => {
                            this.current_drawing = Some(Annotation::Freehand {
                                points: vec![p],
                                color: this.current_color,
                                width: 3.0,
                            });
                        }
                        Some(ToolKind::Highlight) => {
                            this.current_drawing = Some(Annotation::Highlight {
                                points: vec![p],
                                color: this.current_color,
                                width: 6.0,
                                straight: this.marker_straight_mode,
                            });
                        }
                        Some(ToolKind::Mosaic) => {
                            this.current_drawing = Some(Annotation::Mosaic {
                                rect: [p.x, p.y, p.x, p.y],
                                block_size: 10.0,
                            });
                        }
                        Some(ToolKind::Blur) => {
                            this.current_drawing = Some(Annotation::Blur {
                                rect: [p.x, p.y, p.x, p.y],
                                sigma: 3.0,
                            });
                        }
                        Some(ToolKind::Eraser) => {
                            this.current_drawing = Some(Annotation::Eraser {
                                rect: [p.x, p.y, p.x, p.y],
                            });
                        }
                        Some(ToolKind::Badge) => {
                            let max_existing = this
                                .annotations
                                .iter()
                                .filter_map(|a| match a {
                                    Annotation::Badge { number, .. } => Some(*number),
                                    _ => None,
                                    })
                                .max()
                                .unwrap_or(0);
                            let number = (max_existing + 1).min(99);
                            let badge = Annotation::Badge {
                                pos: p,
                                number,
                                color: this.current_color,
                                radius: 14.0,
                            };
                            this.annotations.push(badge);
                            this.redo_stack.clear();
                            this.update_committed();
                        }
                        Some(ToolKind::Text) => {
                            if this.is_text_typing {
                                this.commit_text(cx);
                            }
                            this.is_text_typing = true;
                            this.text_origin = p;
                            let font_sz = if this.stroke_width >= 12.0 {
                                this.stroke_width
                            } else {
                                18.0
                            };
                            let col = this.current_color;
                            let zm = this.zoom;
                            this.text_editor.update(cx, |ed, _| {
                                ed.reset("");
                                ed.color = col;
                                ed.font_size = font_sz * zm;
                            });
                            this.text_editor.read(cx).focus_handle.focus(window);
                            window.prevent_default();
                            window.invalidate_character_coordinates();
                            this.cache_dirty = true;
                        }
                        _ => {}
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
                if this.current_drawing.is_some() {
                    let p = this.to_image_coords(ev.position);
                    if let Some(drawing) = &mut this.current_drawing {
                        match drawing {
                            Annotation::Rectangle { end, .. }
                            | Annotation::Ellipse { end, .. }
                            | Annotation::Line { end, .. }
                            | Annotation::Arrow { end, .. } => {
                                *end = p;
                            }
                            Annotation::Freehand { points, .. } => {
                                if points.last().map_or(true, |last| ((last.x - p.x).powi(2) + (last.y - p.y).powi(2)) > 4.0) {
                                    points.push(p);
                                }
                            }
                            Annotation::Highlight { points, straight, .. } => {
                                if *straight {
                                    if points.len() > 1 {
                                        points.truncate(1);
                                    }
                                    points.push(p);
                                } else if points.last().map_or(true, |last| ((last.x - p.x).powi(2) + (last.y - p.y).powi(2)) > 4.0) {
                                    points.push(p);
                                }
                            }
                            Annotation::Mosaic { rect, .. }
                            | Annotation::Blur { rect, .. }
                            | Annotation::Eraser { rect, .. } => {
                                rect[2] = p.x;
                                rect[3] = p.y;
                            }
                            _ => {}
                        }
                    }
                    this.cache_dirty = true;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _ev: &MouseUpEvent, _, cx| {
                    if let Some(mut drawing) = this.current_drawing.take() {
                        match &mut drawing {
                            Annotation::Mosaic { rect, .. }
                            | Annotation::Blur { rect, .. }
                            | Annotation::Eraser { rect, .. } => {
                                let min_x = rect[0].min(rect[2]);
                                let min_y = rect[1].min(rect[3]);
                                let max_x = rect[0].max(rect[2]);
                                let max_y = rect[1].max(rect[3]);
                                *rect = [min_x, min_y, max_x, max_y];
                            }
                            _ => {}
                        }
                        this.annotations.push(drawing);
                        this.redo_stack.clear();
                        this.update_committed();
                        cx.notify();
                    }
                }),
            )
            .on_scroll_wheel(cx.listener(move |this, ev: &ScrollWheelEvent, window, cx| {
                let delta = match ev.delta {
                    gpui::ScrollDelta::Lines(l) => l.y * 0.1,
                    gpui::ScrollDelta::Pixels(p) => {
                        let py: f32 = p.y.into();
                        py * 0.002
                    }
                };
                let old_zoom = this.zoom;
                let new_zoom = (old_zoom * (1.0 + delta)).clamp(0.1, 10.0);
                if (new_zoom - old_zoom).abs() > 1e-4 {
                    this.zoom = new_zoom;
                    this.sync_window_size(window);
                    cx.notify();
                }
            }))
            .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                let key = ev.keystroke.key.as_str();
                let ctrl = ev.keystroke.modifiers.control || ev.keystroke.modifiers.platform;

                if ctrl {
                    match key {
                        // Undo: Ctrl+Z or Ctrl+U
                        "z" | "Z" => {
                            if ev.keystroke.modifiers.shift {
                                this.redo();
                            } else {
                                this.undo();
                            }
                            cx.notify();
                            return;
                        }
                        "u" | "U" => {
                            this.undo();
                            cx.notify();
                            return;
                        }
                        // Redo: Ctrl+Y or Ctrl+Shift+Z or Ctrl+R
                        "y" | "Y" | "r" | "R" => {
                            this.redo();
                            cx.notify();
                            return;
                        }
                        // Copy: Ctrl+C
                        "c" | "C" => {
                            this.copy_and_quit(window, cx);
                            return;
                        }
                        // Save: Ctrl+S
                        "s" | "S" => {
                            this.save_and_quit(window, cx);
                            return;
                        }
                        // Reset Zoom: Ctrl+0
                        "0" => {
                            this.zoom = this.initial_zoom;
                            this.sync_window_size(window);
                            cx.notify();
                            return;
                        }
                        _ => {}
                    }
                }

                if this.is_text_typing {
                    match key {
                        "enter" | "Return" => {
                            this.commit_text(cx);
                            return;
                        }
                        "escape" | "Escape" => {
                            this.cancel_text(cx);
                            return;
                        }
                        _ => return,
                    }
                }

                // Tool shortcuts and window control when not typing text
                if key == "escape" || key == "Escape" {
                    if this.active_tool.is_some() {
                        this.set_active_tool(None, window);
                        cx.notify();
                    } else {
                        window.remove_window();
                    }
                    return;
                }

                // Single key tool selection shortcuts
                let tool_toggle = match key {
                    "r" | "R" => Some(ToolKind::Rectangle),
                    "o" | "O" | "e" | "E" => Some(ToolKind::Ellipse),
                    "l" | "L" => Some(ToolKind::Line),
                    "a" | "A" => Some(ToolKind::Arrow),
                    "p" | "P" => Some(ToolKind::Freehand),
                    "h" | "H" => Some(ToolKind::Highlight),
                    "m" | "M" => Some(ToolKind::Mosaic),
                    "b" | "B" => Some(ToolKind::Blur),
                    "t" | "T" => Some(ToolKind::Text),
                    "n" | "N" => Some(ToolKind::Badge),
                    "x" | "X" => Some(ToolKind::Eraser),
                    _ => None,
                };
                if let Some(tool) = tool_toggle {
                    this.toggle_tool(tool, window);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(0.0))
                    .w(px(img_w))
                    .h(px(img_h))
                    .child(
                        img(self.cached_render_image.clone())
                            .size_full()
                    ),
            )
            .when(self.is_text_typing, |parent| {
                let x = self.text_origin.x * self.zoom;
                let y = self.text_origin.y * self.zoom;
                parent.child(
                    div()
                        .occlude()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .child(self.text_editor.clone()),
                )
            })
            .when(dock.secondary_visible, |parent| {
                parent.child(
                    div()
                        .occlude()
                        .absolute()
                        .top(px(dock.secondary_y))
                        .left(px(dock.secondary_x))
                        .w(px(dock.secondary_width))
                        .h(px(dock.secondary_height))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap(px(10.0))
                        .px(px(10.0))
                        .py(px(4.0))
                        .bg(rgba(0x262629ff))
                        .border_1()
                        .border_color(rgba(0x4c4c52ff))
                        .rounded(px(8.0))
                        .shadow_lg()
                        // Font size options for Text tool: 14, 18, 24, 32, 44
                        .when(show_font_size, |p| {
                            p.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(3.0))
                                    .children([14.0f32, 18.0, 24.0, 32.0, 44.0].iter().map(|&sz| {
                                        let is_active = (stroke_width - sz).abs() < 1.0;
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .w(px(26.0))
                                            .h(px(22.0))
                                            .rounded(px(4.0))
                                            .cursor_pointer()
                                            .bg(if is_active { rgb(0x0860f2) } else { rgba(0x00000000) })
                                            .border_1()
                                            .border_color(if is_active { rgb(0x60a5fa) } else { rgb(0x4c4c52) })
                                            .hover(|s| s.bg(rgb(0x3a3a3f)))
                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                this.stroke_width = sz;
                                                this.save_active_tool_settings();
                                                if this.is_text_typing {
                                                    let zm = this.zoom;
                                                    this.text_editor.update(cx, |ed, _| ed.font_size = sz * zm);
                                                }
                                                cx.notify();
                                            }))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0xffffff))
                                                    .child(format!("{:.0}", sz))
                                            )
                                    }))
                            )
                        })
                        // Palette section (Current color + 10 candidate colors)
                        .when(show_palette, |p| {
                            let cur_c = cur_color;
                            p.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    // 1. Current color preview
                                    .child(
                                        div()
                                            .size(px(20.0))
                                            .rounded(px(4.0))
                                            .bg(rgba(((cur_c[0] as u32) << 24) | ((cur_c[1] as u32) << 16) | ((cur_c[2] as u32) << 8) | (cur_c[3] as u32)))
                                            .border(px(2.0))
                                            .border_color(rgb(0xffffff))
                                    )
                                    // 2. Candidate colors
                                    .children(PALETTE.iter().map(|&c| {
                                        let is_active = c == cur_color;
                                        div()
                                            .size(px(20.0))
                                            .rounded(px(4.0))
                                            .bg(rgba(((c[0] as u32) << 24) | ((c[1] as u32) << 16) | ((c[2] as u32) << 8) | (c[3] as u32)))
                                            .border(if is_active { px(2.0) } else { px(1.0) })
                                            .border_color(if is_active {
                                                rgb(0xffffff)
                                            } else if c == [28, 28, 30, 255] {
                                                rgb(0x555968)
                                            } else {
                                                rgb(0x161821)
                                            })
                                            .cursor_pointer()
                                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                                                this.current_color = c;
                                                this.save_active_tool_settings();
                                                if this.is_text_typing {
                                                    this.text_editor.update(cx, |ed, _| ed.color = c);
                                                }
                                                cx.notify();
                                            }))
                                    }))
                            )
                        })
                )
            })
            // Primary Toolbar docked to image
            .when(dock.primary_visible, |parent| {
                parent.child(
                    div()
                        .occlude()
                        .absolute()
                        .top(px(dock.primary_y))
                        .left(px(dock.primary_x))
                        .w(px(dock.primary_width))
                        .h(px(dock.primary_height))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap(px(4.0))
                        .px(px(10.0))
                        .py(px(5.0))
                        .bg(rgba(0x1c1c1eff))
                        .border_1()
                        .border_color(rgba(0x4c4c52ff))
                        .rounded(px(8.0))
                        .shadow_lg()
                        // 11 Annotation Tools (28x28px each)
                        .child(toolbar_button("draw-rectangle.svg", active_tool == Some(ToolKind::Rectangle), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Rectangle, window);
                            cx.notify();
                        })))
                        .child(toolbar_button("draw-ellipse.svg", active_tool == Some(ToolKind::Ellipse), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Ellipse, window);
                            cx.notify();
                        })))
                        .child(toolbar_button("draw-line.svg", active_tool == Some(ToolKind::Line), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Line, window);
                            cx.notify();
                        })))
                        .child(toolbar_button("draw-arrow.svg", active_tool == Some(ToolKind::Arrow), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Arrow, window);
                            cx.notify();
                        })))
                        .child(toolbar_button("draw-freehand.svg", active_tool == Some(ToolKind::Freehand), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Freehand, window);
                            cx.notify();
                        })))
                        .child(toolbar_button("draw-highlight.svg", active_tool == Some(ToolKind::Highlight), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Highlight, window);
                            cx.notify();
                        })))
                        .child(toolbar_button("pixelart-trace.svg", active_tool == Some(ToolKind::Mosaic), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Mosaic, window);
                            cx.notify();
                        })))
                        .child(toolbar_button("draw-text.svg", active_tool == Some(ToolKind::Text), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Text, window);
                            cx.notify();
                        })))
                        .child(toolbar_button("draw-number.svg", active_tool == Some(ToolKind::Badge), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Badge, window);
                            cx.notify();
                        })))
                        .child(toolbar_button("draw-eraser.svg", active_tool == Some(ToolKind::Eraser), false, cx.listener(|this, _, window, cx| {
                            this.toggle_tool(ToolKind::Eraser, window);
                            cx.notify();
                        })))
                        // Actions (5 items): Undo, Redo, Zoom reset, Save, Copy (matching original order exactly)
                        .child(toolbar_button("edit-undo.svg", false, self.annotations.is_empty(), cx.listener(|this, _, _, cx| {
                            this.undo();
                            cx.notify();
                        })))
                        .child(toolbar_button("edit-redo.svg", false, self.redo_stack.is_empty(), cx.listener(|this, _, _, cx| {
                            this.redo();
                            cx.notify();
                        })))
                        .child(toolbar_button("zoom-original.svg", false, false, cx.listener(|this, _, window, cx| {
                            this.zoom = this.initial_zoom;
                            this.sync_window_size(window);
                            cx.notify();
                        })))
                        .child(toolbar_button("document-save.svg", false, false, cx.listener(|this, _, window, cx| {
                            this.save_and_quit(window, cx);
                        })))
                        .child(toolbar_button("edit-copy.svg", false, false, cx.listener(|this, _, window, cx| {
                            this.copy_and_quit(window, cx);
                        })))
                )
            })
            // Top-right Toolbar Toggle
            .child(
                div()
                    .occlude()
                    .absolute()
                    .top(px(8.0))
                    .right(px(8.0))
                    .size(px(28.0))
                    .bg(rgb(0x1c1c1e))
                    .rounded(px(6.0))
                    .shadow_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0x3a3a3f)))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        this.toolbar_visible = !this.toolbar_visible;
                        this.sync_window_size(window);
                        cx.notify();
                    }))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .relative()
                            .size(px(12.0))
                            .child(
                                div()
                                    .absolute()
                                    .top(px(5.0))
                                    .left(px(0.0))
                                    .w(px(12.0))
                                    .h(px(2.0))
                                    .bg(rgb(0xffffff)),
                            )
                            .when(!self.toolbar_visible, |btn| {
                                btn.child(
                                    div()
                                        .absolute()
                                        .left(px(5.0))
                                        .top(px(0.0))
                                        .w(px(2.0))
                                        .h(px(12.0))
                                        .bg(rgb(0xffffff)),
                                )
                            }),
                    ),
            )
    }
}

fn toolbar_button(
    icon_name: &'static str,
    is_active: bool,
    is_disabled: bool,
    handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(28.0))
        .rounded(px(7.0))
        .bg(if is_active {
            rgb(0x0860f2)
        } else {
            rgba(0x00000000)
        })
        .border_1()
        .border_color(if is_active {
            rgb(0x60a5fa)
        } else {
            rgba(0x00000000)
        })
        .when(!is_disabled, |this| {
            this.cursor_pointer()
                .hover(|s| s.bg(rgb(0x3a3a3f)).border_color(rgb(0x4c4c52)))
                .on_mouse_down(MouseButton::Left, handler)
        })
        .child(
            svg()
                .path(icon_name)
                .size(px(20.0))
                .text_color(if is_disabled {
                    rgba(0xffffff50)
                } else if is_active {
                    rgb(0xffffff)
                } else {
                    rgb(0xeff0f1)
                }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_coordinates_match_screen_space() {
        let dock = dock_layout(800.0, 600.0, 518.0, true);
        assert_eq!(dock.window_width, 800.0);
        assert_eq!(dock.window_height, 686.0);
        assert_eq!(dock.primary_y, 604.0);
        assert_eq!(dock.secondary_y, 648.0);
    }
}
