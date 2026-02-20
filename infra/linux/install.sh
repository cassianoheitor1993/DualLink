#!/usr/bin/env bash
# DualLink Linux Receiver — install script
# Usage:  sudo ./install.sh [--uninstall]
set -euo pipefail

BINARY_NAME="duallink-receiver"
INSTALL_DIR="/usr/local/bin"
SERVICE_NAME="duallink-receiver.service"
SERVICE_SRC="$(dirname "$0")/$SERVICE_NAME"
SERVICE_DIR="$HOME/.config/systemd/user"
CARGO_RELEASE="$(dirname "$0")/../../linux-receiver/target/release/$BINARY_NAME"

# ── Colours ────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${GREEN}[DualLink]${NC} $*"; }
warn()  { echo -e "${YELLOW}[DualLink]${NC} $*"; }
error() { echo -e "${RED}[DualLink]${NC} $*" >&2; exit 1; }

# ── Uninstall ─────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--uninstall" ]]; then
    info "Stopping and disabling service..."
    systemctl --user stop  "$SERVICE_NAME" 2>/dev/null || true
    systemctl --user disable "$SERVICE_NAME" 2>/dev/null || true
    rm -f "$SERVICE_DIR/$SERVICE_NAME"
    systemctl --user daemon-reload
    info "Removing binary..."
    sudo rm -f "$INSTALL_DIR/$BINARY_NAME"
    info "Uninstall complete."
    exit 0
fi

# ── Build if binary missing ────────────────────────────────────────────────
if [[ ! -f "$CARGO_RELEASE" ]]; then
    info "Binary not found — building release..."
    (cd "$(dirname "$0")/../../linux-receiver" && cargo build --release -p duallink-app)
fi

[[ -f "$CARGO_RELEASE" ]] || error "Build failed — binary not found at $CARGO_RELEASE"

# ── Install binary ─────────────────────────────────────────────────────────
info "Installing binary to $INSTALL_DIR/$BINARY_NAME..."
sudo install -m 755 "$CARGO_RELEASE" "$INSTALL_DIR/$BINARY_NAME"

# ── Install systemd user service ───────────────────────────────────────────
info "Installing systemd user service..."
mkdir -p "$SERVICE_DIR"
cp "$SERVICE_SRC" "$SERVICE_DIR/$SERVICE_NAME"

# Patch XDG_RUNTIME_DIR to current user's UID
UID_CURRENT=$(id -u)
sed -i "s|/run/user/1000|/run/user/$UID_CURRENT|g" "$SERVICE_DIR/$SERVICE_NAME"

systemctl --user daemon-reload
systemctl --user enable "$SERVICE_NAME"
systemctl --user start  "$SERVICE_NAME"

# ── Enable lingering so it survives logout ─────────────────────────────────
loginctl enable-linger "$USER" 2>/dev/null || warn "loginctl not available — service won't auto-start on boot."

# ── Done ───────────────────────────────────────────────────────────────────
info "Installation complete!"
info ""
info "  Status : systemctl --user status $SERVICE_NAME"
info "  Logs   : journalctl --user -u $SERVICE_NAME -f"
info "  Stop   : systemctl --user stop $SERVICE_NAME"
info "  Remove : sudo $0 --uninstall"
