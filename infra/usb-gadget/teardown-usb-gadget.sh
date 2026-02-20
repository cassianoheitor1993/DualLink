#!/usr/bin/env bash
# ── DualLink USB Gadget Teardown ──
#
# Removes the CDC-NCM USB gadget configuration.
#
# Usage:
#   sudo ./teardown-usb-gadget.sh
#
set -euo pipefail

GADGET_NAME="duallink"
GADGET_DIR="/sys/kernel/config/usb_gadget/${GADGET_NAME}"

if [[ $EUID -ne 0 ]]; then
    echo "❌ This script must be run as root (sudo)"
    exit 1
fi

if [[ ! -d "$GADGET_DIR" ]]; then
    echo "ℹ️  Gadget '${GADGET_NAME}' does not exist. Nothing to do."
    exit 0
fi

echo "🔌 Tearing down USB gadget '${GADGET_NAME}'..."

cd "${GADGET_DIR}"

# Disable UDC
echo "" > UDC 2>/dev/null || true

# Remove function symlinks from configuration
rm -f configs/c.1/ncm.usb0

# Remove strings and configs
rmdir configs/c.1/strings/0x409 2>/dev/null || true
rmdir configs/c.1 2>/dev/null || true

# Remove functions
rmdir functions/ncm.usb0 2>/dev/null || true

# Remove gadget strings
rmdir strings/0x409 2>/dev/null || true

# Remove gadget directory
cd /
rmdir "${GADGET_DIR}" 2>/dev/null || true

echo "✅ USB gadget removed."
