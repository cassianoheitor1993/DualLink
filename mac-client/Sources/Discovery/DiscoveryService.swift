import Foundation
import Network
import DualLinkCore

// MARK: - DiscoveryService
//
// Usa mDNS/Bonjour para descoberta automática de dispositivos DualLink na rede local.
// Service type: _duallink._tcp.local.
//
// TODO: Sprint 1.3.2 — Implementar mDNS discovery completo

public final class DiscoveryService: ObservableObject {
    public static let serviceType = "_duallink._tcp"
    public static let serviceDomain = "local."
    public static let defaultPort: UInt16 = 8443

    @Published public private(set) var discoveredPeers: [PeerInfo] = []
    private var browser: NWBrowser?

    public init() {}

    /// Inicia a busca por dispositivos DualLink na rede local.
    public func startBrowsing() {
        let parameters = NWParameters()
        parameters.includePeerToPeer = true

        let browser = NWBrowser(
            for: .bonjourWithTXTRecord(type: Self.serviceType, domain: Self.serviceDomain),
            using: parameters
        )

        browser.stateUpdateHandler = { [weak self] state in
            switch state {
            case .failed(let error):
                print("[Discovery] Browser failed: \(error)")
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

    private func handleResults(_ results: Set<NWBrowser.Result>) {
        // TODO: Sprint 1.3.2 — parsear TXT records e construir PeerInfo
        // Por enquanto, apenas log
        print("[Discovery] Found \(results.count) devices")
    }
}
