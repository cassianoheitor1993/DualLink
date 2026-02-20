#!/usr/bin/env bash
# ── DualLink USB Gadget Setup (CDC-NCM) ──
#
# Configures the Linux machine as a USB network gadget so that connecting
# via USB-C to a macOS host creates a virtual Ethernet interface.
#
# Prerequisites:
#   - Linux kernel with configfs + dwc2/cdns3/etc. UDC driver
#   - libcomposite module
#   - Must run as root
#
# Network:
#   Linux  (gadget): 10.0.1.1
#   macOS  (host):   10.0.1.2  (auto-assigned or manual)
#
# Usage:
#   sudo ./setup-usb-gadget.sh
#
set -euo pipefail

GADGET_NAME="duallink"
GADGET_DIR="/sys/kernel/config/usb_gadget/${GADGET_NAME}"
VENDOR_ID="0x1d6b"   # Linux Foundation
PRODUCT_ID="0x0104"  # Multifunction Composite Gadget
SERIAL="DL$(date +%Y%m%d)"
MANUFACTURER="DualLink"
PRODUCT="DualLink Display Receiver"
USB_NET_IP="10.0.1.1"
USB_NET_MASK="255.255.255.0"

# ── Sanity checks ──

if [[ $EUID -ne 0 ]]; then
    echo "❌ This script must be run as root (sudo)"
    exit 1
fi

if [[ -d "$GADGET_DIR" ]]; then
    echo "⚠️  Gadget '${GADGET_NAME}' already exists at ${GADGET_DIR}"
    echo "   Run teardown-usb-gadget.sh first, or the gadget is already active."
    exit 0
fi

# ── Load required modules ──

modprobe libcomposite 2>/dev/null || true
modprobe usb_f_ncm    2>/dev/null || true

echo "📦 Creating USB gadget '${GADGET_NAME}'..."

# ── Create gadget ──

mkdir -p "${GADGET_DIR}"
cd "${GADGET_DIR}"

echo "${VENDOR_ID}"  > idVendor
echo "${PRODUCT_ID}" > idProduct

# USB 2.0
echo "0x0200" > bcdUSB
echo "0x0100" > bcdDevice

# Strings (English)
mkdir -p strings/0x409
echo "${SERIAL}"       > strings/0x409/serialnumber
echo "${MANUFACTURER}" > strings/0x409/manufacturer
echo "${PRODUCT}"      > strings/0x409/product

# ── Configuration ──

mkdir -p configs/c.1/strings/0x409
echo "CDC-NCM" > configs/c.1/strings/0x409/configuration
echo 250       > configs/c.1/MaxPower  # 250mA

# ── NCM Function ──

mkdir -p functions/ncm.usb0

# Optional: set fixed MAC addresses for stable interface naming
# Host MAC (macOS side)
echo "48:6f:73:74:4d:43" > functions/ncm.usb0/host_addr
# Device MAC (Linux side)
echo "44:65:76:4c:69:6e" > functions/ncm.usb0/dev_addr

# Link function to configuration
ln -sf functions/ncm.usb0 configs/c.1/

# ── Activate gadget ──

# Find UDC (USB Device Controller)
UDC=$(ls /sys/class/udc/ 2>/dev/null | head -n1)
if [[ -z "$UDC" ]]; then
    echo "❌ No USB Device Controller (UDC) found."
    echo "   Your machine may not support USB gadget mode."
    echo "   Check: ls /sys/class/udc/"
    exit 1
fi

echo "${UDC}" > UDC
echo "✅ Gadget activated on UDC: ${UDC}"

# ── Configure network ──

# Wait for interface to appear
sleep 1

IFACE="usb0"
if ! ip link show "${IFACE}" &>/dev/null; then
    echo "⚠️  Interface ${IFACE} not found, trying usb1..."
    IFACE="usb1"
fi

if ip link show "${IFACE}" &>/dev/null; then
    ip addr flush dev "${IFACE}" 2>/dev/null || true
    ip addr add "${USB_NET_IP}/24" dev "${IFACE}"
    ip link set "${IFACE}" up
    echo "✅ Network configured: ${IFACE} → ${USB_NET_IP}/24"
else
    echo "⚠️  No USB network interface found. Network config skipped."
    echo "   You may need to configure manually after the host connects."
fi

echo ""
echo "🔗 DualLink USB Gadget ready!"
echo "   Linux  (this machine): ${USB_NET_IP}"
echo "   macOS  (host):         10.0.1.2 (configure on Mac or let DHCP assign)"
echo ""
echo "   To test: ping 10.0.1.2 (after connecting USB-C cable)"
echo "   To remove: sudo ./teardown-usb-gadget.sh"
