//! `input_inject` — forward `InputEvent`s from the receiver into the local
//! Linux desktop via `/dev/uinput` (evdev).
//!
//! # Requirements
//!
//! - The process must have write access to `/dev/uinput`.
//!   Either run as root or add the user to the `input` group:
//!   ```
//!   sudo usermod -aG input $USER
//!   sudo chmod 0660 /dev/uinput
//!   ```
//! - Kernel module must be loaded: `sudo modprobe uinput`
//!
//! # Devices created
//!
//! The injector creates two `uinput` virtual devices at startup:
//! - **DualLink Mouse** — relative axes, BTN_LEFT/RIGHT/MIDDLE, scroll wheel
//! - **DualLink Keyboard** — full 104-key layout
//!
//! # Coordinate mapping
//!
//! `MouseMove` events carry normalised [0.0, 1.0] coordinates from the
//! macOS sender. We convert to relative motion by tracking the previous
//! position and emitting `REL_X` / `REL_Y` deltas.
//!
//! For absolute positioning a separate `DualLink Tablet` device emitting
//! `ABS_X` / `ABS_Y` events can be added in a future phase.

#![cfg_attr(not(target_os = "linux"), allow(dead_code, unused_imports))]

use duallink_core::input::{InputEvent, MouseButton};
use tracing::{debug, warn};

// ── Global lazy injector ──────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
static INJECTOR: std::sync::OnceLock<std::sync::Mutex<Option<Injector>>> =
    std::sync::OnceLock::new();

/// Initialise the global uinput injector.  Call once at startup.
///
/// If `/dev/uinput` is not accessible, logs a warning and injects nothing.
#[cfg(target_os = "linux")]
pub fn init() {
    let injector = match Injector::new() {
        Ok(i) => {
            tracing::info!("uinput injector ready (DualLink Mouse + DualLink Keyboard)");
            Some(i)
        }
        Err(e) => {
            warn!(
                "uinput init failed — input injection disabled ({e}). \
                 Try: sudo modprobe uinput && sudo chmod 0660 /dev/uinput"
            );
            None
        }
    };
    let _ = INJECTOR.set(std::sync::Mutex::new(injector));
}

/// Inject an `InputEvent` into the local desktop via uinput.
#[cfg(target_os = "linux")]
pub async fn inject_global(event: duallink_core::InputEvent) {
    if let Some(lock) = INJECTOR.get() {
        if let Ok(mut guard) = lock.lock() {
            if let Some(inj) = guard.as_mut() {
                if let Err(e) = inj.inject(event) {
                    debug!("uinput inject error: {e}");
                }
            }
        }
    }
}

/// No-op stub on non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub fn init() {}

/// No-op stub on non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub async fn inject_global(_event: duallink_core::InputEvent) {}

// ── Linux implementation ──────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;
    use evdev::{
        uinput::{VirtualDevice, VirtualDeviceBuilder},
        AttributeSet, EventType, InputId, Key, RelativeAxisType,
    };

    // Maximum screen dimensions for normalised → pixel conversion.
    // TODO: query actual display resolution.
    const MAX_SCREEN_W: f64 = 1920.0;
    const MAX_SCREEN_H: f64 = 1080.0;

    pub(super) struct Injector {
        mouse:   VirtualDevice,
        keyboard: VirtualDevice,
        last_x:  f64,
        last_y:  f64,
    }

    impl Injector {
        pub(super) fn new() -> anyhow::Result<Self> {
            // ── Virtual mouse ─────────────────────────────────────────────
            let mut mouse_keys = AttributeSet::<Key>::new();
            mouse_keys.insert(Key::BTN_LEFT);
            mouse_keys.insert(Key::BTN_RIGHT);
            mouse_keys.insert(Key::BTN_MIDDLE);

            let mut rel_axes = AttributeSet::<RelativeAxisType>::new();
            rel_axes.insert(RelativeAxisType::REL_X);
            rel_axes.insert(RelativeAxisType::REL_Y);
            rel_axes.insert(RelativeAxisType::REL_WHEEL);
            rel_axes.insert(RelativeAxisType::REL_WHEEL_HI_RES);
            rel_axes.insert(RelativeAxisType::REL_HWHEEL);
            rel_axes.insert(RelativeAxisType::REL_HWHEEL_HI_RES);

            let mouse = VirtualDeviceBuilder::new()?
                .name("DualLink Mouse")
                .with_keys(&mouse_keys)?
                .with_relative_axes(&rel_axes)?
                .build()?;

            // ── Virtual keyboard ──────────────────────────────────────────
            let mut key_set = AttributeSet::<Key>::new();
            // Insert a broad range of common keys
            for code in 1u16..=248 {
                if let Ok(k) = Key::new(code).try_into() {
                    // evdev::Key::new just wraps a u16 — no TryInto needed
                    let _ = k; // suppress
                }
                key_set.insert(Key::new(code));
            }

            let keyboard = VirtualDeviceBuilder::new()?
                .name("DualLink Keyboard")
                .with_keys(&key_set)?
                .build()?;

            Ok(Self { mouse, keyboard, last_x: 0.5, last_y: 0.5 })
        }

        pub(super) fn inject(&mut self, event: duallink_core::InputEvent) -> anyhow::Result<()> {
            use duallink_core::input::GesturePhase;
            use evdev::{AbsoluteAxisType, EventType};

            match event {
                InputEvent::MouseMove { x, y } => {
                    let dx = ((x - self.last_x) * MAX_SCREEN_W) as i32;
                    let dy = ((y - self.last_y) * MAX_SCREEN_H) as i32;
                    self.last_x = x;
                    self.last_y = y;
                    if dx != 0 || dy != 0 {
                        let events = [
                            evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_X.0, dx),
                            evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_Y.0, dy),
                            evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                        ];
                        self.mouse.emit(&events)?;
                    }
                }

                InputEvent::MouseDown { x, y, button } => {
                    self.update_pos(x, y);
                    let btn = mouse_button_to_key(button);
                    let events = [
                        evdev::InputEvent::new(EventType::KEY, btn.code(), 1),
                        evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                    ];
                    self.mouse.emit(&events)?;
                }

                InputEvent::MouseUp { x, y, button } => {
                    self.update_pos(x, y);
                    let btn = mouse_button_to_key(button);
                    let events = [
                        evdev::InputEvent::new(EventType::KEY, btn.code(), 0),
                        evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                    ];
                    self.mouse.emit(&events)?;
                }

                InputEvent::MouseScroll { delta_x, delta_y, .. } => {
                    // Vertical scroll
                    if delta_y.abs() > 0.01 {
                        let ticks = (delta_y * 3.0) as i32;
                        let hi_res = (delta_y * 120.0 * 3.0) as i32;
                        let events = [
                            evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_WHEEL.0, -ticks),
                            evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_WHEEL_HI_RES.0, -hi_res),
                            evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                        ];
                        self.mouse.emit(&events)?;
                    }
                    // Horizontal scroll
                    if delta_x.abs() > 0.01 {
                        let ticks = (delta_x * 3.0) as i32;
                        let hi_res = (delta_x * 120.0 * 3.0) as i32;
                        let events = [
                            evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_HWHEEL.0, ticks),
                            evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_HWHEEL_HI_RES.0, hi_res),
                            evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                        ];
                        self.mouse.emit(&events)?;
                    }
                }

                InputEvent::KeyDown { keycode, .. } => {
                    let key = keycode_to_evdev(keycode);
                    let events = [
                        evdev::InputEvent::new(EventType::KEY, key, 1),
                        evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                    ];
                    self.keyboard.emit(&events)?;
                }

                InputEvent::KeyUp { keycode } => {
                    let key = keycode_to_evdev(keycode);
                    let events = [
                        evdev::InputEvent::new(EventType::KEY, key, 0),
                        evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                    ];
                    self.keyboard.emit(&events)?;
                }

                // Gestures — map pinch to Ctrl+scroll (universal zoom)
                InputEvent::GesturePinch { magnification, .. } => {
                    let ctrl_down = [
                        evdev::InputEvent::new(EventType::KEY, Key::KEY_LEFTCTRL.code(), 1),
                        evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                    ];
                    let scroll = [
                        evdev::InputEvent::new(
                            EventType::RELATIVE,
                            RelativeAxisType::REL_WHEEL.0,
                            if magnification > 0.0 { 1 } else { -1 },
                        ),
                        evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                    ];
                    let ctrl_up = [
                        evdev::InputEvent::new(EventType::KEY, Key::KEY_LEFTCTRL.code(), 0),
                        evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                    ];
                    self.keyboard.emit(&ctrl_down)?;
                    self.mouse.emit(&scroll)?;
                    self.keyboard.emit(&ctrl_up)?;
                }

                // Smooth scroll: forward as high-res scroll
                InputEvent::ScrollSmooth { delta_x, delta_y, .. } => {
                    if delta_y.abs() > 0.1 {
                        let hi_res = (delta_y * 120.0) as i32;
                        let events = [
                            evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_WHEEL_HI_RES.0, -hi_res),
                            evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                        ];
                        self.mouse.emit(&events)?;
                    }
                    if delta_x.abs() > 0.1 {
                        let hi_res = (delta_x * 120.0) as i32;
                        let events = [
                            evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_HWHEEL_HI_RES.0, hi_res),
                            evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                        ];
                        self.mouse.emit(&events)?;
                    }
                }

                // Rotation: map to left/right arrow keys (common for presentation next/prev)
                InputEvent::GestureRotation { rotation, .. } => {
                    if rotation.abs() > 15.0 {
                        let key = if rotation > 0.0 { Key::KEY_RIGHT } else { Key::KEY_LEFT };
                        let events = [
                            evdev::InputEvent::new(EventType::KEY, key.code(), 1),
                            evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                            evdev::InputEvent::new(EventType::KEY, key.code(), 0),
                            evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                        ];
                        self.keyboard.emit(&events)?;
                    }
                }

                InputEvent::GestureSwipe { delta_x, delta_y, .. } => {
                    // 3-finger swipe: map to desktop switching shortcuts
                    if delta_x.abs() > delta_y.abs() {
                        let key = if delta_x > 0.0 { Key::KEY_RIGHT } else { Key::KEY_LEFT };
                        // Ctrl+Alt+Arrow (common virtual desktop switch)
                        let events = [
                            evdev::InputEvent::new(EventType::KEY, Key::KEY_LEFTCTRL.code(), 1),
                            evdev::InputEvent::new(EventType::KEY, Key::KEY_LEFTALT.code(), 1),
                            evdev::InputEvent::new(EventType::KEY, key.code(), 1),
                            evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                            evdev::InputEvent::new(EventType::KEY, key.code(), 0),
                            evdev::InputEvent::new(EventType::KEY, Key::KEY_LEFTALT.code(), 0),
                            evdev::InputEvent::new(EventType::KEY, Key::KEY_LEFTCTRL.code(), 0),
                            evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                        ];
                        self.keyboard.emit(&events)?;
                    }
                }
            }
            Ok(())
        }

        fn update_pos(&mut self, x: f64, y: f64) {
            let dx = ((x - self.last_x) * MAX_SCREEN_W) as i32;
            let dy = ((y - self.last_y) * MAX_SCREEN_H) as i32;
            if dx != 0 || dy != 0 {
                let events = [
                    evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_X.0, dx),
                    evdev::InputEvent::new(EventType::RELATIVE, RelativeAxisType::REL_Y.0, dy),
                    evdev::InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
                ];
                let _ = self.mouse.emit(&events);
            }
            self.last_x = x;
            self.last_y = y;
        }
    }

    // ── Key mapping helpers ───────────────────────────────────────────────────

    fn mouse_button_to_key(btn: duallink_core::input::MouseButton) -> Key {
        match btn {
            MouseButton::Left   => Key::BTN_LEFT,
            MouseButton::Right  => Key::BTN_RIGHT,
            MouseButton::Middle => Key::BTN_MIDDLE,
        }
    }

    /// Map X11 keysyms (sent from macOS) → Linux evdev key codes.
    ///
    /// X11 keysyms and Linux evdev codes differ. This table covers the most
    /// frequently used keys. Unknown keysyms are silently ignored.
    fn keycode_to_evdev(xkeysym: u32) -> u16 {
        // X11 keysyms are defined in <X11/keysymdef.h>
        match xkeysym {
            // ASCII printable range — map directly via X11 keysym offset
            0x0020 => Key::KEY_SPACE.code(),
            0x0027 => Key::KEY_APOSTROPHE.code(),
            0x002c => Key::KEY_COMMA.code(),
            0x002d => Key::KEY_MINUS.code(),
            0x002e => Key::KEY_DOT.code(),
            0x002f => Key::KEY_SLASH.code(),
            0x0030..=0x0039 => Key::KEY_0.code() + (xkeysym - 0x0030) as u16,
            0x003b => Key::KEY_SEMICOLON.code(),
            0x003d => Key::KEY_EQUAL.code(),
            0x005b => Key::KEY_LEFTBRACE.code(),
            0x005c => Key::KEY_BACKSLASH.code(),
            0x005d => Key::KEY_RIGHTBRACE.code(),
            0x0060 => Key::KEY_GRAVE.code(),
            // a-z
            0x0061..=0x007a => Key::KEY_A.code() + (xkeysym - 0x0061) as u16,
            // Function keys (XK_F1 = 0xffbe)
            0xffbe..=0xffc9 => Key::KEY_F1.code() + (xkeysym - 0xffbe) as u16,
            // Special keys
            0xff08 => Key::KEY_BACKSPACE.code(),
            0xff09 => Key::KEY_TAB.code(),
            0xff0d => Key::KEY_ENTER.code(),
            0xff1b => Key::KEY_ESC.code(),
            0xff51 => Key::KEY_LEFT.code(),
            0xff52 => Key::KEY_UP.code(),
            0xff53 => Key::KEY_RIGHT.code(),
            0xff54 => Key::KEY_DOWN.code(),
            0xff55 => Key::KEY_PAGEUP.code(),
            0xff56 => Key::KEY_PAGEDOWN.code(),
            0xff50 => Key::KEY_HOME.code(),
            0xff57 => Key::KEY_END.code(),
            0xff63 => Key::KEY_INSERT.code(),
            0xffff => Key::KEY_DELETE.code(),
            // Modifiers
            0xffe1 | 0xffe2 => Key::KEY_LEFTSHIFT.code(),
            0xffe3 | 0xffe4 => Key::KEY_LEFTCTRL.code(),
            0xffe5 => Key::KEY_CAPSLOCK.code(),
            0xffe9 | 0xffea => Key::KEY_LEFTALT.code(),
            0xffe7 | 0xffe8 => Key::KEY_LEFTMETA.code(),  // Super/Command
            // Space bar already at 0x0020 above
            _ => {
                debug!("Unknown X11 keysym 0x{:04x} — skipped", xkeysym);
                0 // KEY_RESERVED — no-op
            }
        }
    }

    pub(super) use Injector;
}

#[cfg(target_os = "linux")]
use linux_impl::Injector;
