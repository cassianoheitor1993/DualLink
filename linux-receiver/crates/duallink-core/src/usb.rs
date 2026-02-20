//! USB Ethernet detection for DualLink Phase 3
//!
//! Detects CDC-NCM USB Ethernet interfaces to determine if a USB-C
//! connection is available for transport.

use std::net::Ipv4Addr;
use tracing::info;

/// Well-known subnet for DualLink USB gadget connections.
pub const USB_GADGET_SUBNET: &str = "10.0.1";
pub const USB_GADGET_DEVICE_IP: &str = "10.0.1.1";
pub const USB_GADGET_HOST_IP: &str = "10.0.1.2";

/// Detected USB Ethernet interface information.
#[derive(Debug, Clone)]
pub struct UsbEthernetInfo {
    pub interface_name: String,
    pub local_ip: Ipv4Addr,
    pub peer_ip: Ipv4Addr,
}

/// Check if a USB Ethernet (CDC-NCM) interface is active.
///
/// Scans network interfaces for one on the DualLink USB subnet (10.0.1.x).
/// Returns `None` if no USB Ethernet is detected.
pub fn detect_usb_ethernet() -> Option<UsbEthernetInfo> {
    // Read /proc/net/if_inet6 or use getifaddrs
    // For simplicity, scan /sys/class/net/ and check IPs
    let net_dir = std::path::Path::new("/sys/class/net");
    if !net_dir.exists() {
        return None;
    }

    for entry in std::fs::read_dir(net_dir).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name().into_string().ok()?;

        // Skip loopback and common non-USB interfaces
        if name == "lo" || name.starts_with("wl") || name.starts_with("docker") {
            continue;
        }

        // Check if this is a USB interface (usb0, usb1, etc.)
        // Also check en* interfaces that might be USB Ethernet
        if name.starts_with("usb") || name.starts_with("enx") {
            // Read the IP address from /proc/net/fib_trie or use ip command
            if let Some(ip) = get_interface_ipv4(&name) {
                let ip_str = ip.to_string();
                if ip_str.starts_with(USB_GADGET_SUBNET) {
                    info!("Detected USB Ethernet: {} → {}", name, ip);
                    return Some(UsbEthernetInfo {
                        interface_name: name,
                        local_ip: ip,
                        peer_ip: USB_GADGET_HOST_IP.parse().unwrap(),
                    });
                }
            }
        }
    }

    None
}

/// Get IPv4 address of a network interface by reading /proc/net/fib_trie.
fn get_interface_ipv4(iface: &str) -> Option<Ipv4Addr> {
    // Try reading from /sys/class/net/<iface>/... or parse ip addr output
    // Simplest: read operstate and if UP, try to get address
    let operstate_path = format!("/sys/class/net/{}/operstate", iface);
    let operstate = std::fs::read_to_string(&operstate_path).ok()?;
    if operstate.trim() != "up" {
        return None;
    }

    // Use socket to get interface address (libc getifaddrs)
    // For portability, shell out to `ip -4 addr show <iface>`
    let output = std::process::Command::new("ip")
        .args(["-4", "-o", "addr", "show", iface])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Format: "2: usb0    inet 10.0.1.1/24 brd 10.0.1.255 scope global usb0"
    for word in stdout.split_whitespace() {
        if word.contains('.') && word.contains('/') {
            let ip_str = word.split('/').next()?;
            return ip_str.parse().ok();
        }
    }

    None
}
