#!/bin/zsh
# build_and_run.sh — Sprint 0.1.4
# Builds PoCVirtualDisplayApp, assembles .app bundle, signs, and runs.
#
# Usage:
#   chmod +x build_and_run.sh && ./build_and_run.sh
#
# Requirements:
#   - macOS 14+, Xcode Command Line Tools
#   - codesign (included with CLT)

setopt errexit

SCRIPT_DIR="${0:A:h}"
cd "$SCRIPT_DIR"

APP_NAME="PoCVirtualDisplayApp"
BUILD_DIR=".build/debug"
BUNDLE_DIR="/tmp/${APP_NAME}.app"
BUNDLE_CONTENTS="$BUNDLE_DIR/Contents"
BUNDLE_MACOS="$BUNDLE_CONTENTS/MacOS"

echo "=== Build & Run: $APP_NAME ==="
echo ""

# ── Step 1: Build ─────────────────────────────────────────────────────────────
echo "[1/4] Building Swift executable..."
swift build 2>&1

BINARY="$SCRIPT_DIR/$BUILD_DIR/$APP_NAME"
if [[ ! -f "$BINARY" ]]; then
    echo "❌ Build failed — binary not found at $BINARY"
    exit 1
fi
echo "[✅] Build complete: $BINARY"
echo ""

# ── Step 2: Assemble .app bundle ──────────────────────────────────────────────
echo "[2/4] Assembling .app bundle at $BUNDLE_DIR..."
rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_MACOS"
cp "$BINARY" "$BUNDLE_MACOS/$APP_NAME"
cp "$SCRIPT_DIR/Info.plist" "$BUNDLE_CONTENTS/Info.plist"
echo "[✅] Bundle assembled"
echo ""

# ── Step 3: Sign (ad-hoc with entitlements) ───────────────────────────────────
echo "[3/4] Signing ad-hoc with entitlements..."
codesign \
    --sign - \
    --entitlements "$SCRIPT_DIR/entitlements.plist" \
    --options runtime \
    --force \
    "$BUNDLE_DIR"

echo "[✅] Signed"
echo "     Effective entitlements:"
codesign -d --entitlements :- "$BUNDLE_DIR" 2>/dev/null | \
    plutil -p - 2>/dev/null || echo "     (could not decode — normal for ad-hoc)"
echo ""

# ── Step 4: Run ───────────────────────────────────────────────────────────────
echo "[4/4] Running $APP_NAME..."
echo "      → Watch System Settings > Displays for a new virtual display"
echo "────────────────────────────────────────────────────────────────────────"
"$BUNDLE_MACOS/$APP_NAME"
echo "────────────────────────────────────────────────────────────────────────"

echo ""
echo "Done. Bundle at: $BUNDLE_DIR"
echo "To manually inspect: open $BUNDLE_DIR"
