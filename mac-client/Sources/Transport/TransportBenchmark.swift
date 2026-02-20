import Foundation
import Network
import DualLinkCore

// MARK: - TransportBenchmark (Phase 3 — Story 3.1.3)
//
// Measures transport latency and throughput for the active connection.
// Used to validate USB vs Wi-Fi performance and for diagnostics.

/// Results of a transport benchmark run.
public struct BenchmarkResult: Sendable {
    public let mode: ConnectionMode
    public let host: String
    public let pingLatencyMs: Double         // Average RTT in milliseconds
    public let minLatencyMs: Double
    public let maxLatencyMs: Double
    public let throughputMbps: Double?        // nil if not measured
    public let packetLoss: Double             // 0.0 - 1.0

    public var summary: String {
        String(format: "%@ → %@ | ping: %.1fms (%.1f-%.1f) | loss: %.0f%%",
               mode.rawValue.uppercased(), host,
               pingLatencyMs, minLatencyMs, maxLatencyMs,
               packetLoss * 100)
    }
}

/// Measures transport performance to the DualLink receiver.
public final class TransportBenchmark: @unchecked Sendable {

    public init() {}

    // MARK: - Ping Latency

    /// Measure round-trip latency by TCP connect/close cycles.
    /// - Parameters:
    ///   - host: Target host IP
    ///   - port: Target port (default: signaling port)
    ///   - count: Number of pings
    /// - Returns: Benchmark result with latency stats
    public func measureLatency(host: String, port: UInt16 = 7879,
                               count: Int = 10, mode: ConnectionMode = .wifi) async -> BenchmarkResult {
        var latencies: [Double] = []
        var failures = 0

        for i in 0..<count {
            let start = CFAbsoluteTimeGetCurrent()
            let connected = await tcpPing(host: host, port: port, timeout: 2.0)
            let elapsed = (CFAbsoluteTimeGetCurrent() - start) * 1000  // ms

            if connected {
                latencies.append(elapsed)
            } else {
                failures += 1
            }

            // Small delay between pings
            if i < count - 1 {
                try? await Task.sleep(nanoseconds: 100_000_000) // 100ms
            }
        }

        let avg = latencies.isEmpty ? 0 : latencies.reduce(0, +) / Double(latencies.count)
        let minL = latencies.min() ?? 0
        let maxL = latencies.max() ?? 0
        let loss = Double(failures) / Double(count)

        let result = BenchmarkResult(
            mode: mode,
            host: host,
            pingLatencyMs: avg,
            minLatencyMs: minL,
            maxLatencyMs: maxL,
            throughputMbps: nil,
            packetLoss: loss
        )

        print("[Benchmark] \(result.summary)")
        return result
    }

    // MARK: - Compare Transports

    /// Benchmark both USB and Wi-Fi (if available) and return comparison.
    public func compareTransports(usbHost: String?, wifiHost: String?) async -> [BenchmarkResult] {
        var results: [BenchmarkResult] = []

        if let usbHost {
            let result = await measureLatency(host: usbHost, count: 5, mode: .usb)
            results.append(result)
        }

        if let wifiHost {
            let result = await measureLatency(host: wifiHost, count: 5, mode: .wifi)
            results.append(result)
        }

        return results.sorted { $0.pingLatencyMs < $1.pingLatencyMs }
    }

    // MARK: - Private

    private func tcpPing(host: String, port: UInt16, timeout: TimeInterval) async -> Bool {
        return await withCheckedContinuation { continuation in
            let endpoint = NWEndpoint.hostPort(host: NWEndpoint.Host(host), port: NWEndpoint.Port(rawValue: port)!)
            let connection = NWConnection(to: endpoint, using: .tcp)
            let flag = LockedFlag()

            let timeoutWork = DispatchWorkItem {
                if flag.trySet() {
                    connection.cancel()
                    continuation.resume(returning: false)
                }
            }
            DispatchQueue.global().asyncAfter(deadline: .now() + timeout, execute: timeoutWork)

            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    if flag.trySet() {
                        timeoutWork.cancel()
                        connection.cancel()
                        continuation.resume(returning: true)
                    }
                case .failed, .cancelled:
                    if flag.trySet() {
                        timeoutWork.cancel()
                        connection.cancel()
                        continuation.resume(returning: false)
                    }
                default:
                    break
                }
            }
            connection.start(queue: DispatchQueue.global(qos: .userInitiated))
        }
    }
}

// MARK: - LockedFlag (shared from TransportDiscovery)

/// Atomic flag for ensuring a continuation is resumed exactly once.
private final class LockedFlag: @unchecked Sendable {
    private var value = false
    private let lock = NSLock()

    func trySet() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if value { return false }
        value = true
        return true
    }
}
