#!/usr/bin/env bash
# install.sh — Build DualLink in release mode and install to /Applications.
# Run once; afterwards just double-click /Applications/DualLink.app to launch.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_NAME="DualLink.app"
BUILD_APP="$SCRIPT_DIR/$APP_NAME"
INSTALL_DEST="/Applications/$APP_NAME"
ENTITLEMENTS="$SCRIPT_DIR/Sources/DualLinkApp/Resources/adhoc.entitlements"
INFO_PLIST_SRC="$SCRIPT_DIR/Sources/DualLinkApp/Resources/Info.plist"

echo "▶  Building DualLink (release)..."
cd "$SCRIPT_DIR"
swift build -c release

echo "▶  Assembling .app bundle..."
rm -rf "$BUILD_APP"
mkdir -p "$BUILD_APP/Contents/MacOS" "$BUILD_APP/Contents/Resources"

cp ".build/release/DualLink" "$BUILD_APP/Contents/MacOS/DualLink"
cp "$INFO_PLIST_SRC"          "$BUILD_APP/Contents/Info.plist"
printf "APPL????"            > "$BUILD_APP/Contents/PkgInfo"

# Optional: copy app icon if it exists
ICON="$SCRIPT_DIR/Sources/DualLinkApp/Resources/AppIcon.icns"
[[ -f "$ICON" ]] && cp "$ICON" "$BUILD_APP/Contents/Resources/AppIcon.icns"

echo "▶  Removing quarantine..."
xattr -cr "$BUILD_APP" 2>/dev/null || true

echo "▶  Codesigning (ad-hoc)..."
codesign --force --sign - \
  --identifier "com.duallink.mac-client" \
  "$BUILD_APP/Contents/MacOS/DualLink"
codesign --force --sign - \
  --entitlements "$ENTITLEMENTS" \
  --identifier "com.duallink.mac-client" \
  "$BUILD_APP"
codesign --verify --deep --strict "$BUILD_APP" && echo "   Signature OK"

echo "▶  Installing to /Applications..."
# Remove old version if present
rm -rf "$INSTALL_DEST"
cp -R "$BUILD_APP" "$INSTALL_DEST"
xattr -cr "$INSTALL_DEST" 2>/dev/null || true

echo ""
echo "✅  DualLink installed → $INSTALL_DEST"
echo "   Open it from Launchpad, Spotlight, or:"
echo "   open /Applications/DualLink.app"
