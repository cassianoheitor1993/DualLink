import Foundation
import ScreenCaptureKit
import CoreMedia
import CoreVideo
import DualLinkCore

// MARK: - ScreenCaptureManager

/// Captura frames de um display específico usando ScreenCaptureKit.
///
/// ## Uso
/// ```swift
/// let manager = ScreenCaptureManager()
/// try await manager.requestPermission()
/// try await manager.startCapture(displayID: id, config: .default) { frame in
///     // processar CVPixelBuffer
/// }
/// ```
@MainActor
public final class ScreenCaptureManager: NSObject, ObservableObject {

    // MARK: - Types

    public typealias FrameHandler = @Sendable (CapturedFrame) -> Void

    // CVPixelBuffer backed by IOSurface is safe to cross actor boundaries.
    public struct CapturedFrame: @unchecked Sendable {
        /// Frame de vídeo — IOSurface-backed (zero-copy para VideoToolbox).
        public let pixelBuffer: CVPixelBuffer
        /// Timestamp de apresentação.
        public let presentationTime: CMTime
        /// Dimensões do frame.
        public let width: Int
        public let height: Int
    }

    // MARK: - State

    public enum State: Equatable {
        case idle
        case requestingPermission
        case ready
        case capturing(displayID: CGDirectDisplayID)
        case error(String)
    }

    @Published public private(set) var state: State = .idle
    @Published public private(set) var framesPerSecond: Double = 0

    // MARK: - Private

    private var stream: SCStream?
    private var frameHandler: FrameHandler?
    private var frameCount: Int = 0
    private var lastFPSUpdate: Date = .now
    private var lastPixelBuffer: CVPixelBuffer?
    private var lastPTS: CMTime = .zero
    private var repeatTimer: Timer?

    // MARK: - Init

    public override init() { super.init() }

    // MARK: - Public API

    /// Solicita permissão de Screen Recording.
    /// Deve ser chamado antes de `startCapture`.
    public func requestPermission() async throws {
        state = .requestingPermission

        // ScreenCaptureKit verifica automaticamente a permissão na primeira chiamada
        // Listar conteúdo capturável força o prompt do sistema
        do {
            _ = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            state = .ready
        } catch {
            state = .error("Screen recording permission denied")
            throw ScreenCaptureError.permissionDenied
        }
    }

    /// Inicia a captura de um display específico.
    /// - Parameters:
    ///   - displayID: ID do display a capturar (de `CGMainDisplayID()` ou virtual display).
    ///   - config: Configuração de stream.
    ///   - handler: Callback chamado a cada frame capturado.
    public func startCapture(
        displayID: CGDirectDisplayID,
        config: StreamConfig = .default,
        handler: @escaping FrameHandler
    ) async throws {
        guard state == .ready || state == .idle else { return }

        // Garantir permissão
        if state == .idle {
            try await requestPermission()
        }

        frameHandler = handler

        // Encontrar o SCDisplay correspondente ao displayID
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
        guard let scDisplay = content.displays.first(where: { $0.displayID == displayID }) else {
            throw ScreenCaptureError.displayNotFound(displayID: displayID)
        }

        // Configurar captura
        let filter = SCContentFilter(display: scDisplay, excludingWindows: [])
        let streamConfig = buildStreamConfig(from: config, display: scDisplay)

        // Criar e iniciar stream
        let stream = SCStream(filter: filter, configuration: streamConfig, delegate: nil)
        try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: .global(qos: .userInteractive))
        try await stream.startCapture()

        self.stream = stream
        state = .capturing(displayID: displayID)

        // Repeat last frame at target FPS when screen is static
        // (macOS 14 SCK stops delivering pixel data on unchanged content).
        let interval = 1.0 / Double(config.targetFPS)
        repeatTimer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self, let pb = self.lastPixelBuffer else { return }
                let pts = CMTimeAdd(self.lastPTS, CMTime(seconds: interval, preferredTimescale: 600))
                self.lastPTS = pts
                let frame = CapturedFrame(pixelBuffer: pb, presentationTime: pts,
                                          width: CVPixelBufferGetWidth(pb), height: CVPixelBufferGetHeight(pb))
                self.updateFPS()
                self.frameHandler?(frame)
            }
        }
    }

    /// Para a captura.
    public func stopCapture() async throws {
        repeatTimer?.invalidate()
        repeatTimer = nil
        lastPixelBuffer = nil
        guard case .capturing = state else { return }
        try await stream?.stopCapture()
        stream = nil
        frameHandler = nil
        state = .ready
    }

    // MARK: - Private Helpers

    private func buildStreamConfig(from config: StreamConfig, display: SCDisplay) -> SCStreamConfiguration {
        let streamConfig = SCStreamConfiguration()

        // --- Cause-2 fix (GT-CLICK-SNAP): ensure output dimensions exactly match
        // the display's aspect ratio so there is NEVER letterboxing or pillarboxing.
        // If config.resolution has a different AR (e.g. 1920×1080 on a 16:10 display),
        // ScreenCaptureKit would letter/pillarbox the content.  The Linux side normalises
        // pointer coordinates by the full frame size (including black bars), producing a
        // systematic click-position offset that grows toward the edges.
        // Fix: fit config.resolution as a bounding box, derive the actual output size from
        // the display's true AR.  Always force even dimensions for H.264 compatibility.
        let displayW = display.width    // logical pixels (points)
        let displayH = display.height   // logical pixels (points)

        let outW: Int
        let outH: Int

        if displayW > 0 && displayH > 0 {
            let displayAR  = Double(displayW) / Double(displayH)
            let requestedAR = Double(config.resolution.width) / Double(config.resolution.height)

            if displayAR >= requestedAR {
                // Display is wider than (or equal to) the requested AR → fit to width
                outW = config.resolution.width & ~1
                outH = Int((Double(config.resolution.width) / displayAR).rounded()) & ~1
            } else {
                // Display is taller than the requested AR → fit to height
                outH = config.resolution.height & ~1
                outW = Int((Double(config.resolution.height) * displayAR).rounded()) & ~1
            }
        } else {
            outW = config.resolution.width  & ~1
            outH = config.resolution.height & ~1
        }

        streamConfig.width  = outW
        streamConfig.height = outH
        streamConfig.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(config.targetFPS))

        // NV12 é o formato nativo para encoding H.264 — minimiza conversão
        streamConfig.pixelFormat = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange

        // scalesToFit: fills the output rectangle; with matching AR this is simply a scale
        streamConfig.scalesToFit = true

        // Capturar apenas o conteúdo visível
        streamConfig.showsCursor = true

        return streamConfig
    }
}

// MARK: - SCStreamOutput

extension ScreenCaptureManager: SCStreamOutput {
    nonisolated public func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of outputType: SCStreamOutputType
    ) {
        guard outputType == .screen,
              let pixelBuffer = sampleBuffer.imageBuffer else { return }

        let pts = sampleBuffer.presentationTimeStamp
        let frame = CapturedFrame(
            pixelBuffer: pixelBuffer,
            presentationTime: pts,
            width: CVPixelBufferGetWidth(pixelBuffer),
            height: CVPixelBufferGetHeight(pixelBuffer)
        )

        Task { @MainActor [weak self] in
            guard let self else { return }
            // Update last frame so the repeat timer has fresh content
            self.lastPixelBuffer = pixelBuffer
            self.lastPTS = pts
            self.updateFPS()
            self.frameHandler?(frame)
        }
    }

    @MainActor
    private func updateFPS() {
        frameCount += 1
        let now = Date.now
        let elapsed = now.timeIntervalSince(lastFPSUpdate)
        if elapsed >= 1.0 {
            framesPerSecond = Double(frameCount) / elapsed
            frameCount = 0
            lastFPSUpdate = now
        }
    }
}

// MARK: - ScreenCaptureError

public enum ScreenCaptureError: LocalizedError {
    case permissionDenied
    case displayNotFound(displayID: CGDirectDisplayID)
    case streamSetupFailed(underlying: Error)
    case captureNotActive

    public var errorDescription: String? {
        switch self {
        case .permissionDenied:
            return "Screen recording permission denied. Enable in System Settings > Privacy & Security."
        case .displayNotFound(let id):
            return "Display with ID \(id) not found in shareable content."
        case .streamSetupFailed(let error):
            return "Failed to set up capture stream: \(error.localizedDescription)"
        case .captureNotActive:
            return "No active capture stream."
        }
    }
}
