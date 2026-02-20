#!/usr/bin/env bash
# run_app.sh — Build DualLink, wrap as .app bundle, codesign, launch.
# Required by CGVirtualDisplay (GT-1005): needs a proper .app with bundle ID.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_DIR="$SCRIPT_DIR/DualLink.app"
CONTENTS="$APP_DIR/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"
INFO_PLIST_SRC="$SCRIPT_DIR/Sources/DualLinkApp/Resources/Info.plist"
ENTITLEMENTS="$SCRIPT_DIR/Sources/DualLinkApp/Resources/adhoc.entitlements"

echo "▶ Building..."
cd "$SCRIPT_DIR"
swift build -c debug

echo "▶ Creating .app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$MACOS" "$RESOURCES"

cp ".build/debug/DualLink" "$MACOS/DualLink"
cp "$INFO_PLIST_SRC" "$CONTENTS/Info.plist"

# Minimal PkgInfo
echo -n "APPL????" > "$CONTENTS/PkgInfo"

echo "▶ Removing quarantine..."
xattr -cr "$APP_DIR" 2>/dev/null || true

echo "▶ Codesigning (ad-hoc)..."
# Sign binary first, then wrap the bundle
codesign --force --sign - \
  --identifier "com.duallink.mac-client" \
  "$MACOS/DualLink"
codesign --force --sign - \
  --entitlements "$ENTITLEMENTS" \
  --identifier "com.duallink.mac-client" \
  "$APP_DIR"

echo "▶ Verifying signature..."
codesign --verify --deep --strict "$APP_DIR" && echo "  Signature OK"

echo "▶ Launching (foreground — Ctrl+C to stop)..."
exec "$MACOS/DualLink"
