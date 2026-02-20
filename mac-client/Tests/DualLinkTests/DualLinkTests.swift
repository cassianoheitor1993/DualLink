import XCTest
@testable import DualLinkCore
@testable import VideoEncoder

final class VideoEncoderConfigTests: XCTestCase {

    func test_defaultConfig_hasExpectedValues() {
        let config = StreamConfig.default

        XCTAssertEqual(config.resolution, .fhd)
        XCTAssertEqual(config.targetFPS, 30)
        XCTAssertEqual(config.maxBitrateBps, 8_000_000)
        XCTAssertEqual(config.codec, .h264)
        XCTAssertTrue(config.lowLatencyMode)
    }

    func test_resolution_aspectRatio() {
        XCTAssertEqual(Resolution.fhd.aspectRatio, 16.0 / 9.0, accuracy: 0.001)
        XCTAssertEqual(Resolution.qhd.aspectRatio, 16.0 / 9.0, accuracy: 0.001)
        XCTAssertEqual(Resolution.uhd.aspectRatio, 16.0 / 9.0, accuracy: 0.001)
    }

    func test_videoEncoder_requiresConfigurationBeforeEncode() throws {
        let encoder = VideoEncoder()
        XCTAssertFalse(encoder.isConfigured)
    }

    func test_videoEncoder_configuresSuccessfully() throws {
        let encoder = VideoEncoder()
        // Encoding H.264 pode falhar em CI sem GPU — testar apenas que não lança erro de configuração
        do {
            try encoder.configure(config: .default)
            XCTAssertTrue(encoder.isConfigured)
            encoder.invalidate()
        } catch VideoEncoderError.sessionCreationFailed {
            // Aceitável em ambientes sem hardware encoder
            throw XCTSkip("Hardware H.264 encoder unavailable in this environment")
        }
    }
}

// MARK: - ConnectionState Tests

final class ConnectionStateTests: XCTestCase {

    func test_streamingState_isActive() {
        let peer = PeerInfo(id: "1", name: "Legion", address: "192.168.1.100", port: 8443)
        let session = SessionInfo(
            sessionID: "abc",
            peer: peer,
            config: .default,
            connectionMode: .wifi
        )
        let state = ConnectionState.streaming(session: session)
        XCTAssertTrue(state.isActive)
    }

    func test_idleState_isNotActive() {
        XCTAssertFalse(ConnectionState.idle.isActive)
    }

    func test_connectingState_isNotActive() {
        let peer = PeerInfo(id: "1", name: "Legion", address: "192.168.1.100", port: 8443)
        XCTAssertFalse(ConnectionState.connecting(peer: peer, attempt: 1).isActive)
    }
}
