import SwiftUI
import DualLinkCore
import VirtualDisplay
import ScreenCapture
import VideoEncoder
import Streaming
import Signaling
import InputInjection
import Transport

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

    // Per-display fps and frames for multi-monitor UI
    @Published var perDisplayFPS: [Double] = []
    @Published var perDisplayFramesSent: [UInt64] = []

    // When true we are already tearing down — suppress cascading onStateChange teardown
    private var isTearingDown = false

    // MARK: - Pipelines (one per active display)

    private var pipelines: [DisplayStreamPipeline] = []

    // MARK: - Shared managers (session-level, not per-display)

    let inputInjector         = InputInjectionManager()
    let transportDiscovery    = TransportDiscovery()
    let transportBenchmark    = TransportBenchmark()

    // MARK: - Private session state

    private var sessionID: String = ""
    private var reconnectTask: Task<Void, Never>?
    private var lastHost: String = ""
    private var lastConfig: StreamConfig = .default
    private var lastDisplayMode: DisplayMode = .extend
    private var lastTransportMode: TransportSelection = .auto
    private var lastWifiHost: String? = nil
    private var lastPairingPin: String? = nil
    private var lastDisplayCount: Int = 1
    private var activeConnectionMode: ConnectionMode = .wifi
    private var reconnectAttempt: Int = 0
    private let maxReconnectAttempts: Int = 5

    // MARK: - Connect & Stream

    /// Full pipeline: virtual display(s) → capture → encode → UDP send.
    ///
    /// - Parameters:
    ///   - config: Stream parameters (applied to every display).
    ///   - displayMode: Mirror (capture main) or Extend (create virtual displays).
    ///   - displayCount: Number of independent display streams (1–2 for Phase 5B).
    ///   - transportMode: Auto, USB, or Wi-Fi.
    ///   - wifiHost: Wi-Fi IP of the receiver.
    ///   - pairingPin: 6-digit PIN displayed by the receiver (required for first connect).
    func connectAndStream(config: StreamConfig = .default, displayMode: DisplayMode = .extend,
                          displayCount: Int = 1,
                          transportMode: TransportSelection = .auto, wifiHost: String? = nil,
                          pairingPin: String? = nil) async {
        guard case .idle = connectionState else { return }
        lastError = nil
        sessionID = UUID().uuidString
        lastConfig = config
        lastDisplayMode = displayMode
        lastDisplayCount = max(1, min(displayCount, 2))
        lastTransportMode = transportMode
        lastWifiHost = wifiHost
        lastPairingPin = pairingPin
        reconnectAttempt = 0

        // Reset per-display stats arrays
        perDisplayFPS = Array(repeating: 0.0, count: lastDisplayCount)
        perDisplayFramesSent = Array(repeating: 0, count: lastDisplayCount)

        do {
            // ── 0. Resolve transport endpoint ──────────────────────────────────
            let host: String
            let connMode: ConnectionMode

            switch transportMode {
            case .usb:
                guard let usb = transportDiscovery.detectUSBEthernet() else {
                    throw DualLinkError.streamError("No USB Ethernet detected. Is the USB-C cable connected and gadget configured?")
                }
                let reachable = await transportDiscovery.probeReachability(host: usb.peerIP, timeout: 2.0)
                guard reachable else {
                    throw DualLinkError.streamError("USB Ethernet detected (\(usb.interfaceName)) but receiver not reachable at \(usb.peerIP)")
                }
                host = usb.peerIP
                connMode = .usb
                print("[DualLink] Transport: USB via \(usb.interfaceName) → \(host)")

            case .wifi:
                guard let wifiHost, !wifiHost.isEmpty else {
                    throw DualLinkError.streamError("No Wi-Fi IP provided")
                }
                host = wifiHost
                connMode = .wifi
                print("[DualLink] Transport: Wi-Fi → \(host)")

            case .auto:
                if let endpoint = await transportDiscovery.bestEndpoint(wifiHost: wifiHost) {
                    host = endpoint.host
                    connMode = endpoint.mode
                    print("[DualLink] Transport: Auto selected \(connMode.rawValue) → \(host) (latency ~\(Int(endpoint.latencyEstimate * 1000))ms)")
                } else if let wifiHost, !wifiHost.isEmpty {
                    host = wifiHost
                    connMode = .wifi
                    print("[DualLink] Transport: Auto fallback to Wi-Fi → \(host)")
                } else {
                    throw DualLinkError.streamError("No transport available. Check USB cable or enter Wi-Fi IP.")
                }
            }

            lastHost = host
            activeConnectionMode = connMode

            // ── 1. Show connecting state ─────────────────────────────────────
            connectionState = .connecting(
                peer: PeerInfo(id: sessionID, name: host, address: host, port: 7879),
                attempt: 1
            )

            // ── 2. Create and start pipelines ────────────────────────────────
            pipelines.removeAll()
            for idx in 0..<lastDisplayCount {
                let pipeline = DisplayStreamPipeline(displayIndex: UInt8(idx))
                pipelines.append(pipeline)

                let capturedIdx = idx
                let stateHandler: (@Sendable (SignalingClientState) -> Void)?
                if idx == 0 {
                    stateHandler = { [weak self] state in
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
                    }
                } else {
                    stateHandler = nil
                }
                try await pipeline.start(
                    host: host,
                    displayMode: (idx == 0) ? displayMode : .extend,
                    config: config,
                    pairingPin: pairingPin,
                    sessionID: sessionID,
                    onFrameSent: { [weak self] in
                        guard let self else { return }
                        Task { @MainActor in
                            if capturedIdx < self.pipelines.count {
                                self.perDisplayFPS[capturedIdx] = self.pipelines[capturedIdx].fps
                                self.perDisplayFramesSent[capturedIdx] = self.pipelines[capturedIdx].framesSent
                            }
                            self.streamFPS = self.pipelines.reduce(0) { $0 + $1.fps }
                            self.framesSent = self.pipelines.reduce(0) { $0 + $1.framesSent }
                        }
                    },
                    onInputEvent: { [weak self] event in
                        guard let self else { return }
                        self.inputInjector.inject(event: event)
                    },
                    onSignalingStateChange: stateHandler
                )

                // Configure input injection for display 0
                if idx == 0 {
                    if displayMode == .extend,
                       let vid = pipeline.virtualDisplayManager.activeDisplayID {
                        inputInjector.configure(displayID: vid)
                    } else {
                        inputInjector.configure(displayID: CGMainDisplayID())
                    }
                }
            }

            // ── 3. Update UI state ───────────────────────────────────────────
            connectionState = .streaming(session: SessionInfo(
                sessionID: sessionID,
                peer: PeerInfo(id: sessionID, name: host, address: host, port: 7878),
                config: config,
                connectionMode: connMode
            ))

            // ── 4. Background: benchmark transport ───────────────────────────
            Task.detached { [weak self] in
                guard let self else { return }
                let result = await self.transportBenchmark.measureLatency(
                    host: host, count: 5, mode: connMode
                )
                print("[DualLink] Transport benchmark: \(result.summary)")
            }

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
        for pipeline in pipelines {
            try? await pipeline.signalingClient.sendStop(sessionID: sessionID)
        }
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

        // Partial teardown — keep virtual displays alive
        isTearingDown = true
        for pipeline in pipelines {
            await pipeline.stopKeepDisplay()
        }
        isTearingDown = false

        // Exponential backoff: 1s, 2s, 4s, 8s, 16s
        let delaySec = UInt64(pow(2.0, Double(reconnectAttempt - 1)))
        print("[DualLink] Waiting \(delaySec)s before reconnect...")
        try? await Task.sleep(nanoseconds: delaySec * 1_000_000_000)
        guard !Task.isCancelled else { return }

        // Re-discover best transport
        var reconnectHost = lastHost
        if lastTransportMode == .auto || lastTransportMode == .usb {
            if let endpoint = await transportDiscovery.bestEndpoint(wifiHost: lastWifiHost) {
                reconnectHost = endpoint.host
                activeConnectionMode = endpoint.mode
            } else if let wifiHost = lastWifiHost, !wifiHost.isEmpty {
                reconnectHost = wifiHost
                activeConnectionMode = .wifi
            }
        }
        lastHost = reconnectHost

        do {
            for (idx, pipeline) in pipelines.enumerated() {
                let capturedIdx = idx
                let reconnStateHandler: (@Sendable (SignalingClientState) -> Void)?
                if idx == 0 {
                    reconnStateHandler = { [weak self] state in
                        guard let self else { return }
                        if case .failed(let reason) = state {
                            Task { @MainActor in
                                guard !self.isTearingDown else { return }
                                if case .streaming = self.connectionState {
                                    await self.attemptReconnect()
                                }
                            }
                        }
                    }
                } else {
                    reconnStateHandler = nil
                }
                try await pipeline.start(
                    host: lastHost,
                    displayMode: (idx == 0) ? lastDisplayMode : .extend,
                    config: lastConfig,
                    pairingPin: lastPairingPin,
                    sessionID: sessionID,
                    onFrameSent: { [weak self] in
                        guard let self else { return }
                        Task { @MainActor in
                            if capturedIdx < self.pipelines.count {
                                self.perDisplayFPS[capturedIdx] = self.pipelines[capturedIdx].fps
                                self.perDisplayFramesSent[capturedIdx] = self.pipelines[capturedIdx].framesSent
                            }
                            self.streamFPS = self.pipelines.reduce(0) { $0 + $1.fps }
                            self.framesSent = self.pipelines.reduce(0) { $0 + $1.framesSent }
                        }
                    },
                    onInputEvent: { [weak self] event in
                        guard let self else { return }
                        self.inputInjector.inject(event: event)
                    },
                    onSignalingStateChange: reconnStateHandler
                )
            }

            reconnectAttempt = 0
            connectionState = .streaming(session: SessionInfo(
                sessionID: sessionID,
                peer: PeerInfo(id: sessionID, name: lastHost, address: lastHost, port: 7878),
                config: lastConfig,
                connectionMode: activeConnectionMode
            ))
            print("[DualLink] Reconnected successfully!")
        } catch {
            print("[DualLink] Reconnect failed: \(error.localizedDescription)")
            await attemptReconnect()
        }
    }

    // MARK: - Teardown

    private func teardown() async {
        guard !isTearingDown else { return }
        isTearingDown = true
        defer { isTearingDown = false }
        for pipeline in pipelines {
            await pipeline.stop()
        }
        pipelines.removeAll()
        connectionState = .idle
        streamFPS = 0
        framesSent = 0
        perDisplayFPS = []
        perDisplayFramesSent = []
    }
}

