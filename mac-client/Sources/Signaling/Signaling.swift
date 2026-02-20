import Foundation
import Network
import DualLinkCore

// MARK: - Signaling Protocol
//
// Length-prefixed JSON over TCP on port 7879.
// Each message:  [UInt32 big-endian length][JSON-UTF8 payload]
//
// Message types (SignalingMessage.MessageType):
//   hello         → mac → linux   session open + stream config
//   hello_ack     → linux → mac   accepted / rejected
//   config_update → mac → linux   mid-session config change
//   keepalive     → mac → linux   1Hz heartbeat
//   stop          → mac → linux   end session gracefully

let kSignalingPort: UInt16 = 7879

// MARK: - SignalingMessage

/// A JSON message exchanged over the TCP control channel.
public struct SignalingMessage: Codable, Sendable {

    public enum MessageType: String, Codable, Sendable {
        case hello
        case helloAck       = "hello_ack"
        case configUpdate   = "config_update"
        case keepalive
        case stop
        case inputEvent     = "input_event"
    }

    public let type: MessageType
    public let sessionID: String?
    public let deviceName: String?
    public let config: StreamConfig?
    public let accepted: Bool?
    public let reason: String?
    public let timestampMs: UInt64?
    public let inputEvent: InputEvent?

    // MARK: Factories

    public static func hello(sessionID: String, deviceName: String, config: StreamConfig) -> SignalingMessage {
        SignalingMessage(
            type: .hello,
            sessionID: sessionID,
            deviceName: deviceName,
            config: config,
            accepted: nil,
            reason: nil,
            timestampMs: nil,
            inputEvent: nil
        )
    }

    public static func configUpdate(sessionID: String, config: StreamConfig) -> SignalingMessage {
        SignalingMessage(
            type: .configUpdate,
            sessionID: sessionID,
            deviceName: nil,
            config: config,
            accepted: nil,
            reason: nil,
            timestampMs: nil,
            inputEvent: nil
        )
    }

    public static func keepalive(timestampMs: UInt64) -> SignalingMessage {
        SignalingMessage(
            type: .keepalive,
            sessionID: nil,
            deviceName: nil,
            config: nil,
            accepted: nil,
            reason: nil,
            timestampMs: timestampMs,
            inputEvent: nil
        )
    }

    public static func stop(sessionID: String) -> SignalingMessage {
        SignalingMessage(
            type: .stop,
            sessionID: sessionID,
            deviceName: nil,
            config: nil,
            accepted: nil,
            reason: nil,
            timestampMs: nil,
            inputEvent: nil
        )
    }
}

// MARK: - SignalingClientState

public enum SignalingClientState: Equatable, Sendable {
    case idle
    case connecting
    case connected
    case waitingForAck
    case sessionActive
    case failed(String)
}

// MARK: - SignalingClient

/// Manages the TCP control channel between mac-client and linux-receiver.
///
/// ## Lifecycle
/// 1. `connect(host:port:)` — open TCP connection
/// 2. `sendHello(sessionID:config:)` — announce session, wait for ack
/// 3. `sendKeepalive()` — call every 1s from a repeating Task
/// 4. `sendStop(sessionID:)` — graceful shutdown
///
/// ## Thread Safety
/// All methods are `async` and safe to call from any context.
public actor SignalingClient {

    // MARK: - State

    public private(set) var state: SignalingClientState = .idle
    public var onStateChange: (@Sendable (SignalingClientState) -> Void)?

    /// Called when the receiver sends back `hello_ack`.
    public var onHelloAck: (@Sendable (Bool, String?) -> Void)?

    /// Called when any message is received from the receiver.
    public var onMessage: (@Sendable (SignalingMessage) -> Void)?

    /// Called when an input event is received from the Linux receiver.
    public var onInputEvent: (@Sendable (InputEvent) -> Void)?

    // MARK: - Private

    private var connection: NWConnection?
    private var receiveBuffer = Data()
    private let sendQueue = DispatchQueue(label: "com.duallink.signaling", qos: .utility)

    private var keepaliveTask: Task<Void, Never>?

    // MARK: - Init / Deinit

    public init() {}

    // MARK: - Configure Callbacks

    /// Sets callback handlers. Call before `connect`.
    public func configure(
        onStateChange: (@Sendable (SignalingClientState) -> Void)? = nil,
        onHelloAck: (@Sendable (Bool, String?) -> Void)? = nil,
        onMessage: (@Sendable (SignalingMessage) -> Void)? = nil,
        onInputEvent: (@Sendable (InputEvent) -> Void)? = nil
    ) {
        self.onStateChange = onStateChange
        self.onHelloAck = onHelloAck
        self.onMessage = onMessage
        self.onInputEvent = onInputEvent
    }

    // MARK: - Connect

    public func connect(host: String, port: UInt16 = 7879) async throws {
        disconnect()
        setState(.connecting)

        let endpoint = NWEndpoint.hostPort(
            host: NWEndpoint.Host(host),
            port: NWEndpoint.Port(rawValue: port)!
        )
        let params = NWParameters.tcp
        params.allowLocalEndpointReuse = true

        let conn = NWConnection(to: endpoint, using: params)
        connection = conn

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            conn.stateUpdateHandler = { [weak self] newState in
                guard let self else { return }
                Task {
                    switch newState {
                    case .ready:
                        await self.setState(.connected)
                        await self.startReceiving()
                        continuation.resume()
                    case .failed(let error):
                        await self.setState(.failed(error.localizedDescription))
                        continuation.resume(throwing: error)
                    case .cancelled:
                        await self.setState(.idle)
                    default:
                        break
                    }
                }
            }
            conn.start(queue: sendQueue)
        }
    }

    // MARK: - Send Hello

    /// Sends the `hello` handshake to the receiver and starts the keepalive loop.
    public func sendHello(sessionID: String, config: StreamConfig) throws {
        let deviceName = Host.current().localizedName ?? "DualLink Mac"
        let msg = SignalingMessage.hello(sessionID: sessionID, deviceName: deviceName, config: config)
        try sendMessage(msg)
        setState(.waitingForAck)
        startKeepalive(sessionID: sessionID)
    }

    // MARK: - Send Config Update

    public func sendConfigUpdate(sessionID: String, config: StreamConfig) throws {
        let msg = SignalingMessage.configUpdate(sessionID: sessionID, config: config)
        try sendMessage(msg)
    }

    // MARK: - Keepalive

    public func sendKeepalive() throws {
        let ts = UInt64(Date().timeIntervalSince1970 * 1000)
        try sendMessage(.keepalive(timestampMs: ts))
    }

    // MARK: - Stop

    public func sendStop(sessionID: String) throws {
        try sendMessage(.stop(sessionID: sessionID))
    }

    // MARK: - Disconnect

    public func disconnect() {
        keepaliveTask?.cancel()
        keepaliveTask = nil
        connection?.cancel()
        connection = nil
        receiveBuffer.removeAll()
        setState(.idle)
    }

    // MARK: - Private: Send

    private func sendMessage(_ message: SignalingMessage) throws {
        guard state == .connected || state == .waitingForAck || state == .sessionActive else {
            throw SignalingError.notConnected
        }
        guard let connection else { throw SignalingError.notConnected }

        let jsonData = try JSONEncoder().encode(message)
        var length = UInt32(jsonData.count).bigEndian

        var frame = Data(capacity: 4 + jsonData.count)
        frame.append(contentsOf: withUnsafeBytes(of: &length, Array.init))
        frame.append(jsonData)

        connection.send(content: frame, completion: .contentProcessed { _ in })
    }

    // MARK: - Private: Receive Loop

    private func startReceiving() {
        guard let connection else { return }
        receiveNextChunk(from: connection)
    }

    private func receiveNextChunk(from connection: NWConnection) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
            guard let self else { return }
            Task {
                if let data, !data.isEmpty {
                    await self.appendReceived(data)
                }
                if isComplete || error != nil {
                    await self.setState(.failed("Connection closed"))
                    return
                }
                await self.receiveNextChunk(from: connection)
            }
        }
    }

    private func appendReceived(_ data: Data) {
        receiveBuffer.append(data)
        processReceiveBuffer()
    }

    private func processReceiveBuffer() {
        while receiveBuffer.count >= 4 {
            let lengthBytes = receiveBuffer.prefix(4)
            let length = lengthBytes.withUnsafeBytes {
                UInt32(bigEndian: $0.load(as: UInt32.self))
            }
            let totalNeeded = 4 + Int(length)
            guard receiveBuffer.count >= totalNeeded else { break }

            let jsonData = receiveBuffer[4 ..< totalNeeded]
            receiveBuffer.removeFirst(totalNeeded)

            if let message = try? JSONDecoder().decode(SignalingMessage.self, from: jsonData) {
                handleMessage(message)
            }
        }
    }

    private func handleMessage(_ message: SignalingMessage) {
        onMessage?(message)

        switch message.type {
        case .helloAck:
            if message.accepted == true {
                setState(.sessionActive)
            } else {
                setState(.failed(message.reason ?? "Receiver rejected connection"))
            }
            onHelloAck?(message.accepted ?? false, message.reason)
        case .inputEvent:
            if let event = message.inputEvent {
                onInputEvent?(event)
            }
        default:
            break
        }
    }

    // MARK: - Private: Keepalive Loop

    private func startKeepalive(sessionID: String) {
        keepaliveTask?.cancel()
        keepaliveTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 1_000_000_000) // 1s
                guard let self, !Task.isCancelled else { break }
                try? await self.sendKeepalive()
            }
        }
    }

    // MARK: - Private: State

    private func setState(_ newState: SignalingClientState) {
        state = newState
        let cb = onStateChange
        Task { @MainActor in cb?(newState) }
    }
}

// MARK: - SignalingError

public enum SignalingError: LocalizedError {
    case notConnected
    case encodingFailed

    public var errorDescription: String? {
        switch self {
        case .notConnected:  return "Signaling channel not connected"
        case .encodingFailed: return "Failed to encode signaling message"
        }
    }
}

