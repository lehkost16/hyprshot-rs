use egui::{Event, ImeEvent, Key, Modifiers, PointerButton, Pos2, RawInput, Vec2};
use sctk::seat::keyboard::{KeyEvent, Keysym, Modifiers as SctkModifiers};
use sctk::seat::pointer::{PointerEvent, PointerEventKind};
use smithay_clipboard::Clipboard;
use std::sync::Arc;

pub struct EguiInput {
    pub raw: RawInput,
    pub modifiers: Modifiers,
    pub clipboard: Option<Arc<Clipboard>>,
}

impl EguiInput {
    pub fn new() -> Self {
        Self {
            raw: RawInput::default(),
            modifiers: Modifiers::default(),
            clipboard: None,
        }
    }

    pub fn with_clipboard(mut self, clipboard: Arc<Clipboard>) -> Self {
        self.clipboard = Some(clipboard);
        self
    }

    pub fn handle_pointer_event(&mut self, event: &PointerEvent) {
        let (x, y) = event.position;
        let pos = Pos2::new(x as f32, y as f32);

        match event.kind {
            PointerEventKind::Enter { .. } => {
                self.raw.events.push(Event::PointerMoved(pos));
            }
            PointerEventKind::Leave { .. } => {
                self.raw.events.push(Event::PointerGone);
            }
            PointerEventKind::Motion { .. } => {
                self.raw.events.push(Event::PointerMoved(pos));
            }
            PointerEventKind::Press { button, .. } => {
                if let Some(egui_button) = map_pointer_button(button) {
                    self.raw.events.push(Event::PointerButton {
                        pos,
                        button: egui_button,
                        pressed: true,
                        modifiers: self.raw.modifiers,
                    });
                }
            }
            PointerEventKind::Release { button, .. } => {
                if let Some(egui_button) = map_pointer_button(button) {
                    self.raw.events.push(Event::PointerButton {
                        pos,
                        button: egui_button,
                        pressed: false,
                        modifiers: self.raw.modifiers,
                    });
                }
            }
            PointerEventKind::Axis {
                horizontal,
                vertical,
                ..
            } => {
                let delta = Vec2::new(
                    horizontal.absolute as f32 + horizontal.discrete as f32 * 10.0,
                    vertical.absolute as f32 + vertical.discrete as f32 * 10.0,
                );
                log::info!(
                    "[DEBUG] EguiInput Axis event: delta={:?}, ctrl={}",
                    delta,
                    self.raw.modifiers.ctrl
                );
                self.raw.events.push(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta,
                    phase: egui::TouchPhase::Move,
                    modifiers: self.raw.modifiers,
                });
            }
        }
    }

    pub fn handle_keyboard_event(&mut self, event: KeyEvent, pressed: bool, repeat: bool) {
        let raw_keysym = event.keysym.raw();
        if is_ctrl_keysym(raw_keysym) {
            self.modifiers.ctrl = pressed;
            self.modifiers.command = pressed;
            self.raw.modifiers.ctrl = pressed;
            self.raw.modifiers.command = pressed;
        }

        if let Some(key) = map_keysym(event.keysym) {
            self.raw.events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat,
                modifiers: self.modifiers,
            });
            // 处理Ctrl+C/Ctrl+V
            if pressed && self.modifiers.ctrl {
                if key == Key::C {
                    self.raw.events.push(Event::Copy);
                } else if key == Key::V
                    && let Some(clipboard) = self.clipboard.clone()
                {
                    let text = clipboard.load_text();
                    if let Ok(text) = text {
                        self.raw.events.push(Event::Paste(text));
                    }
                }
            }
        }

        // For text input
        if pressed && self.modifiers.is_none() {
            if let Some(txt) = event.utf8 {
                if !txt.chars().any(|c| c.is_control()) {
                    self.raw.events.push(Event::Text(txt));
                }
            } else if let Some(txt) = event.keysym.key_char()
                && !txt.is_control()
            {
                self.raw.events.push(Event::Text(txt.to_string()));
            } else if repeat {
                let character = event.keysym.key_char();
                if let Some(txt) = character
                    && self.modifiers.is_none()
                {
                    self.raw.events.push(Event::Text(txt.to_string()));
                }
            }
        }
    }

    pub fn handle_ime_event(&mut self, event: &ImeEvent) {
        self.raw.events.push(Event::Ime(event.clone()));
    }

    pub fn update_modifiers(&mut self, modifiers: SctkModifiers) {
        let modifiers = Modifiers {
            alt: modifiers.alt,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            mac_cmd: modifiers.logo,
            command: modifiers.ctrl || modifiers.logo,
        };
        self.modifiers = modifiers;
        self.raw.modifiers = modifiers;
    }
}

fn is_ctrl_keysym(raw_keysym: u32) -> bool {
    matches!(raw_keysym, 0xffe3 | 0xffe4)
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[allow(dead_code)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Side,
    Extra,
    Unknown,
}

impl From<u32> for MouseButton {
    fn from(button: u32) -> Self {
        match button {
            0x110 => MouseButton::Left,   // BTN_LEFT
            0x111 => MouseButton::Right,  // BTN_RIGHT
            0x112 => MouseButton::Middle, // BTN_MIDDLE
            0x113 => MouseButton::Side,   // BTN_SIDE
            0x114 => MouseButton::Extra,  // BTN_EXTRA
            _ => MouseButton::Unknown,
        }
    }
}

fn map_pointer_button(button: u32) -> Option<PointerButton> {
    // These are linux/input-event-codes.h constants
    match button {
        0x110 => Some(PointerButton::Primary),   // BTN_LEFT
        0x111 => Some(PointerButton::Secondary), // BTN_RIGHT
        0x112 => Some(PointerButton::Middle),    // BTN_MIDDLE
        0x113 => Some(PointerButton::Extra1),    // BTN_SIDE
        0x114 => Some(PointerButton::Extra2),    // BTN_EXTRA
        _ => None,
    }
}

fn map_keysym(keysym: Keysym) -> Option<Key> {
    use sctk::seat::keyboard::Keysym as K;
    match keysym.raw() {
        0x0030 | 0xffb0 => Some(Key::Num0),
        0x0031 | 0xffb1 => Some(Key::Num1),
        0x0032 | 0xffb2 => Some(Key::Num2),
        0x0033 | 0xffb3 => Some(Key::Num3),
        0x0034 | 0xffb4 => Some(Key::Num4),
        0x0035 | 0xffb5 => Some(Key::Num5),
        0x0036 | 0xffb6 => Some(Key::Num6),
        0x0037 | 0xffb7 => Some(Key::Num7),
        0x0038 | 0xffb8 => Some(Key::Num8),
        0x0039 | 0xffb9 => Some(Key::Num9),
        _ => match keysym {
            K::Return => Some(Key::Enter),
            K::Tab => Some(Key::Tab),
            K::BackSpace => Some(Key::Backspace),
            K::Insert => Some(Key::Insert),
            K::Delete => Some(Key::Delete),
            K::Right => Some(Key::ArrowRight),
            K::Left => Some(Key::ArrowLeft),
            K::Down => Some(Key::ArrowDown),
            K::Up => Some(Key::ArrowUp),
            K::Page_Up => Some(Key::PageUp),
            K::Page_Down => Some(Key::PageDown),
            K::Home => Some(Key::Home),
            K::End => Some(Key::End),
            K::Escape => Some(Key::Escape),
            K::space => Some(Key::Space),
            K::A | K::a => Some(Key::A),
            K::B | K::b => Some(Key::B),
            K::C | K::c => Some(Key::C),
            K::D | K::d => Some(Key::D),
            K::E | K::e => Some(Key::E),
            K::F | K::f => Some(Key::F),
            K::G | K::g => Some(Key::G),
            K::H | K::h => Some(Key::H),
            K::I | K::i => Some(Key::I),
            K::J | K::j => Some(Key::J),
            K::K | K::k => Some(Key::K),
            K::L | K::l => Some(Key::L),
            K::M | K::m => Some(Key::M),
            K::N | K::n => Some(Key::N),
            K::O | K::o => Some(Key::O),
            K::P | K::p => Some(Key::P),
            K::Q | K::q => Some(Key::Q),
            K::R | K::r => Some(Key::R),
            K::S | K::s => Some(Key::S),
            K::T | K::t => Some(Key::T),
            K::U | K::u => Some(Key::U),
            K::V | K::v => Some(Key::V),
            K::W | K::w => Some(Key::W),
            K::X | K::x => Some(Key::X),
            K::Y | K::y => Some(Key::Y),
            K::Z | K::z => Some(Key::Z),
            _ => None,
        },
    }
}
