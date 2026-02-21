import Foundation
import CoreGraphics

// MARK: - Resolution

/// Resolução de display suportada pelo DualLink.
public struct Resolution: Equatable, Hashable, Codable, Sendable {
    public let width: Int
    public let height: Int

    public init(width: Int, height: Int) {
        self.width = width
        self.height = height
    }

    public static let fhd  = Resolution(width: 1920, height: 1080)  // 1080p
    public static let qhd  = Resolution(width: 2560, height: 1440)  // 1440p
    public static let uhd  = Resolution(width: 3840, height: 2160)  // 4K

    /// All supported resolution presets, ordered ascending.
    public static let allPresets: [Resolution] = [.fhd, .qhd, .uhd]

    public var aspectRatio: Double { Double(width) / Double(height) }
    public var cgSize: CGSize { CGSize(width: width, height: height) }

    /// Human-readable label.
    public var label: String {
        switch (width, height) {
        case (1920, 1080): return "1080p"
        case (2560, 1440): return "1440p"
        case (3840, 2160): return "4K"
        default: return "\(width)×\(height)"
        }
    }
}

// MARK: - ConnectionMode

/// Modo de transporte da conexão.
public enum ConnectionMode: String, Codable, Sendable {
    case wifi
    case usb
}

// MARK: - DisplayMode

/// Whether DualLink mirrors the main screen or extends the desktop.
public enum DisplayMode: String, Codable, Sendable {
    /// Mirror: capture the main display and stream it (no virtual display needed).
    case mirror
    /// Extend: create a virtual display and stream it as a second screen.
    case extend
}

// MARK: - StreamConfig

/// Configuração de stream de vídeo.
public struct StreamConfig: Equatable, Codable, Sendable {
    public var resolution: Resolution
    public var targetFPS: Int
    public var maxBitrateBps: Int
    public var codec: VideoCodec
    public var lowLatencyMode: Bool
    /// Zero-based index identifying which display channel this config belongs to.
    /// Display 0 = first (primary) stream; display 1 = second, etc.
    public var displayIndex: UInt8

    public init(
        resolution: Resolution = .fhd,
        targetFPS: Int = 30,
        maxBitrateBps: Int = 8_000_000,
        codec: VideoCodec = .h264,
        lowLatencyMode: Bool = true,
        displayIndex: UInt8 = 0
    ) {
        self.resolution = resolution
        self.targetFPS = targetFPS
        self.maxBitrateBps = maxBitrateBps
        self.codec = codec
        self.lowLatencyMode = lowLatencyMode
        self.displayIndex = displayIndex
    }

    public static let `default` = StreamConfig()

    public static let highPerformance = StreamConfig(
        resolution: .fhd,
        targetFPS: 60,
        maxBitrateBps: 15_000_000,
        codec: .h264,
        lowLatencyMode: true,
        displayIndex: 0
    )

    /// Auto-select a good bitrate for a given resolution + fps combo.
    public static func recommended(resolution: Resolution, fps: Int, codec: VideoCodec = .h264) -> StreamConfig {
        let pixelCount = resolution.width * resolution.height
        let baseBps: Int
        switch pixelCount {
        case ..<(2_100_000):  baseBps = 8_000_000     // 1080p
        case ..<(3_700_000):  baseBps = 15_000_000    // 1440p
        default:              baseBps = 30_000_000    // 4K
        }
        // Scale for 60fps (×1.5) and H.265 (×0.7 efficiency)
        let fpsMultiplier = fps >= 60 ? 1.5 : 1.0
        let codecMultiplier = codec == .h265 ? 0.7 : 1.0
        let bitrate = Int(Double(baseBps) * fpsMultiplier * codecMultiplier)
        return StreamConfig(
            resolution: resolution,
            targetFPS: fps,
            maxBitrateBps: bitrate,
            codec: codec,
            lowLatencyMode: true,
            displayIndex: 0
        )
    }
}

// MARK: - VideoCodec

public enum VideoCodec: String, Codable, Sendable {
    case h264
    case h265
}

// MARK: - ConnectionState

/// Estado da conexão entre mac-client e linux-receiver.
public enum ConnectionState: Equatable, Sendable {
    case idle
    case discovering
    case connecting(peer: PeerInfo, attempt: Int)
    case streaming(session: SessionInfo)
    case reconnecting(peer: PeerInfo, attempt: Int)
    case error(reason: String)

    public var isActive: Bool {
        if case .streaming = self { return true }
        return false
    }
}

// MARK: - PeerInfo

public struct PeerInfo: Equatable, Codable, Sendable {
    public let id: String
    public let name: String
    public let address: String
    public let port: Int

    public init(id: String, name: String, address: String, port: Int) {
        self.id = id
        self.name = name
        self.address = address
        self.port = port
    }
}

// MARK: - SessionInfo

public struct SessionInfo: Equatable, Codable, Sendable {
    public let sessionID: String
    public let peer: PeerInfo
    public let config: StreamConfig
    public let connectionMode: ConnectionMode

    public init(sessionID: String, peer: PeerInfo, config: StreamConfig, connectionMode: ConnectionMode) {
        self.sessionID = sessionID
        self.peer = peer
        self.config = config
        self.connectionMode = connectionMode
    }
}

// MARK: - DualLinkError

/// Errors base do projeto.
public enum DualLinkError: LocalizedError, Sendable {
    case notImplemented(String)
    case configurationInvalid(String)
    case permissionDenied(String)
    case connectionFailed(String)
    case streamError(String)

    public var errorDescription: String? {
        switch self {
        case .notImplemented(let feature):
            return "Not implemented yet: \(feature)"
        case .configurationInvalid(let reason):
            return "Invalid configuration: \(reason)"
        case .permissionDenied(let permission):
            return "Permission denied: \(permission)"
        case .connectionFailed(let reason):
            return "Connection failed: \(reason)"
        case .streamError(let reason):
            return "Stream error: \(reason)"
        }
    }
}

// MARK: - InputEvent (Sprint 2.3)

/// Input event received from the Linux receiver.
/// Coordinates are normalised [0.0, 1.0] relative to the display.
public enum InputEvent: Codable, Sendable {
    case mouseMove(x: Double, y: Double)
    case mouseDown(x: Double, y: Double, button: MouseButton)
    case mouseUp(x: Double, y: Double, button: MouseButton)
    case mouseScroll(x: Double, y: Double, deltaX: Double, deltaY: Double)
    case keyDown(keycode: UInt32, text: String?)
    case keyUp(keycode: UInt32)

    // -- Trackpad Gestures (Sprint 2.3.4) --
    case gesturePinch(x: Double, y: Double, magnification: Double, phase: GesturePhase)
    case gestureRotation(x: Double, y: Double, rotation: Double, phase: GesturePhase)
    case gestureSwipe(deltaX: Double, deltaY: Double, phase: GesturePhase)
    case scrollSmooth(x: Double, y: Double, deltaX: Double, deltaY: Double, phase: GesturePhase)

    // Custom Codable to match Rust's adjacently-tagged serde format
    enum CodingKeys: String, CodingKey {
        case kind, x, y, button
        case deltaX = "delta_x"
        case deltaY = "delta_y"
        case keycode, text
        case magnification, rotation, phase
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "mouse_move":
            self = .mouseMove(
                x: try c.decode(Double.self, forKey: .x),
                y: try c.decode(Double.self, forKey: .y)
            )
        case "mouse_down":
            self = .mouseDown(
                x: try c.decode(Double.self, forKey: .x),
                y: try c.decode(Double.self, forKey: .y),
                button: try c.decode(MouseButton.self, forKey: .button)
            )
        case "mouse_up":
            self = .mouseUp(
                x: try c.decode(Double.self, forKey: .x),
                y: try c.decode(Double.self, forKey: .y),
                button: try c.decode(MouseButton.self, forKey: .button)
            )
        case "mouse_scroll":
            self = .mouseScroll(
                x: try c.decode(Double.self, forKey: .x),
                y: try c.decode(Double.self, forKey: .y),
                deltaX: try c.decode(Double.self, forKey: .deltaX),
                deltaY: try c.decode(Double.self, forKey: .deltaY)
            )
        case "key_down":
            self = .keyDown(
                keycode: try c.decode(UInt32.self, forKey: .keycode),
                text: try c.decodeIfPresent(String.self, forKey: .text)
            )
        case "key_up":
            self = .keyUp(keycode: try c.decode(UInt32.self, forKey: .keycode))
        case "gesture_pinch":
            self = .gesturePinch(
                x: try c.decode(Double.self, forKey: .x),
                y: try c.decode(Double.self, forKey: .y),
                magnification: try c.decode(Double.self, forKey: .magnification),
                phase: try c.decode(GesturePhase.self, forKey: .phase)
            )
        case "gesture_rotation":
            self = .gestureRotation(
                x: try c.decode(Double.self, forKey: .x),
                y: try c.decode(Double.self, forKey: .y),
                rotation: try c.decode(Double.self, forKey: .rotation),
                phase: try c.decode(GesturePhase.self, forKey: .phase)
            )
        case "gesture_swipe":
            self = .gestureSwipe(
                deltaX: try c.decode(Double.self, forKey: .deltaX),
                deltaY: try c.decode(Double.self, forKey: .deltaY),
                phase: try c.decode(GesturePhase.self, forKey: .phase)
            )
        case "scroll_smooth":
            self = .scrollSmooth(
                x: try c.decode(Double.self, forKey: .x),
                y: try c.decode(Double.self, forKey: .y),
                deltaX: try c.decode(Double.self, forKey: .deltaX),
                deltaY: try c.decode(Double.self, forKey: .deltaY),
                phase: try c.decode(GesturePhase.self, forKey: .phase)
            )
        default:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: c, debugDescription: "Unknown input event kind: \(kind)")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .mouseMove(let x, let y):
            try c.encode("mouse_move", forKey: .kind)
            try c.encode(x, forKey: .x)
            try c.encode(y, forKey: .y)
        case .mouseDown(let x, let y, let button):
            try c.encode("mouse_down", forKey: .kind)
            try c.encode(x, forKey: .x)
            try c.encode(y, forKey: .y)
            try c.encode(button, forKey: .button)
        case .mouseUp(let x, let y, let button):
            try c.encode("mouse_up", forKey: .kind)
            try c.encode(x, forKey: .x)
            try c.encode(y, forKey: .y)
            try c.encode(button, forKey: .button)
        case .mouseScroll(let x, let y, let dx, let dy):
            try c.encode("mouse_scroll", forKey: .kind)
            try c.encode(x, forKey: .x)
            try c.encode(y, forKey: .y)
            try c.encode(dx, forKey: .deltaX)
            try c.encode(dy, forKey: .deltaY)
        case .keyDown(let keycode, let text):
            try c.encode("key_down", forKey: .kind)
            try c.encode(keycode, forKey: .keycode)
            try c.encodeIfPresent(text, forKey: .text)
        case .keyUp(let keycode):
            try c.encode("key_up", forKey: .kind)
            try c.encode(keycode, forKey: .keycode)
        case .gesturePinch(let x, let y, let magnification, let phase):
            try c.encode("gesture_pinch", forKey: .kind)
            try c.encode(x, forKey: .x)
            try c.encode(y, forKey: .y)
            try c.encode(magnification, forKey: .magnification)
            try c.encode(phase, forKey: .phase)
        case .gestureRotation(let x, let y, let rotation, let phase):
            try c.encode("gesture_rotation", forKey: .kind)
            try c.encode(x, forKey: .x)
            try c.encode(y, forKey: .y)
            try c.encode(rotation, forKey: .rotation)
            try c.encode(phase, forKey: .phase)
        case .gestureSwipe(let dx, let dy, let phase):
            try c.encode("gesture_swipe", forKey: .kind)
            try c.encode(dx, forKey: .deltaX)
            try c.encode(dy, forKey: .deltaY)
            try c.encode(phase, forKey: .phase)
        case .scrollSmooth(let x, let y, let dx, let dy, let phase):
            try c.encode("scroll_smooth", forKey: .kind)
            try c.encode(x, forKey: .x)
            try c.encode(y, forKey: .y)
            try c.encode(dx, forKey: .deltaX)
            try c.encode(dy, forKey: .deltaY)
            try c.encode(phase, forKey: .phase)
        }
    }
}

/// Phase of a trackpad gesture (maps to macOS NSEvent.Phase).
public enum GesturePhase: String, Codable, Sendable {
    case begin
    case changed
    case end
    case cancelled
}

public enum MouseButton: String, Codable, Sendable {
    case left
    case right
    case middle
}
