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

echo "▶ Removing quarantine and resource forks..."
xattr -cr "$APP_DIR" 2>/dev/null || true
find "$APP_DIR" -name '._*' -delete 2>/dev/null || true
find "$APP_DIR" -name "*.DS_Store" -delete 2>/dev/null || true
dot_clean -m "$APP_DIR" 2>/dev/null || true

# Use persistent cert if available (keeps TCC permissions across rebuilds)
CERT_NAME="DualLink Dev"
if security find-certificate -c "$CERT_NAME" ~/Library/Keychains/login.keychain-db &>/dev/null; then
    SIGN_ID="$CERT_NAME"
else
    SIGN_ID="-"
fi
echo "▶ Codesigning (identity: $SIGN_ID)..."
codesign --force --sign "$SIGN_ID" \
  --identifier "com.duallink.mac-client" \
  "$MACOS/DualLink"
codesign --force --sign "$SIGN_ID" \
  --entitlements "$ENTITLEMENTS" \
  --identifier "com.duallink.mac-client" \
  "$APP_DIR"

echo "▶ Verifying signature..."
codesign --verify --deep --strict "$APP_DIR" && echo "  Signature OK"

echo "▶ Launching (foreground — Ctrl+C to stop)..."
exec "$MACOS/DualLink"
