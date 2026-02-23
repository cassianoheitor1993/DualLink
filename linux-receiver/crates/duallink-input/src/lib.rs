//! duallink-input — Input event capture and forwarding utilities
//!
//! This crate provides two input capture paths:
//!
//! ## 1. GStreamer Navigation Events (primary, via `duallink-decoder`)
//! When the receiver renders video through a GStreamer `autovideosink` window,
//! the sink emits navigation bus messages for mouse and keyboard events.
//! `DisplayDecoder::poll_input_events()` in `duallink-decoder` drains that bus
//! and returns `Vec<InputEvent>`.  The `duallink-app` decode thread forwards
//! them to the Mac via `InputSender::try_send`.
//!
//! ## 2. Egui Input Bridge (secondary, for the status/setup window)
//! `EguiInputBridge` converts egui pointer and keyboard events to `InputEvent`
//! values for use when the display is rendered inside an egui panel rather than
//! a standalone GStreamer window.  Coordinates are normalised to [0.0, 1.0].
//!
//! ## Serialisation
//! All `InputEvent` values are JSON-serialised and sent over the existing TLS
//! TCP signaling connection (Linux → Mac direction) as `input_event` messages.

use duallink_core::{GesturePhase, InputEvent, MouseButton};
use egui::{Event, Key, PointerButton, Rect};
use tracing::trace;

// ── EguiInputBridge ────────────────────────────────────────────────────────────

/// Converts egui `Event` values to `InputEvent` values.
///
/// Requires the viewport (display area) rect so it can normalise absolute
/// pixel positions to the [0.0, 1.0] range expected by the Mac client.
///
/// # Example
/// ```rust,no_run
/// # use duallink_input::EguiInputBridge;
/// # use egui::Rect;
/// let bridge = EguiInputBridge::new();
/// // In the egui update() closure:
/// // let events = ctx.input(|i| bridge.convert(&i.events, display_rect));
/// ```
#[derive(Debug, Default)]
pub struct EguiInputBridge {
    /// Last normalised mouse position — used to attach position to scroll
    /// events which egui emits without an explicit coord.
    last_pos: Option<(f64, f64)>,
}

impl EguiInputBridge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convert a slice of egui events to `InputEvent` values.
    ///
    /// `viewport` is the on-screen rect occupied by the display panel so
    /// pointer coordinates can be normalised.  Returns only events that map
    /// directly to `InputEvent` variants; UI-only egui events are silently
    /// dropped.
    pub fn convert(&mut self, events: &[Event], viewport: Rect) -> Vec<InputEvent> {
        let mut out = Vec::new();
        for ev in events {
            if let Some(ie) = self.map_event(ev, viewport) {
                out.push(ie);
            }
        }
        out
    }

    fn normalise(&self, px: f32, py: f32, vp: Rect) -> (f64, f64) {
        let w = vp.width().max(1.0);
        let h = vp.height().max(1.0);
        let nx = ((px - vp.left()) / w).clamp(0.0, 1.0) as f64;
        let ny = ((py - vp.top()) / h).clamp(0.0, 1.0) as f64;
        (nx, ny)
    }

    fn egui_button(btn: PointerButton) -> MouseButton {
        match btn {
            PointerButton::Primary => MouseButton::Left,
            PointerButton::Secondary => MouseButton::Right,
            PointerButton::Middle => MouseButton::Middle,
            _ => MouseButton::Left,
        }
    }

    fn map_event(&mut self, ev: &Event, vp: Rect) -> Option<InputEvent> {
        match ev {
            // ── Pointer ────────────────────────────────────────────────────
            Event::PointerMoved(pos) => {
                let (nx, ny) = self.normalise(pos.x, pos.y, vp);
                self.last_pos = Some((nx, ny));
                trace!("egui PointerMoved → MouseMove ({:.3}, {:.3})", nx, ny);
                Some(InputEvent::MouseMove { x: nx, y: ny })
            }

            Event::PointerButton { pos, button, pressed, .. } => {
                let (nx, ny) = self.normalise(pos.x, pos.y, vp);
                self.last_pos = Some((nx, ny));
                let btn = Self::egui_button(*button);
                if *pressed {
                    trace!("egui PointerButton → MouseDown {:?}", btn);
                    Some(InputEvent::MouseDown { x: nx, y: ny, button: btn })
                } else {
                    trace!("egui PointerButton → MouseUp {:?}", btn);
                    Some(InputEvent::MouseUp { x: nx, y: ny, button: btn })
                }
            }

            // ── Scroll ─────────────────────────────────────────────────────
            Event::MouseWheel { unit, delta, .. } => {
                let (x, y) = self.last_pos.unwrap_or((0.5, 0.5));
                let (dx, dy) = match unit {
                    egui::MouseWheelUnit::Line  => (delta.x as f64 * 3.0,  delta.y as f64 * 3.0),
                    egui::MouseWheelUnit::Page  => (delta.x as f64 * 30.0, delta.y as f64 * 30.0),
                    egui::MouseWheelUnit::Point => (delta.x as f64,        delta.y as f64),
                };
                Some(InputEvent::MouseScroll { x, y, delta_x: dx, delta_y: dy })
            }

            // ── Keyboard ───────────────────────────────────────────────────
            Event::Key { key, pressed, .. } => {
                let kc = key_to_x11_keyval(*key);
                if *pressed {
                    let text = key_to_text(*key);
                    Some(InputEvent::KeyDown { keycode: kc, text })
                } else {
                    Some(InputEvent::KeyUp { keycode: kc })
                }
            }

            // ── Text input ─────────────────────────────────────────────────
            // egui emits Text events for printable chars typed; map to
            // synthetic KeyDown/KeyUp with keycode 0 and the text payload.
            Event::Text(s) if !s.is_empty() => {
                Some(InputEvent::KeyDown { keycode: 0, text: Some(s.clone()) })
            }

            // ── Touchpad gestures (egui 0.29+) ─────────────────────────────
            Event::Zoom(factor) => {
                let (x, y) = self.last_pos.unwrap_or((0.5, 0.5));
                let mag = (*factor as f64) - 1.0; // delta from unity
                Some(InputEvent::GesturePinch { x, y, magnification: mag, phase: GesturePhase::Changed })
            }

            _ => None,
        }
    }
}

// ── X11 keyval mapping ─────────────────────────────────────────────────────────

/// Map an egui `Key` to the corresponding X11 keysym value.
///
/// The Mac client's `InputInjectionManager` uses the keycode field to
/// drive `CGEvent` key events.  We use X11 keysyms as the platform-neutral
/// wire format (matching the GStreamer navigation path).
pub fn key_to_x11_keyval(key: Key) -> u32 {
    // Latin letters 0x0061–0x007a (lowercase)
    match key {
        Key::A => 0x0061, Key::B => 0x0062, Key::C => 0x0063,
        Key::D => 0x0064, Key::E => 0x0065, Key::F => 0x0066,
        Key::G => 0x0067, Key::H => 0x0068, Key::I => 0x0069,
        Key::J => 0x006a, Key::K => 0x006b, Key::L => 0x006c,
        Key::M => 0x006d, Key::N => 0x006e, Key::O => 0x006f,
        Key::P => 0x0070, Key::Q => 0x0071, Key::R => 0x0072,
        Key::S => 0x0073, Key::T => 0x0074, Key::U => 0x0075,
        Key::V => 0x0076, Key::W => 0x0077, Key::X => 0x0078,
        Key::Y => 0x0079, Key::Z => 0x007a,
        // Digits
        Key::Num0 => 0x0030, Key::Num1 => 0x0031, Key::Num2 => 0x0032,
        Key::Num3 => 0x0033, Key::Num4 => 0x0034, Key::Num5 => 0x0035,
        Key::Num6 => 0x0036, Key::Num7 => 0x0037, Key::Num8 => 0x0038,
        Key::Num9 => 0x0039,
        // Function keys
        Key::F1  => 0xffbe, Key::F2  => 0xffbf, Key::F3  => 0xffc0,
        Key::F4  => 0xffc1, Key::F5  => 0xffc2, Key::F6  => 0xffc3,
        Key::F7  => 0xffc4, Key::F8  => 0xffc5, Key::F9  => 0xffc6,
        Key::F10 => 0xffc7, Key::F11 => 0xffc8, Key::F12 => 0xffc9,
        // Navigation
        Key::ArrowLeft  => 0xff51, Key::ArrowUp    => 0xff52,
        Key::ArrowRight => 0xff53, Key::ArrowDown  => 0xff54,
        Key::Home  => 0xff50, Key::End   => 0xff57,
        Key::PageUp => 0xff55, Key::PageDown => 0xff56,
        Key::Insert => 0xff63, Key::Delete  => 0xffff,
        // Editing
        Key::Backspace => 0xff08,
        Key::Enter     => 0xff0d,
        Key::Escape    => 0xff1b,
        Key::Tab       => 0xff09,
        Key::Space     => 0x0020,
        // Modifiers
        Key::Minus         => 0x002d,
        Key::PlusEquals    => 0x003d,
        Key::OpenBracket   => 0x005b,
        Key::CloseBracket  => 0x005d,
        Key::Backslash     => 0x005c,
        Key::Semicolon     => 0x003b,
        Key::Comma         => 0x002c,
        Key::Period        => 0x002e,
        Key::Slash         => 0x002f,
        _ => 0,
    }
}

/// Return the printable text for a key if it produces a single character.
fn key_to_text(key: Key) -> Option<String> {
    match key {
        Key::Space => Some(" ".into()),
        Key::Enter => Some("\n".into()),
        Key::Tab   => Some("\t".into()),
        _ => None, // egui emits Text events for printable chars
    }
}

// ── Re-exports ─────────────────────────────────────────────────────────────────

pub use duallink_core::{InputEvent, MouseButton as DlMouseButton, GesturePhase as DlGesturePhase};

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect};

    fn full_rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, egui::vec2(1920.0, 1080.0))
    }

    #[test]
    fn mouse_move_normalisation() {
        let mut bridge = EguiInputBridge::new();
        let events = vec![Event::PointerMoved(Pos2::new(960.0, 540.0))];
        let out = bridge.convert(&events, full_rect());
        assert_eq!(out.len(), 1);
        match out[0] {
            InputEvent::MouseMove { x, y } => {
                assert!((x - 0.5).abs() < 1e-4, "x={}", x);
                assert!((y - 0.5).abs() < 1e-4, "y={}", y);
            }
            _ => panic!("expected MouseMove"),
        }
    }

    #[test]
    fn pointer_button_pressed() {
        let mut bridge = EguiInputBridge::new();
        let events = vec![Event::PointerButton {
            pos: Pos2::new(192.0, 108.0),
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }];
        let out = bridge.convert(&events, full_rect());
        assert_eq!(out.len(), 1);
        match &out[0] {
            InputEvent::MouseDown { x, y, button } => {
                assert!((x - 0.1).abs() < 1e-4);
                assert!((y - 0.1).abs() < 1e-4);
                assert_eq!(*button, MouseButton::Left);
            }
            _ => panic!("expected MouseDown"),
        }
    }

    #[test]
    fn key_mapping_roundtrip() {
        assert_eq!(key_to_x11_keyval(Key::A), 0x0061);
        assert_eq!(key_to_x11_keyval(Key::Enter), 0xff0d);
        assert_eq!(key_to_x11_keyval(Key::Escape), 0xff1b);
        assert_eq!(key_to_x11_keyval(Key::F5), 0xffc2);
        assert_eq!(key_to_x11_keyval(Key::ArrowLeft), 0xff51);
    }
}
