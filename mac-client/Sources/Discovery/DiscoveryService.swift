import Foundation
import Network
import DualLinkCore

// MARK: - DiscoveryService
//
// Browses for DualLink receivers advertised via mDNS/Bonjour:
//   Service type: _duallink._tcp.local.
//
// TXT record keys read from the receiver:
//   version  = "1"
//   displays = number of display channels
//   port     = base TCP signaling port (default 7879)
//   host     = LAN IP address
//   fp       = first 16 hex chars of TLS fingerprint

public final class DiscoveryService: ObservableObject {
    public static let serviceType   = "_duallink._tcp"
    public static let serviceDomain = "local."

    /// All currently visible DualLink receivers on the LAN.
    @Published public private(set) var discoveredPeers: [PeerInfo] = []

    private var browser: NWBrowser?

    public init() {}

    /// Start Bonjour browsing for DualLink receivers.
    public func startBrowsing() {
        stopBrowsing()
        let params = NWParameters()
        params.includePeerToPeer = true

        let browser = NWBrowser(
            for: .bonjourWithTXTRecord(type: Self.serviceType, domain: Self.serviceDomain),
            using: params
        )

        browser.stateUpdateHandler = { state in
            switch state {
            case .failed(let error):
                print("[Discovery] Browser failed: \(error)")
            case .ready:
                print("[Discovery] Browsing for DualLink receivers…")
            default:
                break
            }
        }

        browser.browseResultsChangedHandler = { [weak self] results, _ in
            self?.handleResults(results)
        }

        browser.start(queue: .main)
        self.browser = browser
    }

    public func stopBrowsing() {
        browser?.cancel()
        browser = nil
        discoveredPeers = []
    }

    // MARK: - Private

    private func handleResults(_ results: Set<NWBrowser.Result>) {
        var peers: [PeerInfo] = []

        for result in results {
            // Only handle Bonjour service endpoints
            guard case .service(let name, _, _, _) = result.endpoint else { continue }

            // Extract TXT record values
            var host: String = ""
            var port: Int    = 7879
            var displays: Int = 1

            if case .bonjour(let txt) = result.metadata {
                host     = txt.dictionary["host"]     ?? ""
                port     = Int(txt.dictionary["port"]     ?? "") ?? 7879
                displays = Int(txt.dictionary["displays"] ?? "") ?? 1
            }

            // Skip if we couldn't determine an IP address
            guard !host.isEmpty else {
                print("[Discovery] Receiver '\(name)' has no 'host' TXT record — skipping")
                continue
            }

            let peer = PeerInfo(
                id:      name,
                name:    name,
                address: host,
                port:    port
            )
            peers.append(peer)
            print("[Discovery] Found: \(name) at \(host):\(port) (\(displays) display(s))")
        }

        // Sort by name for stable ordering
        peers.sort { $0.name < $1.name }
        discoveredPeers = peers
    }
}

// MARK: - NWTXTRecord convenience

private extension NWTXTRecord {
    /// Extract a string value from an NWTXTRecord.Entry.
    private static func entryString(_ entry: NWTXTRecord.Entry) -> String? {
        switch entry {
        case .string(let s): return s
        case .data(let d):   return String(data: d, encoding: .utf8)
        case .none:          return nil
        @unknown default:    return nil
        }
    }

    /// All DualLink TXT keys as a [String: String] dictionary.
    var dictionary: [String: String] {
        let keys = ["host", "port", "displays", "fp"]
        var result: [String: String] = [:]
        for key in keys {
            if let entry = getEntry(for: key),
               let value = Self.entryString(entry) {
                result[key] = value
            }
        }
        return result
    }
}
