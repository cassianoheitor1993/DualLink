import Foundation
import VideoToolbox
import CoreMedia
import CoreVideo
import DualLinkCore

// MARK: - VideoEncoder

/// Encoda frames de vídeo usando hardware acceleration (VideoToolbox).
///
/// Suporta H.264 e H.265, com modo de baixa latência para streaming em tempo real.
///
/// ## Uso
/// ```swift
/// let encoder = VideoEncoder()
/// try await encoder.configure(config: .default)
/// encoder.onEncodedData = { nalUnits in
///     // enviar via WebRTC / USB
/// }
/// encoder.encode(pixelBuffer: frame.pixelBuffer, presentationTime: frame.presentationTime)
/// ```
public final class VideoEncoder: @unchecked Sendable {

    // MARK: - Types

    public typealias EncodedDataHandler = @Sendable ([UInt8], CMTime, Bool) -> Void

    // MARK: - State

    public private(set) var isConfigured: Bool = false
    public private(set) var config: StreamConfig = .default

    /// Callback chamado com cada NAL unit encodada.
    /// Parâmetros: (nalData, presentationTime, isKeyframe)
    public var onEncodedData: EncodedDataHandler?

    // MARK: - Private

    private var compressionSession: VTCompressionSession?
    private let encoderQueue = DispatchQueue(label: "com.duallink.encoder", qos: .userInteractive)

    // MARK: - Init

    public init() {}

    deinit {
        invalidate()
    }

    // MARK: - Public API

    /// Configura a sessão de encoding.
    /// - Parameter config: Configuração de stream (resolução, codec, bitrate, fps).
    public func configure(config: StreamConfig) throws {
        invalidate()
        self.config = config

        let codecType: CMVideoCodecType = config.codec == .h265
            ? kCMVideoCodecType_HEVC
            : kCMVideoCodecType_H264

        var session: VTCompressionSession?
        let status = VTCompressionSessionCreate(
            allocator: kCFAllocatorDefault,
            width: Int32(config.resolution.width),
            height: Int32(config.resolution.height),
            codecType: codecType,
            encoderSpecification: nil,
            imageBufferAttributes: nil,
            compressedDataAllocator: nil,
            outputCallback: compressionOutputCallback,
            refcon: Unmanaged.passUnretained(self).toOpaque(),
            compressionSessionOut: &session
        )

        guard status == noErr, let session else {
            throw VideoEncoderError.sessionCreationFailed(status: status)
        }

        try applyProperties(to: session, config: config)
        VTCompressionSessionPrepareToEncodeFrames(session)

        compressionSession = session
        isConfigured = true
    }

    /// Encoda um único frame.
    /// Thread-safe — pode ser chamado de qualquer thread.
    public func encode(pixelBuffer: CVPixelBuffer, presentationTime: CMTime) {
        guard let session = compressionSession else { return }

        // Frame properties — forçar keyframe a cada N segundos é gerenciado pelo MaxKeyFrameInterval
        let frameProperties: CFDictionary? = nil

        // PERF: Use the outputCallback variant (registered at session creation) — non-blocking
        VTCompressionSessionEncodeFrame(
            session,
            imageBuffer: pixelBuffer,
            presentationTimeStamp: presentationTime,
            duration: CMTime(value: 1, timescale: CMTimeScale(config.targetFPS)),
            frameProperties: frameProperties,
            sourceFrameRefcon: nil,
            infoFlagsOut: nil
        )
    }

    /// Para o encoding e libera recursos.
    public func invalidate() {
        guard let session = compressionSession else { return }
        VTCompressionSessionInvalidate(session)
        compressionSession = nil
        isConfigured = false
    }

    // MARK: - Private Helpers

    private func applyProperties(to session: VTCompressionSession, config: StreamConfig) throws {
        var properties: [CFString: Any] = [
            // Modo real-time — essencial para streaming
            kVTCompressionPropertyKey_RealTime: kCFBooleanTrue!,

            // Sem B-frames — elimina latência de reordenação
            kVTCompressionPropertyKey_AllowFrameReordering: kCFBooleanFalse!,

            // Keyframe a cada 2 segundos (permite recovery mais rápido)
            kVTCompressionPropertyKey_MaxKeyFrameInterval: config.targetFPS * 2,

            // Bitrate médio alvo
            kVTCompressionPropertyKey_AverageBitRate: config.maxBitrateBps,

            // H.264: usar Baseline para máxima compatibilidade e mínima latência
            kVTCompressionPropertyKey_ProfileLevel: config.codec == .h264
                ? kVTProfileLevel_H264_Baseline_AutoLevel
                : kVTProfileLevel_HEVC_Main_AutoLevel,
        ]

        if config.lowLatencyMode {
            // MaxKeyFrameIntervalDuration takes a CFNumber (seconds), not CMTime.
            // Not supported by all HW encoders — treat as non-fatal.
            let status = VTSessionSetProperty(
                session,
                key: kVTCompressionPropertyKey_MaxKeyFrameIntervalDuration,
                value: NSNumber(value: 2.0)
            )
            if status != noErr {
                // -17281 = kVTPropertyNotSupportedErr — safe to ignore on some HW encoders
                print("[VideoEncoder] MaxKeyFrameIntervalDuration not supported (status \(status)), skipping")
            }
        }

        for (key, value) in properties {
            let status = VTSessionSetProperty(session, key: key, value: value as CFTypeRef)
            guard status == noErr else {
                throw VideoEncoderError.propertySetFailed(key: key as String, status: status)
            }
        }
    }
}

// MARK: - Compression Output Callback

/// Callback chamado pelo VideoToolbox quando um frame é encodado.
private let compressionOutputCallback: VTCompressionOutputCallback = { refcon, _, status, flags, sampleBuffer in
    guard status == noErr,
          let sampleBuffer,
          let refcon else { return }

    let encoder = Unmanaged<VideoEncoder>.fromOpaque(refcon).takeUnretainedValue()
    encoder.handleEncodedSampleBuffer(sampleBuffer, flags: flags)
}

extension VideoEncoder {
    /// Processa um CMSampleBuffer encodado e extrai NAL units.
    func handleEncodedSampleBuffer(_ sampleBuffer: CMSampleBuffer, flags: VTEncodeInfoFlags) {
        // Keyframe detection: check kCMSampleAttachmentKey_NotSync
        // A frame is a keyframe if NotSync is absent or false.
        let isKeyframe: Bool = !flags.contains(.frameDropped) && {
            guard let attachments = CMSampleBufferGetSampleAttachmentsArray(
                sampleBuffer, createIfNecessary: false
            ) as? [[CFString: Any]],
            let first = attachments.first else { return true }
            return first[kCMSampleAttachmentKey_NotSync] as? Bool != true
        }()

        guard let nalData = buildAnnexBNALData(from: sampleBuffer, isKeyframe: isKeyframe) else { return }
        onEncodedData?(nalData, sampleBuffer.presentationTimeStamp, isKeyframe)
    }

    private func buildAnnexBNALData(from sampleBuffer: CMSampleBuffer, isKeyframe: Bool) -> [UInt8]? {
        guard let dataBuffer = sampleBuffer.dataBuffer else { return nil }

        var dataLength = 0
        var dataPointer: UnsafeMutablePointer<Int8>?
        let status = CMBlockBufferGetDataPointer(
            dataBuffer,
            atOffset: 0,
            lengthAtOffsetOut: nil,
            totalLengthOut: &dataLength,
            dataPointerOut: &dataPointer
        )
        guard status == noErr, let dataPointer, dataLength > 0 else { return nil }

        let bytes = UnsafeRawPointer(dataPointer).assumingMemoryBound(to: UInt8.self)
        let startCode: [UInt8] = [0x00, 0x00, 0x00, 0x01]
        var annexB: [UInt8] = []
        annexB.reserveCapacity(dataLength + 64)

        if isKeyframe, let formatDesc = CMSampleBufferGetFormatDescription(sampleBuffer) {
            appendH264ParameterSets(from: formatDesc, into: &annexB, startCode: startCode)
        }

        var offset = 0
        while offset + 4 <= dataLength {
            let nalLen = Int(
                (UInt32(bytes[offset]) << 24)
                | (UInt32(bytes[offset + 1]) << 16)
                | (UInt32(bytes[offset + 2]) << 8)
                | UInt32(bytes[offset + 3])
            )
            offset += 4

            guard nalLen > 0, offset + nalLen <= dataLength else {
                return nil
            }

            annexB.append(contentsOf: startCode)
            let nalStart = bytes.advanced(by: offset)
            annexB.append(contentsOf: UnsafeBufferPointer(start: nalStart, count: nalLen))
            offset += nalLen
        }

        return annexB.isEmpty ? nil : annexB
    }

    private func appendH264ParameterSets(from formatDesc: CMFormatDescription, into out: inout [UInt8], startCode: [UInt8]) {
        var parameterSetCount: Int = 0
        var nalHeaderLength: Int32 = 0

        let firstStatus = CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
            formatDesc,
            parameterSetIndex: 0,
            parameterSetPointerOut: nil,
            parameterSetSizeOut: nil,
            parameterSetCountOut: &parameterSetCount,
            nalUnitHeaderLengthOut: &nalHeaderLength
        )
        guard firstStatus == noErr, parameterSetCount > 0 else { return }

        for idx in 0..<parameterSetCount {
            var ptr: UnsafePointer<UInt8>?
            var size: Int = 0
            let status = CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                formatDesc,
                parameterSetIndex: idx,
                parameterSetPointerOut: &ptr,
                parameterSetSizeOut: &size,
                parameterSetCountOut: nil,
                nalUnitHeaderLengthOut: nil
            )
            guard status == noErr, let ptr, size > 0 else { continue }
            out.append(contentsOf: startCode)
            out.append(contentsOf: UnsafeBufferPointer(start: ptr, count: size))
        }
    }
}

// MARK: - VideoEncoderError

public enum VideoEncoderError: LocalizedError {
    case sessionCreationFailed(status: OSStatus)
    case propertySetFailed(key: String, status: OSStatus)
    case notConfigured

    public var errorDescription: String? {
        switch self {
        case .sessionCreationFailed(let status):
            return "VTCompressionSession creation failed with status \(status)"
        case .propertySetFailed(let key, let status):
            return "Failed to set encoder property '\(key)': \(status)"
        case .notConfigured:
            return "VideoEncoder must be configured before encoding"
        }
    }
}
