#!/usr/bin/env bash
# DualLink Linux Receiver — install script
# Usage:  sudo ./install.sh [--uninstall]
set -euo pipefail

BINARY_NAME="duallink-receiver"
GUI_BINARY_NAME="duallink-gui"
INSTALL_DIR="/usr/local/bin"
SERVICE_NAME="duallink-receiver.service"
SERVICE_SRC="$(dirname "$0")/$SERVICE_NAME"
DESKTOP_SRC="$(dirname "$0")/duallink-receiver.desktop"
ICON_SRC="$(dirname "$0")/duallink-receiver.svg"

# When invoked via sudo, operate on the real user's home/systemd session
REAL_USER="${SUDO_USER:-$USER}"
REAL_HOME=$(getent passwd "$REAL_USER" | cut -d: -f6)
SERVICE_DIR="$REAL_HOME/.config/systemd/user"
CARGO_RELEASE="$(dirname "$0")/../../linux-receiver/target/release/$BINARY_NAME"
GUI_CARGO_RELEASE="$(dirname "$0")/../../linux-receiver/target/release/$GUI_BINARY_NAME"

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
    rm -f "$INSTALL_DIR/$GUI_BINARY_NAME"
    info "Removing desktop entry and icon..."
    rm -f "$REAL_HOME/.local/share/applications/duallink-receiver.desktop"
    rm -f "$REAL_HOME/.local/share/icons/hicolor/scalable/apps/duallink-receiver.svg"
    run_as_user update-desktop-database "$REAL_HOME/.local/share/applications" 2>/dev/null || true
    info "Uninstall complete."
    exit 0
fi

# ── Build if binary missing ────────────────────────────────────────────────
if [[ ! -f "$CARGO_RELEASE" ]] || [[ ! -f "$GUI_CARGO_RELEASE" ]]; then
    info "Building release binaries (duallink-receiver + duallink-gui)..."
    (cd "$(dirname "$0")/../../linux-receiver" && cargo build --release -p duallink-app -p duallink-gui)
fi

[[ -f "$CARGO_RELEASE"     ]] || error "Build failed — $CARGO_RELEASE not found"
[[ -f "$GUI_CARGO_RELEASE" ]] || error "Build failed — $GUI_CARGO_RELEASE not found"

# ── Install binaries ────────────────────────────────────────────────────────
info "Installing binaries to $INSTALL_DIR/..."
install -m 755 "$CARGO_RELEASE"     "$INSTALL_DIR/$BINARY_NAME"
install -m 755 "$GUI_CARGO_RELEASE" "$INSTALL_DIR/$GUI_BINARY_NAME"

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

# ── Install desktop entry + icon ───────────────────────────────────────────
info "Installing app icon and desktop entry..."
ICON_DIR="$REAL_HOME/.local/share/icons/hicolor/scalable/apps"
APPS_DIR="$REAL_HOME/.local/share/applications"
mkdir -p "$ICON_DIR" "$APPS_DIR"
cp "$ICON_SRC" "$ICON_DIR/duallink-receiver.svg"
cp "$DESKTOP_SRC" "$APPS_DIR/duallink-receiver.desktop"
chown -R "$REAL_USER:" "$REAL_HOME/.local/share/icons/hicolor" "$APPS_DIR/duallink-receiver.desktop"
run_as_user update-desktop-database "$APPS_DIR" 2>/dev/null || true
run_as_user gtk-update-icon-cache -f -t "$REAL_HOME/.local/share/icons/hicolor" 2>/dev/null || true

# ── Done ───────────────────────────────────────────────────────────────────
info "Installation complete!"
info ""
info "  GUI app: duallink-gui  (launch from app menu or run directly)"
info "  CLI svc: systemctl --user status $SERVICE_NAME"
info "  Logs   : journalctl --user -u $SERVICE_NAME -f"
info "  Stop   : systemctl --user stop $SERVICE_NAME"
info "  Remove : sudo $0 --uninstall"
info ""
info "  NOTE: The GUI and the systemd service both bind UDP:7878 + TCP:7879."
info "        Stop the service before opening the GUI, or vice-versa."
