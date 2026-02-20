import Foundation
import Network
import CoreMedia
import DualLinkCore

// MARK: - DualLink UDP Transport Protocol v1
//
// Each encoded H.264 frame is fragmented into MTU-sized UDP packets.
//
// Packet layout (16-byte header + up to 1384 bytes payload):
//
//  ┌────────────────────────────────────────────────┐
//  │  magic       UInt32  0x444C4E4B ("DLNK")       │
//  │  frameSeq    UInt32  monotonic frame counter    │
//  │  fragIndex   UInt16  0-based fragment index     │
//  │  fragCount   UInt16  total fragments this frame │
//  │  ptsMs       UInt32  presentation time (ms)     │
//  │  flags       UInt8   bit0=keyframe              │
//  │  reserved    UInt8[3]                           │
//  ├────────────────────────────────────────────────┤
//  │  payload     [UInt8] NAL data slice             │
//  └────────────────────────────────────────────────┘
//
// Fragmentation: frames larger than kMaxPayloadBytes are split across
// multiple UDP datagrams. The receiver reassembles using (frameSeq, fragIndex, fragCount).

let kMagic: UInt32       = 0x444C_4E4B
let kHeaderSize: Int     = 16
let kMaxPayloadBytes: Int = 1_384   // MTU 1400 − 16 header
let kDefaultPort: UInt16 = 7878     // DualLink video UDP port
let kDefaultSignalingPort: UInt16 = 7879  // DualLink signaling TCP port

// MARK: - FramePacketizer

/// Splits one encoded frame into a sequence of fixed-size UDP-ready packets.
///
/// Pure function — no state, thread-safe by construction.
public struct FramePacketizer {

    /// Encapsulates a single UDP datagram ready to be sent.
    public struct Packet {
        /// Complete datagram bytes (header + payload).
        public let data: Data
        /// Frame sequence number this packet belongs to.
        public let frameSeq: UInt32
        /// Fragment index within the frame.
        public let fragmentIndex: UInt16
        /// Total number of fragments in the frame.
        public let fragmentCount: UInt16
    }

    public static func packetize(
        nalData: [UInt8],
        frameSeq: UInt32,
        ptsMs: UInt32,
        isKeyframe: Bool
    ) -> [Packet] {
        guard !nalData.isEmpty else { return [] }

        let payloadSize = kMaxPayloadBytes
        let totalBytes = nalData.count
        let numFragments = max(1, (totalBytes + payloadSize - 1) / payloadSize)
        let fragCount = UInt16(numFragments)
        var packets: [Packet] = []
        packets.reserveCapacity(numFragments)

        for i in 0..<numFragments {
            let offset = i * payloadSize
            let length = min(payloadSize, totalBytes - offset)
            let slice = nalData[offset ..< offset + length]

            var datagram = Data(capacity: kHeaderSize + length)

            // magic
            var magic = kMagic.bigEndian
            datagram.append(contentsOf: withUnsafeBytes(of: &magic, Array.init))

            // frameSeq
            var seq = frameSeq.bigEndian
            datagram.append(contentsOf: withUnsafeBytes(of: &seq, Array.init))

            // fragIndex
            var fragIdx = UInt16(i).bigEndian
            datagram.append(contentsOf: withUnsafeBytes(of: &fragIdx, Array.init))

            // fragCount
            var fragCnt = fragCount.bigEndian
            datagram.append(contentsOf: withUnsafeBytes(of: &fragCnt, Array.init))

            // ptsMs
            var pts = ptsMs.bigEndian
            datagram.append(contentsOf: withUnsafeBytes(of: &pts, Array.init))

            // flags
            let flags: UInt8 = isKeyframe ? 0x01 : 0x00
            datagram.append(flags)

            // reserved (3 bytes)
            datagram.append(contentsOf: [0x00, 0x00, 0x00])

            // payload
            datagram.append(contentsOf: slice)

            packets.append(Packet(
                data: datagram,
                frameSeq: frameSeq,
                fragmentIndex: UInt16(i),
                fragmentCount: fragCount
            ))
        }

        return packets
    }
}

// MARK: - VideoSenderState

public enum VideoSenderState: Equatable, Sendable {
    case idle
    case connecting
    case ready
    case failed(String)
}

// MARK: - VideoSender

/// Sends encoded H.264/H.265 frames to the Linux receiver over UDP.
///
/// ## Usage
/// ```swift
/// let sender = VideoSender()
/// await sender.connect(host: "192.168.1.42", port: 7878)
/// // ... in encoder callback:
/// await sender.send(nalData: bytes, presentationTime: pts, isKeyframe: true)
/// ```
public actor VideoSender {

    // MARK: - State

    public private(set) var state: VideoSenderState = .idle
    public var onStateChange: (@Sendable (VideoSenderState) -> Void)?

    // MARK: - Stats (for UI / debug)

    public private(set) var framesSent: UInt64 = 0
    public private(set) var bytesSent: UInt64 = 0
    public private(set) var framesDropped: UInt64 = 0

    // MARK: - Private

    private var connection: NWConnection?
    private var frameSeq: UInt32 = 0
    private let sendQueue = DispatchQueue(label: "com.duallink.video-sender", qos: .userInteractive)

    // MARK: - Init

    public init() {}

    // MARK: - Configure Callbacks

    /// Sets callback handlers. Call before `connect`.
    public func configure(
        onStateChange: (@Sendable (VideoSenderState) -> Void)? = nil
    ) {
        self.onStateChange = onStateChange
    }

    // MARK: - Connect

    /// Opens a UDP "connection" to the receiver.
    /// Network.framework UDP connections are soft — no actual handshake.
    public func connect(host: String, port: UInt16 = 7878) async throws {
        disconnect()
        state = .connecting
        notifyState()

        let endpoint = NWEndpoint.hostPort(
            host: NWEndpoint.Host(host),
            port: NWEndpoint.Port(rawValue: port)!
        )
        let conn = NWConnection(to: endpoint, using: .udp)
        connection = conn

        return try await withCheckedThrowingContinuation { continuation in
            conn.stateUpdateHandler = { [weak self] newState in
                guard let self else { return }
                Task {
                    switch newState {
                    case .ready:
                        await self.setState(.ready)
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
            conn.start(queue: self.sendQueue)
        }
    }

    // MARK: - Send

    /// Packetizes and sends one encoded frame.
    ///
    /// - Parameters:
    ///   - nalData: Raw H.264/H.265 NAL unit bytes from VideoEncoder.
    ///   - presentationTime: Frame PTS from VideoToolbox.
    ///   - isKeyframe: Whether this is a keyframe (IDR).
    public func send(
        nalData: [UInt8],
        presentationTime: CMTime,
        isKeyframe: Bool
    ) {
        guard state == .ready, let connection else {
            framesDropped += 1
            return
        }

        let seq = frameSeq
        frameSeq &+= 1

        // Convert PTS to milliseconds (UInt32 wraps after ~49 days — acceptable)
        let ptsMs = UInt32(max(0, presentationTime.seconds * 1000))

        let packets = FramePacketizer.packetize(
            nalData: nalData,
            frameSeq: seq,
            ptsMs: ptsMs,
            isKeyframe: isKeyframe
        )

        var totalBytes: Int = 0
        for packet in packets {
            let data = packet.data
            totalBytes += data.count
            connection.send(
                content: data,
                completion: .contentProcessed { _ in
                    // UDP is fire-and-forget — ignore send errors
                }
            )
        }

        framesSent += 1
        bytesSent += UInt64(totalBytes)
    }

    // MARK: - Disconnect

    public func disconnect() {
        connection?.cancel()
        connection = nil
        frameSeq = 0
        setState(.idle)
        notifyState()
    }

    // MARK: - Private Helpers

    private func setState(_ newState: VideoSenderState) {
        state = newState
        notifyState()
    }

    private func notifyState() {
        let current = state
        let cb = onStateChange
        Task { @MainActor in cb?(current) }
    }
}

