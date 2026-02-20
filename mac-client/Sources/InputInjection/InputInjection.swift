import Foundation
import CoreGraphics
import DualLinkCore

// MARK: - InputInjectionManager (Sprint 2.3)
//
// Receives InputEvent from the Linux receiver and injects them as
// CGEvent on macOS, targeting the virtual display.
//
// Requires Accessibility permission (System Preferences → Privacy → Accessibility).

public final class InputInjectionManager: @unchecked Sendable {

    /// The CGDirectDisplayID of the virtual display to target.
    /// Mouse coordinates are mapped to this display's bounds.
    private var targetDisplayID: CGDirectDisplayID?
    private var displayBounds: CGRect = .zero
    private var eventsInjected: UInt64 = 0

    public init() {}

    // MARK: - Configure

    /// Set the target display for coordinate mapping.
    public func configure(displayID: CGDirectDisplayID) {
        self.targetDisplayID = displayID
        self.displayBounds = CGDisplayBounds(displayID)
        print("[InputInjection] Targeting display \(displayID) bounds=\(displayBounds)")
    }

    // MARK: - Inject

    /// Inject an InputEvent as a macOS CGEvent.
    public func inject(event: InputEvent) {
        switch event {
        case .mouseMove(let x, let y):
            injectMouseMove(x: x, y: y)
        case .mouseDown(let x, let y, let button):
            injectMouseButton(x: x, y: y, button: button, isDown: true)
        case .mouseUp(let x, let y, let button):
            injectMouseButton(x: x, y: y, button: button, isDown: false)
        case .mouseScroll(_, _, let deltaX, let deltaY):
            injectScroll(deltaX: deltaX, deltaY: deltaY)
        case .keyDown(let keycode, _):
            injectKey(keycode: keycode, isDown: true)
        case .keyUp(let keycode):
            injectKey(keycode: keycode, isDown: false)
        }

        eventsInjected += 1
        if eventsInjected == 1 {
            print("[InputInjection] First input event injected!")
        }
    }

    // MARK: - Private: Mouse

    private func injectMouseMove(x: Double, y: Double) {
        let point = mapToDisplay(x: x, y: y)
        guard let event = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved,
                                   mouseCursorPosition: point, mouseButton: .left) else { return }
        event.post(tap: .cghidEventTap)
    }

    private func injectMouseButton(x: Double, y: Double, button: MouseButton, isDown: Bool) {
        let point = mapToDisplay(x: x, y: y)
        let (eventType, cgButton) = mouseEventParams(button: button, isDown: isDown)
        guard let event = CGEvent(mouseEventSource: nil, mouseType: eventType,
                                   mouseCursorPosition: point, mouseButton: cgButton) else { return }
        event.post(tap: .cghidEventTap)
    }

    private func injectScroll(deltaX: Double, deltaY: Double) {
        // CGEvent scroll uses discrete units
        guard let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel,
                                   wheelCount: 2,
                                   wheel1: Int32(deltaY), wheel2: Int32(deltaX)) else { return }
        event.post(tap: .cghidEventTap)
    }

    // MARK: - Private: Keyboard

    private func injectKey(keycode: UInt32, isDown: Bool) {
        let macKeycode = x11KeyvalToMacKeycode(keycode)
        guard let event = CGEvent(keyboardEventSource: nil,
                                   virtualKey: CGKeyCode(macKeycode),
                                   keyDown: isDown) else { return }
        event.post(tap: .cghidEventTap)
    }

    // MARK: - Private: Coordinate Mapping

    /// Map normalised [0,1] coordinates to absolute display coordinates.
    private func mapToDisplay(x: Double, y: Double) -> CGPoint {
        CGPoint(
            x: displayBounds.origin.x + x * displayBounds.width,
            y: displayBounds.origin.y + y * displayBounds.height
        )
    }

    // MARK: - Private: Button Mapping

    private func mouseEventParams(button: MouseButton, isDown: Bool) -> (CGEventType, CGMouseButton) {
        switch button {
        case .left:
            return (isDown ? .leftMouseDown : .leftMouseUp, .left)
        case .right:
            return (isDown ? .rightMouseDown : .rightMouseUp, .right)
        case .middle:
            return (isDown ? .otherMouseDown : .otherMouseUp, .center)
        }
    }

    // MARK: - Private: X11 Keyval → Mac Keycode

    /// Map X11 keyval to macOS virtual keycode.
    /// Only the most common keys are mapped; extend as needed.
    private func x11KeyvalToMacKeycode(_ keyval: UInt32) -> UInt16 {
        switch keyval {
        // Letters (X11 lowercase ASCII → Mac keycodes)
        case 0x61: return 0x00 // a
        case 0x73: return 0x01 // s
        case 0x64: return 0x02 // d
        case 0x66: return 0x03 // f
        case 0x68: return 0x04 // h
        case 0x67: return 0x05 // g
        case 0x7A: return 0x06 // z
        case 0x78: return 0x07 // x
        case 0x63: return 0x08 // c
        case 0x76: return 0x09 // v
        case 0x62: return 0x0B // b
        case 0x71: return 0x0C // q
        case 0x77: return 0x0D // w
        case 0x65: return 0x0E // e
        case 0x72: return 0x0F // r
        case 0x79: return 0x10 // y
        case 0x74: return 0x11 // t
        case 0x31: return 0x12 // 1
        case 0x32: return 0x13 // 2
        case 0x33: return 0x14 // 3
        case 0x34: return 0x15 // 4
        case 0x36: return 0x16 // 6
        case 0x35: return 0x17 // 5
        case 0x3D: return 0x18 // =
        case 0x39: return 0x19 // 9
        case 0x37: return 0x1A // 7
        case 0x2D: return 0x1B // -
        case 0x38: return 0x1C // 8
        case 0x30: return 0x1D // 0
        case 0x5D: return 0x1E // ]
        case 0x6F: return 0x1F // o
        case 0x75: return 0x20 // u
        case 0x5B: return 0x21 // [
        case 0x69: return 0x22 // i
        case 0x70: return 0x23 // p
        case 0x6C: return 0x25 // l
        case 0x6A: return 0x26 // j
        case 0x27: return 0x27 // '
        case 0x6B: return 0x28 // k
        case 0x3B: return 0x29 // ;
        case 0x5C: return 0x2A // backslash
        case 0x2C: return 0x2B // ,
        case 0x2F: return 0x2C // /
        case 0x6E: return 0x2D // n
        case 0x6D: return 0x2E // m
        case 0x2E: return 0x2F // .
        case 0x60: return 0x32 // `
        // Special keys (X11 keyval)
        case 0xff0d: return 0x24 // Return
        case 0xff09: return 0x30 // Tab
        case 0x0020: return 0x31 // Space
        case 0xff08: return 0x33 // Backspace
        case 0xff1b: return 0x35 // Escape
        case 0xffeb, 0xffec: return 0x37 // Super → Command
        case 0xffe1, 0xffe2: return 0x38 // Shift
        case 0xffe5: return 0x39 // Caps Lock
        case 0xffe9, 0xffea: return 0x3A // Alt → Option
        case 0xffe3, 0xffe4: return 0x3B // Control
        case 0xffff: return 0x75 // Delete (forward)
        // Arrow keys
        case 0xff51: return 0x7B // Left
        case 0xff53: return 0x7C // Right
        case 0xff54: return 0x7D // Down
        case 0xff52: return 0x7E // Up
        // Function keys
        case 0xffbe: return 0x7A // F1
        case 0xffbf: return 0x78 // F2
        case 0xffc0: return 0x63 // F3
        case 0xffc1: return 0x76 // F4
        case 0xffc2: return 0x60 // F5
        case 0xffc3: return 0x61 // F6
        case 0xffc4: return 0x62 // F7
        case 0xffc5: return 0x64 // F8
        case 0xffc6: return 0x65 // F9
        case 0xffc7: return 0x6D // F10
        case 0xffc8: return 0x67 // F11
        case 0xffc9: return 0x6F // F12
        // Navigation
        case 0xff50: return 0x73 // Home
        case 0xff57: return 0x77 // End
        case 0xff55: return 0x74 // Page Up
        case 0xff56: return 0x79 // Page Down
        default: return 0x00 // fallback to 'a'
        }
    }
}
