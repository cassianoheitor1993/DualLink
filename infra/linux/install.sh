#!/usr/bin/env bash
# DualLink Linux Receiver — install script
# Usage:  sudo ./install.sh [--uninstall]
set -euo pipefail

BINARY_NAME="duallink-receiver"
INSTALL_DIR="/usr/local/bin"
SERVICE_NAME="duallink-receiver.service"
SERVICE_SRC="$(dirname "$0")/$SERVICE_NAME"

# When invoked via sudo, operate on the real user's home/systemd session
REAL_USER="${SUDO_USER:-$USER}"
REAL_HOME=$(getent passwd "$REAL_USER" | cut -d: -f6)
SERVICE_DIR="$REAL_HOME/.config/systemd/user"
CARGO_RELEASE="$(dirname "$0")/../../linux-receiver/target/release/$BINARY_NAME"

# Helper: run a command as the real (non-root) user
run_as_user() { sudo -u "$REAL_USER" XDG_RUNTIME_DIR="/run/user/$(id -u "$REAL_USER")" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$(id -u "$REAL_USER")/bus" "$@"; }

# ── Colours ────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${GREEN}[DualLink]${NC} $*"; }
warn()  { echo -e "${YELLOW}[DualLink]${NC} $*"; }
error() { echo -e "${RED}[DualLink]${NC} $*" >&2; exit 1; }

# ── Uninstall ─────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--uninstall" ]]; then
    info "Stopping and disabling service..."
    run_as_user systemctl --user stop  "$SERVICE_NAME" 2>/dev/null || true
    run_as_user systemctl --user disable "$SERVICE_NAME" 2>/dev/null || true
    rm -f "$SERVICE_DIR/$SERVICE_NAME"
    run_as_user systemctl --user daemon-reload
    info "Removing binary..."
    rm -f "$INSTALL_DIR/$BINARY_NAME"
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
install -m 755 "$CARGO_RELEASE" "$INSTALL_DIR/$BINARY_NAME"

# ── Install systemd user service ───────────────────────────────────────────
info "Installing systemd user service..."
mkdir -p "$SERVICE_DIR"
cp "$SERVICE_SRC" "$SERVICE_DIR/$SERVICE_NAME"
chown "$REAL_USER:" "$SERVICE_DIR/$SERVICE_NAME"

# Patch XDG_RUNTIME_DIR to the real user's UID
REAL_UID=$(id -u "$REAL_USER")
sed -i "s|/run/user/1000|/run/user/$REAL_UID|g" "$SERVICE_DIR/$SERVICE_NAME"

run_as_user systemctl --user daemon-reload
run_as_user systemctl --user enable "$SERVICE_NAME"
run_as_user systemctl --user start  "$SERVICE_NAME"

# ── Enable lingering so it survives logout ─────────────────────────────────
loginctl enable-linger "$REAL_USER" 2>/dev/null || warn "loginctl not available — service won't auto-start on boot."

# ── Done ───────────────────────────────────────────────────────────────────
info "Installation complete!"
info ""
info "  Status : systemctl --user status $SERVICE_NAME"
info "  Logs   : journalctl --user -u $SERVICE_NAME -f"
info "  Stop   : systemctl --user stop $SERVICE_NAME"
info "  Remove : sudo $0 --uninstall"
