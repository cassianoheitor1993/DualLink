import Foundation
import Network
import DualLinkCore

// MARK: - TransportDiscovery (Phase 3)
//
// Detects available transport modes (Wi-Fi, USB Ethernet via CDC-NCM).
// When a USB-C cable connects a Linux device running CDC-NCM gadget mode,
// macOS auto-creates a network interface.  This module finds it.

/// Describes a discovered transport endpoint.
public struct TransportEndpoint: Sendable {
    public let mode: ConnectionMode
    public let host: String
    public let interfaceName: String
    public let latencyEstimate: TimeInterval  // rough estimate in seconds

    public init(mode: ConnectionMode, host: String, interfaceName: String, latencyEstimate: TimeInterval) {
        self.mode = mode
        self.host = host
        self.interfaceName = interfaceName
        self.latencyEstimate = latencyEstimate
    }
}

/// Discovers and prioritises available transport connections to a DualLink receiver.
public final class TransportDiscovery: @unchecked Sendable {

    /// Well-known USB Ethernet subnet used by DualLink CDC-NCM gadget.
    /// Linux gadget: 10.0.1.1, macOS host: 10.0.1.2
    public static let usbGadgetPeerIP = "10.0.1.1"
    public static let usbGadgetHostIP = "10.0.1.2"
    public static let usbSubnet = "10.0.1"

    public init() {}

    // MARK: - Detect USB Ethernet

    /// Check if a USB Ethernet interface (CDC-NCM) is present.
    /// Returns the interface name and local IP if found.
    public func detectUSBEthernet() -> (interfaceName: String, localIP: String, peerIP: String)? {
        var ifaddr: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&ifaddr) == 0, let firstAddr = ifaddr else { return nil }
        defer { freeifaddrs(ifaddr) }

        var current: UnsafeMutablePointer<ifaddrs>? = firstAddr
        while let addr = current {
            defer { current = addr.pointee.ifa_next }

            // Only IPv4
            guard addr.pointee.ifa_addr?.pointee.sa_family == UInt8(AF_INET) else { continue }

            let name = String(cString: addr.pointee.ifa_name)

            // Skip loopback and well-known interfaces
            guard !name.hasPrefix("lo") else { continue }

            // Get IP address
            guard let sockaddr = addr.pointee.ifa_addr else { continue }
            var hostname = [CChar](repeating: 0, count: Int(NI_MAXHOST))
            guard getnameinfo(sockaddr, socklen_t(sockaddr.pointee.sa_len),
                              &hostname, socklen_t(hostname.count),
                              nil, 0, NI_NUMERICHOST) == 0 else { continue }
            let ip = String(cString: hostname)

            // Check if this interface is on the USB gadget subnet
            if ip.hasPrefix(Self.usbSubnet) {
                print("[TransportDiscovery] Found USB Ethernet: \(name) → \(ip)")
                return (interfaceName: name, localIP: ip, peerIP: Self.usbGadgetPeerIP)
            }
        }

        return nil
    }

    // MARK: - Probe Reachability

    /// Quick TCP probe to check if a host is reachable on the signaling port.
    /// Returns true if connection succeeds within `timeout`.
    public func probeReachability(host: String, port: UInt16 = 7879, timeout: TimeInterval = 2.0) async -> Bool {
        return await withCheckedContinuation { continuation in
            let endpoint = NWEndpoint.hostPort(host: NWEndpoint.Host(host), port: NWEndpoint.Port(rawValue: port)!)
            let connection = NWConnection(to: endpoint, using: .tcp)
            let resumed = LockedFlag()

            let timeoutWork = DispatchWorkItem {
                if resumed.trySet() {
                    connection.cancel()
                    continuation.resume(returning: false)
                }
            }
            DispatchQueue.global().asyncAfter(deadline: .now() + timeout, execute: timeoutWork)

            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    if resumed.trySet() {
                        timeoutWork.cancel()
                        connection.cancel()
                        continuation.resume(returning: true)
                    }
                case .failed, .cancelled:
                    if resumed.trySet() {
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

    // MARK: - Discover Best Transport

    /// Discover the best available transport to the receiver.
    ///
    /// Priority: USB > Wi-Fi (lower latency).
    ///
    /// - Parameter wifiHost: Known Wi-Fi IP/hostname of the receiver (from mDNS or manual entry).
    /// - Returns: Ordered list of available endpoints, best first.
    public func discoverEndpoints(wifiHost: String?) async -> [TransportEndpoint] {
        var endpoints: [TransportEndpoint] = []

        // 1. Check USB Ethernet
        if let usb = detectUSBEthernet() {
            let reachable = await probeReachability(host: usb.peerIP, timeout: 1.0)
            if reachable {
                endpoints.append(TransportEndpoint(
                    mode: .usb,
                    host: usb.peerIP,
                    interfaceName: usb.interfaceName,
                    latencyEstimate: 0.001  // ~1ms for USB Ethernet
                ))
                print("[TransportDiscovery] USB endpoint available: \(usb.peerIP) via \(usb.interfaceName)")
            } else {
                print("[TransportDiscovery] USB interface found (\(usb.interfaceName)) but peer \(usb.peerIP) not reachable")
            }
        }

        // 2. Check Wi-Fi
        if let host = wifiHost {
            let reachable = await probeReachability(host: host, timeout: 2.0)
            if reachable {
                endpoints.append(TransportEndpoint(
                    mode: .wifi,
                    host: host,
                    interfaceName: "wifi",
                    latencyEstimate: 0.005  // ~5ms for Wi-Fi
                ))
                print("[TransportDiscovery] Wi-Fi endpoint available: \(host)")
            }
        }

        return endpoints
    }

    /// Convenience: get the single best endpoint.
    public func bestEndpoint(wifiHost: String?) async -> TransportEndpoint? {
        let endpoints = await discoverEndpoints(wifiHost: wifiHost)
        return endpoints.first
    }
}

// MARK: - LockedFlag (thread-safe one-shot flag)

/// Atomic flag for ensuring a continuation is resumed exactly once.
private final class LockedFlag: @unchecked Sendable {
    private var value = false
    private let lock = NSLock()

    /// Atomically tries to set the flag. Returns `true` if this is the first call.
    func trySet() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if value { return false }
        value = true
        return true
    }
}
