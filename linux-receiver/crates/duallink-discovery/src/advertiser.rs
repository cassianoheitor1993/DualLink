//! mDNS service advertisement for the DualLink receiver.
//!
//! The receiver calls [`DualLinkAdvertiser::register`] at startup so that any
//! DualLink sender on the same subnet can discover it without manual IP entry.
//!
//! # TXT record keys
//!
//! | Key       | Value                                        |
//! |-----------|----------------------------------------------|
//! | `version` | Protocol version (`"1"`)                     |
//! | `displays` | Number of display channels being served     |
//! | `port`    | Base TCP signaling port (default `"7879"`)   |
//! | `host`    | Advertised LAN IP address                    |
//! | `fp`      | First 16 hex chars of the TLS fingerprint    |
//!
//! # Usage
//!
//! ```rust,no_run
//! use duallink_discovery::DualLinkAdvertiser;
//! use std::net::IpAddr;
//!
//! let ip: IpAddr = "192.168.1.42".parse().unwrap();
//! let adv = DualLinkAdvertiser::register(
//!     "DualLink Receiver",
//!     1,          // display count
//!     7879,       // base signaling port
//!     ip,
//!     "AABBCCDDEE112233", // short TLS fingerprint
//! ).expect("mDNS advertising failed");
//!
//! // When the receiver shuts down:
//! adv.unregister();
//! ```

use std::collections::HashMap;
use std::net::IpAddr;

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use tracing::{info, warn};

pub const SERVICE_TYPE: &str = "_duallink._tcp.local.";

/// Active mDNS service advertisement.  Drop or call [`unregister`] to stop.
pub struct DualLinkAdvertiser {
    daemon:   ServiceDaemon,
    fullname: String,
}

impl DualLinkAdvertiser {
    /// Register a DualLink receiver on the local mDNS domain.
    ///
    /// # Arguments
    /// - `instance_name` — human-readable instance name
    ///   (visible in sender discovery lists, e.g. `"DualLink Receiver"`)
    /// - `display_count` — number of display channels being served
    /// - `base_port` — TCP signaling port for display 0 (usually `7879`)
    /// - `host_ip` — local LAN IP address to advertise
    /// - `fingerprint` — TLS certificate fingerprint (colon-separated SHA-256 hex)
    pub fn register(
        instance_name: &str,
        display_count: u8,
        base_port: u16,
        host_ip: IpAddr,
        fingerprint: &str,
    ) -> Result<Self> {
        let daemon = ServiceDaemon::new()?;

        // Build hostname — e.g. "myhost.local."
        let raw_host = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "duallink-receiver".to_owned());
        let hostname = format!("{raw_host}.local.");

        // Short fingerprint: first 16 hex chars (8 bytes) — enough to identify the cert
        let fp_short: String = fingerprint
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(16)
            .collect();

        let mut properties = HashMap::new();
        properties.insert("version".to_owned(),  "1".to_owned());
        properties.insert("displays".to_owned(), display_count.to_string());
        properties.insert("port".to_owned(),     base_port.to_string());
        properties.insert("host".to_owned(),     host_ip.to_string());
        properties.insert("fp".to_owned(),       fp_short);

        let service = ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &hostname,
            host_ip,
            base_port,
            Some(properties),
        )?;

        let fullname = service.get_fullname().to_owned();
        daemon.register(service)?;

        info!(
            "[mDNS] Advertising '{}' at {}:{} (displays={})",
            instance_name, host_ip, base_port, display_count
        );

        Ok(Self { daemon, fullname })
    }

    /// Remove the mDNS advertisement.
    pub fn unregister(self) {
        if let Err(e) = self.daemon.unregister(&self.fullname) {
            warn!("[mDNS] Failed to unregister '{}': {}", self.fullname, e);
        } else {
            info!("[mDNS] Advertisement '{}' removed.", self.fullname);
        }
    }
}

// ── Local IP detection ────────────────────────────────────────────────────────

/// Detect the primary LAN IPv4 address by probing an external socket.
///
/// No packets are actually sent — this just queries the OS routing table.
pub fn detect_local_ip() -> IpAddr {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| { s.connect("8.8.8.8:80")?; s.local_addr() })
        .map(|a| a.ip())
        .unwrap_or_else(|_| IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)))
}
