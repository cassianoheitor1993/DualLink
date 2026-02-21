//! Windows input injection via `SendInput`.
//!
//! Translates `InputEvent` values received from the Linux display window into
//! Win32 input events injected into the local Windows session.
//!
//! Mouse coordinates arrive as normalised [0.0, 1.0] floats and are converted
//! to the absolute MOUSEEVENTF_ABSOLUTE range [0, 65535].
//!
//! Keyboard keycodes arrive as X11 keysyms; `x11_keysym_to_vk` maps them to
//! Windows Virtual-Key codes.

use duallink_core::{InputEvent, MouseButton};
use tracing::warn;

#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, MOUSEINPUT, KEYBDINPUT,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_WHEEL,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    INPUT_MOUSE, INPUT_KEYBOARD,
    VIRTUAL_KEY,
};

/// Inject an InputEvent received from the Linux receiver into the local
/// Windows session using `SendInput`.
///
/// No-op on non-Windows platforms (only compiled on Windows).
pub fn inject_input_event(ev: &InputEvent) {
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = inject_win32(ev) {
            warn!("SendInput failed: {e:#}");
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = ev;
    }
}

// ── Windows-only implementation ───────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn inject_win32(ev: &InputEvent) -> windows::core::Result<()> {
    match ev {
        InputEvent::MouseMove { x, y } => {
            let input = mouse_input(
                norm_to_abs(*x),
                norm_to_abs(*y),
                (MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE).0,
                0,
            );
            send_inputs(&[input])?;
        }

        InputEvent::MouseDown { x, y, button } => {
            let flags = match button {
                MouseButton::Left   => MOUSEEVENTF_LEFTDOWN.0,
                MouseButton::Right  => MOUSEEVENTF_RIGHTDOWN.0,
                MouseButton::Middle => MOUSEEVENTF_MIDDLEDOWN.0,
            };
            let input = mouse_input(
                norm_to_abs(*x),
                norm_to_abs(*y),
                (MOUSEEVENTF_ABSOLUTE).0 | flags,
                0,
            );
            send_inputs(&[input])?;
        }

        InputEvent::MouseUp { x, y, button } => {
            let flags = match button {
                MouseButton::Left   => MOUSEEVENTF_LEFTUP.0,
                MouseButton::Right  => MOUSEEVENTF_RIGHTUP.0,
                MouseButton::Middle => MOUSEEVENTF_MIDDLEUP.0,
            };
            let input = mouse_input(
                norm_to_abs(*x),
                norm_to_abs(*y),
                (MOUSEEVENTF_ABSOLUTE).0 | flags,
                0,
            );
            send_inputs(&[input])?;
        }

        InputEvent::MouseScroll { delta_y, .. } => {
            // WHEEL data: 120 units = one standard notch; positive = scroll up
            let wheel_delta = (-delta_y * 120.0) as i32;
            let input = mouse_input(0, 0, MOUSEEVENTF_WHEEL.0, wheel_delta as u32);
            send_inputs(&[input])?;
        }

        InputEvent::KeyDown { keycode, text } => {
            if let Some(vk) = x11_keysym_to_vk(*keycode) {
                let input = key_input(vk, 0);
                send_inputs(&[input])?;
            } else if let Some(ch) = text.as_deref().and_then(|s| s.chars().next()) {
                // Fall back to Unicode key event for characters without a VK mapping
                let input = unicode_input(ch as u16, false);
                send_inputs(&[input])?;
            }
        }

        InputEvent::KeyUp { keycode } => {
            if let Some(vk) = x11_keysym_to_vk(*keycode) {
                let input = key_input(vk, KEYEVENTF_KEYUP.0);
                send_inputs(&[input])?;
            }
        }

        // Gesture and smooth-scroll events — no direct Win32 equivalent; ignore
        InputEvent::GesturePinch { .. }
        | InputEvent::GestureRotation { .. }
        | InputEvent::GestureSwipe { .. }
        | InputEvent::ScrollSmooth { .. } => {}
    }
    Ok(())
}

// ── Input struct builders ─────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn mouse_input(dx: i32, dy: i32, flags: u32, data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(flags),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn key_input(vk: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(flags),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn unicode_input(ch: u16, key_up: bool) -> INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS;
    let mut flags = KEYEVENTF_UNICODE.0;
    if key_up { flags |= KEYEVENTF_KEYUP.0; }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: ch,
                dwFlags: KEYBD_EVENT_FLAGS(flags),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn send_inputs(inputs: &[INPUT]) -> windows::core::Result<()> {
    let sent = unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32)
    };
    if sent != inputs.len() as u32 {
        // GetLastError is set by Windows
        Err(windows::core::Error::from_win32())
    } else {
        Ok(())
    }
}

// ── Coordinate helpers ────────────────────────────────────────────────────────

/// Convert normalised [0.0, 1.0] to MOUSEEVENTF_ABSOLUTE range [0, 65535].
#[cfg(target_os = "windows")]
fn norm_to_abs(v: f64) -> i32 {
    (v.clamp(0.0, 1.0) * 65535.0) as i32
}

// ── X11 keysym → Windows Virtual-Key mapping ─────────────────────────────────
//
// X11 keysyms for printable ASCII match the Unicode codepoint (0x20–0x7E).
// Virtual-Key codes for A–Z and 0–9 also match uppercase ASCII.
// Function and special keys need an explicit lookup.

fn x11_keysym_to_vk(keysym: u32) -> Option<u16> {
    // ── Printable ASCII: letters (a-z / A-Z → VK_A–VK_Z = 0x41–0x5A) ────────
    let keysym = if (0x61..=0x7a).contains(&keysym) {
        keysym - 0x20 // lowercase → uppercase (=VK code)
    } else {
        keysym
    };

    // Digits 0–9: keysym == ASCII == VK
    if (0x30..=0x39).contains(&keysym) {
        return Some(keysym as u16);
    }

    // Uppercase letters A–Z: keysym == VK
    if (0x41..=0x5a).contains(&keysym) {
        return Some(keysym as u16);
    }

    // ── Special / function keys ────────────────────────────────────────────
    // Reference: /usr/include/X11/keysymdef.h and Windows VK_ constants
    let vk: u16 = match keysym {
        0x0020 => 0x20,       // VK_SPACE
        0xff08 => 0x08,       // VK_BACK       (BackSpace)
        0xff09 => 0x09,       // VK_TAB
        0xff0d => 0x0D,       // VK_RETURN     (Return / KP_Enter)
        0xff1b => 0x1B,       // VK_ESCAPE
        0xffff => 0x2E,       // VK_DELETE
        0xff50 => 0x24,       // VK_HOME
        0xff51 => 0x25,       // VK_LEFT
        0xff52 => 0x26,       // VK_UP
        0xff53 => 0x27,       // VK_RIGHT
        0xff54 => 0x28,       // VK_DOWN
        0xff55 => 0x21,       // VK_PRIOR      (Page Up)
        0xff56 => 0x22,       // VK_NEXT       (Page Down)
        0xff57 => 0x23,       // VK_END
        0xff63 => 0x2D,       // VK_INSERT
        0xffe1 | 0xffe2 => 0x10, // VK_SHIFT  (Shift_L / Shift_R)
        0xffe3 | 0xffe4 => 0x11, // VK_CONTROL (Control_L / Control_R)
        0xffe9 | 0xffea => 0x12, // VK_MENU   (Alt_L / Alt_R)
        0xffeb | 0xffec => 0x5B, // VK_LWIN   (Super_L / Super_R)
        0xff7f => 0x90,       // VK_NUMLOCK
        0xff14 => 0x91,       // VK_SCROLL
        0xffbe => 0x70,       // VK_F1
        0xffbf => 0x71,       // VK_F2
        0xffc0 => 0x72,       // VK_F3
        0xffc1 => 0x73,       // VK_F4
        0xffc2 => 0x74,       // VK_F5
        0xffc3 => 0x75,       // VK_F6
        0xffc4 => 0x76,       // VK_F7
        0xffc5 => 0x77,       // VK_F8
        0xffc6 => 0x78,       // VK_F9
        0xffc7 => 0x79,       // VK_F10
        0xffc8 => 0x7A,       // VK_F11
        0xffc9 => 0x7B,       // VK_F12
        // OEM keys (common keyboard punctuation)
        0x003b | 0x003B => 0xBA, // VK_OEM_1    ; :
        0x003d | 0x002b => 0xBB, // VK_OEM_PLUS = +
        0x002c => 0xBC,       // VK_OEM_COMMA  ,
        0x002d => 0xBD,       // VK_OEM_MINUS  -
        0x002e => 0xBE,       // VK_OEM_PERIOD .
        0x002f => 0xBF,       // VK_OEM_2      / ?
        0x0060 => 0xC0,       // VK_OEM_3      ` ~
        0x005b => 0xDB,       // VK_OEM_4      [ {
        0x005c => 0xDC,       // VK_OEM_5      \ |
        0x005d => 0xDD,       // VK_OEM_6      ] }
        0x0027 => 0xDE,       // VK_OEM_7      ' "
        _      => return None,
    };
    Some(vk)
}
