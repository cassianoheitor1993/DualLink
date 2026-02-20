import Foundation
import CoreGraphics

// MARK: - Resolution

/// Resolução de display suportada pelo DualLink.
public struct Resolution: Equatable, Codable, Sendable {
    public let width: Int
    public let height: Int

    public init(width: Int, height: Int) {
        self.width = width
        self.height = height
    }

    public static let fhd  = Resolution(width: 1920, height: 1080)  // 1080p
    public static let qhd  = Resolution(width: 2560, height: 1440)  // 1440p
    public static let uhd  = Resolution(width: 3840, height: 2160)  // 4K

    public var aspectRatio: Double { Double(width) / Double(height) }
    public var cgSize: CGSize { CGSize(width: width, height: height) }
}

// MARK: - ConnectionMode

/// Modo de transporte da conexão.
public enum ConnectionMode: String, Codable, Sendable {
    case wifi
    case usb
}

// MARK: - StreamConfig

/// Configuração de stream de vídeo.
public struct StreamConfig: Equatable, Codable, Sendable {
    public var resolution: Resolution
    public var targetFPS: Int
    public var maxBitrateBps: Int
    public var codec: VideoCodec
    public var lowLatencyMode: Bool

    public init(
        resolution: Resolution = .fhd,
        targetFPS: Int = 30,
        maxBitrateBps: Int = 8_000_000,
        codec: VideoCodec = .h264,
        lowLatencyMode: Bool = true
    ) {
        self.resolution = resolution
        self.targetFPS = targetFPS
        self.maxBitrateBps = maxBitrateBps
        self.codec = codec
        self.lowLatencyMode = lowLatencyMode
    }

    public static let `default` = StreamConfig()

    public static let highPerformance = StreamConfig(
        resolution: .fhd,
        targetFPS: 60,
        maxBitrateBps: 15_000_000,
        codec: .h264,
        lowLatencyMode: true
    )
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
