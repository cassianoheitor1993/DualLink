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
# Create bundle in /tmp first, then move — avoids Finder/Spotlight adding
# resource forks and extended attributes to the .app while we assemble it.
TMPBUNDLE="$(mktemp -d)/DualLink.app"
TMPCONTENTS="$TMPBUNDLE/Contents"
TMPMACOS="$TMPCONTENTS/MacOS"
TMPRESOURCES="$TMPCONTENTS/Resources"
mkdir -p "$TMPMACOS" "$TMPRESOURCES"

cp ".build/debug/DualLink" "$TMPMACOS/DualLink"
cp "$INFO_PLIST_SRC" "$TMPCONTENTS/Info.plist"

# Minimal PkgInfo
echo -n "APPL????" > "$TMPCONTENTS/PkgInfo"

# Strip ALL extended attributes before signing
xattr -cr "$TMPBUNDLE" 2>/dev/null || true
find "$TMPBUNDLE" -name '._*' -delete 2>/dev/null || true

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
  "$TMPMACOS/DualLink"
codesign --force --sign "$SIGN_ID" \
  --entitlements "$ENTITLEMENTS" \
  --identifier "com.duallink.mac-client" \
  "$TMPBUNDLE"

echo "▶ Verifying signature..."
codesign --verify --deep --strict "$TMPBUNDLE" && echo "  Signature OK"

# Move clean bundle to final location
mv "$TMPBUNDLE" "$APP_DIR"

echo "▶ Launching (foreground — Ctrl+C to stop)..."
exec "$MACOS/DualLink"
