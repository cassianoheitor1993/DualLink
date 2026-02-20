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
    }

    /// Para a captura.
    public func stopCapture() async throws {
        guard case .capturing = state else { return }
        try await stream?.stopCapture()
        stream = nil
        frameHandler = nil
        state = .ready
    }

    // MARK: - Private Helpers

    private func buildStreamConfig(from config: StreamConfig, display: SCDisplay) -> SCStreamConfiguration {
        let streamConfig = SCStreamConfiguration()
        streamConfig.width = config.resolution.width
        streamConfig.height = config.resolution.height
        streamConfig.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(config.targetFPS))

        // NV12 é o formato nativo para encoding H.264 — minimiza conversão
        streamConfig.pixelFormat = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange

        // Escalar para resolução alvo se diferente do display físico
        streamConfig.scalesToFit = true

        // Capturar apenas o conteúdo visível
        streamConfig.showsCursor = false

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

        let frame = CapturedFrame(
            pixelBuffer: pixelBuffer,
            presentationTime: sampleBuffer.presentationTimeStamp,
            width: CVPixelBufferGetWidth(pixelBuffer),
            height: CVPixelBufferGetHeight(pixelBuffer)
        )

        // Métricas de FPS + frame delivery on MainActor
        Task { @MainActor [weak self] in
            self?.updateFPS()
            self?.frameHandler?(frame)
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
