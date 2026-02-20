import SwiftUI
import DualLinkCore
import VirtualDisplay
import ScreenCapture
import VideoEncoder
import Streaming
import Signaling
import InputInjection

@main
struct DualLinkApp: App {
    @StateObject private var appState = AppState()

    init() {
        // Prompt for Accessibility permission on first launch.
        // CGEvent injection requires this to control the pointer/keyboard.
        InputInjectionManager.ensureAccessibility(prompt: true)
    }

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
    let inputInjector         = InputInjectionManager()

    // MARK: - Private

    private var fpsCounter = FPSCounter()
    private var sessionID: String = ""
    private var reconnectTask: Task<Void, Never>?
    private var lastHost: String = ""
    private var lastConfig: StreamConfig = .default
    private var lastDisplayMode: DisplayMode = .extend
    private var reconnectAttempt: Int = 0
    private let maxReconnectAttempts: Int = 5

    // MARK: - Connect & Stream

    /// Full pipeline: virtual display → capture → encode → UDP send.
    /// - Parameters:
    ///   - host: IP address of the Linux receiver.
    ///   - config: Stream parameters.
    ///   - displayMode: Mirror (capture main screen) or Extend (create virtual display).
    func connectAndStream(to host: String, config: StreamConfig = .default, displayMode: DisplayMode = .extend) async {
        guard case .idle = connectionState else { return }
        lastError = nil
        sessionID = UUID().uuidString
        lastHost = host
        lastConfig = config
        lastDisplayMode = displayMode
        reconnectAttempt = 0

        do {
            // ── 1. Connect Signaling (TCP) ─────────────────────────────────────
            connectionState = .connecting(
                peer: PeerInfo(id: sessionID, name: host, address: host, port: 7879),
                attempt: 1
            )
            await signalingClient.configure(
                onStateChange: { [weak self] state in
                    guard let self else { return }
                    if case .failed(let reason) = state {
                        Task { @MainActor in
                            guard !self.isTearingDown else { return }
                            if case .streaming = self.connectionState {
                                print("[DualLink] Connection lost: \(reason) — attempting reconnect")
                                await self.attemptReconnect()
                            } else if case .reconnecting = self.connectionState {
                                // Already reconnecting
                            } else {
                                self.lastError = "Signaling failed: \(reason)"
                                await self.teardown()
                                self.connectionState = .error(reason: reason)
                            }
                        }
                    }
                },
                onInputEvent: { [weak self] event in
                    guard let self else { return }
                    print("[DualLink] Input event received: \(event)")
                    self.inputInjector.inject(event: event)
                }
            )
            try await signalingClient.connect(host: host)

            // ── 2. Display setup (depends on mode) ───────────────────────────
            let captureID: CGDirectDisplayID
            switch displayMode {
            case .extend:
                // Create virtual display and capture it (screen extension)
                try await virtualDisplayManager.create(
                    resolution: config.resolution,
                    refreshRate: config.targetFPS
                )
                guard let virtualDisplayID = virtualDisplayManager.activeDisplayID else {
                    throw DualLinkError.streamError("Virtual display ID unavailable")
                }
                captureID = virtualDisplayID
                inputInjector.configure(displayID: virtualDisplayID)
                print("[DualLink] Extend mode: capturing virtual display \(virtualDisplayID)")

            case .mirror:
                // No virtual display needed — capture the main screen directly
                captureID = CGMainDisplayID()
                inputInjector.configure(displayID: captureID)
                print("[DualLink] Mirror mode: capturing main display \(captureID)")
            }

            // ── 3. Configure Video Encoder ─────────────────────────────────
            try videoEncoder.configure(config: config)

            // ── 4. Connect Video Sender (UDP) ─────────────────────────────
            try await videoSender.connect(host: host)

            // ── 5. Wire Encoder → Sender ────────────────────────────────────
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
        reconnectTask?.cancel()
        reconnectTask = nil
        try? await signalingClient.sendStop(sessionID: sessionID)
        await teardown()
    }

    // MARK: - Reconnect

    private func attemptReconnect() async {
        reconnectAttempt += 1
        guard reconnectAttempt <= maxReconnectAttempts else {
            lastError = "Connection lost after \(maxReconnectAttempts) reconnect attempts"
            await teardown()
            connectionState = .error(reason: lastError!)
            return
        }

        let peer = PeerInfo(id: sessionID, name: lastHost, address: lastHost, port: 7879)
        connectionState = .reconnecting(peer: peer, attempt: reconnectAttempt)
        print("[DualLink] Reconnect attempt \(reconnectAttempt)/\(maxReconnectAttempts)")

        // Tear down current pipeline but don't go idle
        isTearingDown = true
        try? await screenCaptureManager.stopCapture()
        videoEncoder.onEncodedData = nil
        videoEncoder.invalidate()
        await videoSender.disconnect()
        await signalingClient.disconnect()
        // Keep virtual display alive across reconnects in extend mode
        isTearingDown = false

        // Exponential backoff: 1s, 2s, 4s, 8s, 16s
        let delaySec = UInt64(pow(2.0, Double(reconnectAttempt - 1)))
        print("[DualLink] Waiting \(delaySec)s before reconnect...")
        try? await Task.sleep(nanoseconds: delaySec * 1_000_000_000)

        guard !Task.isCancelled else { return }

        do {
            try await signalingClient.connect(host: lastHost)
            try videoEncoder.configure(config: lastConfig)
            try await videoSender.connect(host: lastHost)

            videoEncoder.onEncodedData = { [weak self] nalData, pts, isKeyframe in
                guard let self else { return }
                Task {
                    await self.videoSender.send(nalData: nalData, presentationTime: pts, isKeyframe: isKeyframe)
                    let sent = await self.videoSender.framesSent
                    await MainActor.run {
                        self.framesSent = sent
                        self.streamFPS = self.fpsCounter.tick()
                    }
                }
            }

            // Re-capture the same display
            let captureID: CGDirectDisplayID
            if lastDisplayMode == .extend, let vid = virtualDisplayManager.activeDisplayID {
                captureID = vid
            } else {
                captureID = CGMainDisplayID()
            }
            try await screenCaptureManager.startCapture(displayID: captureID, config: lastConfig) { [weak self] frame in
                guard let self else { return }
                self.videoEncoder.encode(pixelBuffer: frame.pixelBuffer, presentationTime: frame.presentationTime)
            }

            try await signalingClient.sendHello(sessionID: sessionID, config: lastConfig)

            reconnectAttempt = 0
            connectionState = .streaming(session: SessionInfo(
                sessionID: sessionID,
                peer: PeerInfo(id: sessionID, name: lastHost, address: lastHost, port: 7878),
                config: lastConfig,
                connectionMode: .wifi
            ))
            print("[DualLink] Reconnected successfully!")
        } catch {
            print("[DualLink] Reconnect failed: \(error.localizedDescription)")
            await attemptReconnect()
        }
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
