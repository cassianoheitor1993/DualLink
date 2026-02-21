import Foundation
import CoreGraphics
import CoreMedia
import DualLinkCore
import VirtualDisplay
import ScreenCapture
import VideoEncoder
import Streaming
import Signaling

// MARK: - DisplayStreamPipeline

/// Encapsulates all managers needed for a single display stream.
///
/// One pipeline = one virtual display → capture → encode → UDP send loop.
/// A session with `displayCount = N` creates N pipelines, each bound to
/// port pair `(7878+2n, 7879+2n)` and tagged with `display_index = n`.
@MainActor
final class DisplayStreamPipeline {

    // MARK: - Identity

    let displayIndex: UInt8

    // MARK: - Managers (one independent set per display)

    let virtualDisplayManager = VirtualDisplayManager()
    let screenCaptureManager  = ScreenCaptureManager()
    let videoEncoder          = VideoEncoder()
    let videoSender           = VideoSender()
    let signalingClient       = SignalingClient()

    // MARK: - Observed state (aggregated by AppState)

    var fps: Double = 0
    var framesSent: UInt64 = 0

    private var fpsCounter = PipelineFPSCounter()

    // MARK: - Init

    init(displayIndex: UInt8) {
        self.displayIndex = displayIndex
    }

    // MARK: - Start

    /// Brings up the full pipeline for this display.
    ///
    /// - Parameters:
    ///   - host: Target receiver IP.
    ///   - displayMode: Extend (virtual display) or Mirror (main screen).
    ///   - config: Stream parameters; `display_index` is overwritten to match this instance.
    ///   - pairingPin: Optional 6-digit pairing PIN (only required on first connect).
    ///   - sessionID: Shared session UUID from the outer `AppState`.
    ///   - onFrameSent: Called on every sent frame so AppState can update aggregate FPS.
    ///   - onInputEvent: Called when the receiver sends an input event back.
    ///   - onSignalingStateChange: Forwarded from `SignalingClient` for reconnect handling.
    func start(
        host: String,
        displayMode: DisplayMode,
        config: StreamConfig,
        pairingPin: String?,
        sessionID: String,
        onFrameSent: @escaping @Sendable () -> Void,
        onInputEvent: @escaping @Sendable (InputEvent) -> Void,
        onSignalingStateChange: (@Sendable (SignalingClientState) -> Void)? = nil
    ) async throws {
        // Inject display_index into config
        var streamConfig = config
        streamConfig.displayIndex = displayIndex

        // ── Wire signaling ─────────────────────────────────────────────────
        await signalingClient.configure(
            onStateChange: onSignalingStateChange,
            onInputEvent: onInputEvent
        )
        try await signalingClient.connect(host: host, displayIndex: displayIndex)

        // ── Display setup ──────────────────────────────────────────────────
        let captureID: CGDirectDisplayID
        switch displayMode {
        case .extend:
            try await virtualDisplayManager.create(
                resolution: streamConfig.resolution,
                refreshRate: streamConfig.targetFPS
            )
            guard let vid = virtualDisplayManager.activeDisplayID else {
                throw DualLinkError.streamError(
                    "Virtual display \(displayIndex) ID unavailable after creation"
                )
            }
            captureID = vid
            print("[DualLink] Display[\(displayIndex)] extend mode: virtual display \(vid)")

        case .mirror:
            captureID = CGMainDisplayID()
            print("[DualLink] Display[\(displayIndex)] mirror mode: main display \(captureID)")
        }

        // ── Encoder ────────────────────────────────────────────────────────
        try videoEncoder.configure(config: streamConfig)

        // ── UDP sender (display-specific port) ─────────────────────────────
        try await videoSender.connect(host: host, displayIndex: displayIndex)

        // ── Encoder → Sender wiring ────────────────────────────────────────
        videoEncoder.onEncodedData = { [weak self] nalData, pts, isKeyframe in
            guard let self else { return }
            Task {
                let senderState = await self.videoSender.state
                guard senderState == .ready else {
                    self.videoEncoder.notifyFrameDropped()
                    return
                }
                await self.videoSender.send(
                    nalData: nalData,
                    presentationTime: pts,
                    isKeyframe: isKeyframe
                )
                self.videoEncoder.notifyFrameSent()
                let sent = await self.videoSender.framesSent
                await MainActor.run {
                    self.framesSent = sent
                    self.fps = self.fpsCounter.tick()
                    onFrameSent()
                }
            }
        }

        // ── Start capture ──────────────────────────────────────────────────
        try await screenCaptureManager.startCapture(
            displayID: captureID,
            config: streamConfig
        ) { [weak self] frame in
            guard let self else { return }
            self.videoEncoder.encode(
                pixelBuffer: frame.pixelBuffer,
                presentationTime: frame.presentationTime
            )
        }

        // ── Hello handshake ────────────────────────────────────────────────
        try await signalingClient.sendHello(
            sessionID: sessionID,
            config: streamConfig,
            pairingPin: pairingPin
        )

        print("[DualLink] Display[\(displayIndex)] pipeline started successfully")
    }

    // MARK: - Stop

    /// Full teardown including virtual display destruction.
    func stop() async {
        try? await screenCaptureManager.stopCapture()
        videoEncoder.onEncodedData = nil
        videoEncoder.invalidate()
        await videoSender.disconnect()
        await signalingClient.disconnect()
        await virtualDisplayManager.destroy()
        fps = 0
        framesSent = 0
        print("[DualLink] Display[\(displayIndex)] pipeline stopped")
    }

    /// Partial teardown — keep virtual display alive for reconnect.
    func stopKeepDisplay() async {
        try? await screenCaptureManager.stopCapture()
        videoEncoder.onEncodedData = nil
        videoEncoder.invalidate()
        await videoSender.disconnect()
        await signalingClient.disconnect()
    }
}

// MARK: - PipelineFPSCounter

private struct PipelineFPSCounter {
    private var timestamps: [Date] = []

    mutating func tick() -> Double {
        let now = Date()
        timestamps.append(now)
        timestamps = timestamps.filter { now.timeIntervalSince($0) < 1.0 }
        return Double(timestamps.count)
    }
}
