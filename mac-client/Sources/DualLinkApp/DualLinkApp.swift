import SwiftUI
import DualLinkCore
import VirtualDisplay
import ScreenCapture
import VideoEncoder
import Streaming
import Signaling

@main
struct DualLinkApp: App {
    @StateObject private var appState = AppState()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(appState)
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
        .commands {
            CommandGroup(replacing: .newItem) {}
        }
    }
}

// MARK: - AppState

/// Estado global da aplicação — gerencia ciclo de vida de todos os managers.
@MainActor
final class AppState: ObservableObject {
    @Published var connectionState: ConnectionState = .idle
    @Published var streamFPS: Double = 0
    @Published var framesSent: UInt64 = 0
    @Published var lastError: String?

    // When true we are already tearing down — suppress cascading onStateChange teardown
    private var isTearingDown = false

    // MARK: - Managers

    let virtualDisplayManager = VirtualDisplayManager()
    let screenCaptureManager  = ScreenCaptureManager()
    let videoEncoder          = VideoEncoder()
    let videoSender           = VideoSender()
    let signalingClient       = SignalingClient()

    // MARK: - Private

    private var fpsCounter = FPSCounter()
    private var sessionID: String = ""

    // MARK: - Connect & Stream

    /// Full pipeline: virtual display → capture → encode → UDP send.
    /// - Parameters:
    ///   - host: IP address of the Linux receiver.
    ///   - config: Stream parameters.
    func connectAndStream(to host: String, config: StreamConfig = .default) async {
        guard case .idle = connectionState else { return }
        lastError = nil
        sessionID = UUID().uuidString

        do {
            // ── 1. Connect Signaling (TCP) ─────────────────────────────────────
            connectionState = .connecting(
                peer: PeerInfo(id: sessionID, name: host, address: host, port: 7879),
                attempt: 1
            )
            await signalingClient.configure(onStateChange: { [weak self] state in
                guard let self else { return }
                if case .failed(let reason) = state {
                    Task { @MainActor in
                        // Only react to drops while actively streaming.
                        // During setup, the real error is already in lastError
                        // and teardown() is already being called from the catch block.
                        guard !self.isTearingDown,
                              case .streaming = self.connectionState else { return }
                        self.lastError = "Signaling lost: \(reason)"
                        await self.teardown()
                        self.connectionState = .error(reason: reason)
                    }
                }
            })
            try await signalingClient.connect(host: host)

            // ── 2. Create Virtual Display ────────────────────────────────────
            try await virtualDisplayManager.create(
                resolution: config.resolution,
                refreshRate: config.targetFPS
            )
            guard let displayID = virtualDisplayManager.activeDisplayID else {
                throw DualLinkError.streamError("Virtual display ID unavailable")
            }

            // ── 3. Configure Video Encoder ─────────────────────────────────
            try videoEncoder.configure(config: config)

            // ── 4. Connect Video Sender (UDP) ─────────────────────────────
            try await videoSender.connect(host: host)

            // ── 5. Wire Encoder → Sender ────────────────────────────────────
            videoEncoder.onEncodedData = { [weak self] nalData, pts, isKeyframe in
                guard let self else { return }
                Task {
                    await self.videoSender.send(
                        nalData: nalData,
                        presentationTime: pts,
                        isKeyframe: isKeyframe
                    )
                    let sent = await self.videoSender.framesSent
                    if sent == 1 {
                        print("[DualLink] First encoded frame sent: \(nalData.count) bytes keyframe=\(isKeyframe)")
                    }
                    await MainActor.run {
                        self.framesSent = sent
                        self.streamFPS = self.fpsCounter.tick()
                    }
                }
            }

            // ── 6. Start Screen Capture ───────────────────────────────────
            // TODO: capture virtualDisplayID once user starts placing windows on it.
            // For E2E pipeline test, capturing the main display confirms the full chain.
            let captureID = CGMainDisplayID()
            print("[DualLink] Capturing displayID \(captureID) (main display for E2E test)")
            try await screenCaptureManager.startCapture(displayID: captureID, config: config) { [weak self] frame in
                guard let self else { return }
                self.videoEncoder.encode(
                    pixelBuffer: frame.pixelBuffer,
                    presentationTime: frame.presentationTime
                )
            }

            // ── 7. Send Hello handshake ──────────────────────────────────
            try await signalingClient.sendHello(sessionID: sessionID, config: config)

            // ── 8. Update UI state ─────────────────────────────────────────
            connectionState = .streaming(session: SessionInfo(
                sessionID: sessionID,
                peer: PeerInfo(id: sessionID, name: host, address: host, port: 7878),
                config: config,
                connectionMode: .wifi
            ))

        } catch {
            let msg = error.localizedDescription
            lastError = msg
            connectionState = .error(reason: msg)
            await teardown()
        }
    }

    // MARK: - Stop

    func stopStreaming() async {
        try? await signalingClient.sendStop(sessionID: sessionID)
        await teardown()
    }

    // MARK: - Private

    private func teardown() async {
        guard !isTearingDown else { return }
        isTearingDown = true
        defer { isTearingDown = false }
        try? await screenCaptureManager.stopCapture()
        videoEncoder.onEncodedData = nil
        videoEncoder.invalidate()
        await videoSender.disconnect()
        await signalingClient.disconnect()
        await virtualDisplayManager.destroy()
        connectionState = .idle
        streamFPS = 0
        framesSent = 0
    }
}

// MARK: - FPSCounter

/// Lightweight rolling FPS counter.
private struct FPSCounter {
    private var timestamps: [Date] = []

    mutating func tick() -> Double {
        let now = Date()
        timestamps.append(now)
        timestamps = timestamps.filter { now.timeIntervalSince($0) < 1.0 }
        return Double(timestamps.count)
    }
}
