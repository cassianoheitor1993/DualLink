//! Input event types — Sprint 2.3
//!
//! Defines mouse and keyboard events captured on Linux and
//! injected on macOS via CGEvent.
//!
//! These are serialised as JSON and sent over the TCP signaling
//! channel as `input_event` messages.

use serde::{Deserialize, Serialize};

// MARK: - InputEvent

/// A user input event captured from the Linux display window.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputEvent {
    /// Mouse moved to (x, y) in normalised coordinates [0.0, 1.0].
    MouseMove {
        x: f64,
        y: f64,
    },

    /// Mouse button pressed.
    MouseDown {
        x: f64,
        y: f64,
        button: MouseButton,
    },

    /// Mouse button released.
    MouseUp {
        x: f64,
        y: f64,
        button: MouseButton,
    },

    /// Mouse scroll (delta in pixels / points).
    MouseScroll {
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
    },

    /// Key pressed.
    KeyDown {
        /// Platform-neutral keycode (X11 keyval).
        keycode: u32,
        /// Optional character string (for text input).
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },

    /// Key released.
    KeyUp {
        keycode: u32,
    },

    // -- Trackpad Gestures (Sprint 2.3.4) --

    /// Pinch-to-zoom / magnification gesture.
    GesturePinch {
        /// Center of gesture in normalised [0.0, 1.0] coordinates.
        x: f64,
        y: f64,
        /// Magnification delta (positive = zoom in, negative = zoom out).
        magnification: f64,
        phase: GesturePhase,
    },

    /// Rotation gesture (two-finger twist).
    GestureRotation {
        /// Center of rotation in normalised [0.0, 1.0] coordinates.
        x: f64,
        y: f64,
        /// Rotation delta in degrees (positive = clockwise).
        rotation: f64,
        phase: GesturePhase,
    },

    /// Multi-finger swipe gesture (three/four fingers).
    GestureSwipe {
        /// Swipe direction vector (normalised).
        delta_x: f64,
        delta_y: f64,
        phase: GesturePhase,
    },

    /// Smooth (continuous) scroll with phase info for momentum scrolling.
    ScrollSmooth {
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        phase: GesturePhase,
    },
}

// MARK: - GesturePhase

/// Phase of a trackpad gesture (matches macOS NSEvent.Phase semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GesturePhase {
    /// Gesture just started.
    Begin,
    /// Gesture is ongoing.
    Changed,
    /// Gesture ended.
    End,
    /// Gesture was cancelled.
    Cancelled,
}

// MARK: - MouseButton

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_event_roundtrip() {
        let events = vec![
            InputEvent::MouseMove { x: 0.5, y: 0.3 },
            InputEvent::MouseDown { x: 0.1, y: 0.9, button: MouseButton::Left },
            InputEvent::MouseUp { x: 0.1, y: 0.9, button: MouseButton::Right },
            InputEvent::MouseScroll { x: 0.5, y: 0.5, delta_x: 0.0, delta_y: -3.0 },
            InputEvent::KeyDown { keycode: 38, text: Some("a".to_string()) },
            InputEvent::KeyUp { keycode: 38 },
            InputEvent::GesturePinch { x: 0.5, y: 0.5, magnification: 0.1, phase: GesturePhase::Changed },
            InputEvent::GestureRotation { x: 0.5, y: 0.5, rotation: 15.0, phase: GesturePhase::Begin },
            InputEvent::GestureSwipe { delta_x: 1.0, delta_y: 0.0, phase: GesturePhase::End },
            InputEvent::ScrollSmooth { x: 0.5, y: 0.5, delta_x: 0.0, delta_y: -2.5, phase: GesturePhase::Changed },
        ];

        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            let parsed: InputEvent = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&parsed).unwrap();
            assert_eq!(json, json2, "roundtrip failed for {:?}", event);
        }
    }
}
