#!/bin/bash
# setup.sh — Install GStreamer + VA-API + NVDEC plugins
# Suporta: Ubuntu 22.04+, Debian 12+, Fedora 38+, Arch Linux
#
# Usage: ./setup.sh

set -e

echo "=== DualLink GStreamer Setup ==="
echo ""

# ── Detect distro ──────────────────────────────────────────────────────────────
if command -v apt-get &>/dev/null; then
    DISTRO="debian"
elif command -v dnf &>/dev/null; then
    DISTRO="fedora"
elif command -v pacman &>/dev/null; then
    DISTRO="arch"
else
    echo "❌ Unsupported distro. Install manually:"
    echo "   GStreamer 1.22+ with good + bad + ugly + libav + va plugins"
    exit 1
fi

echo "Distro: $DISTRO"
echo ""

# ── Install ────────────────────────────────────────────────────────────────────
case "$DISTRO" in
debian)
    echo "[1/2] Installing GStreamer packages (Ubuntu/Debian)..."
    sudo apt-get update -qq
    sudo apt-get install -y \
        gstreamer1.0-tools \
        gstreamer1.0-plugins-base \
        gstreamer1.0-plugins-good \
        gstreamer1.0-plugins-bad \
        gstreamer1.0-plugins-ugly \
        gstreamer1.0-libav \
        gstreamer1.0-vaapi \
        libgstreamer1.0-dev \
        libgstreamer-plugins-base1.0-dev \
        libgstreamer-plugins-bad1.0-dev \
        libglib2.0-dev \
        pkg-config \
        build-essential \
        vainfo \
        libva-drm2 \
        libva-x11-2

    # NVDEC: requires gst-plugins-bad with cuda support
    # On Ubuntu 22.04+ with NVIDIA driver 520+ and CUDA toolkit:
    if command -v nvidia-smi &>/dev/null; then
        echo "[1b] NVIDIA GPU detected — checking nvdec availability..."
        if gst-inspect-1.0 nvdec &>/dev/null; then
            echo "     ✅ nvdec already available"
        else
            echo "     ⚠️  nvdec not found. Requires:"
            echo "        1. NVIDIA driver 520+ (check: nvidia-smi)"
            echo "        2. CUDA toolkit 11.8+"
            echo "        3. gstreamer1.0-plugins-bad compiled with --enable-nvcodec"
            echo "        Alternative: sudo apt install nvidia-gstreamer (if available in your repo)"
        fi
    fi
    ;;

fedora)
    echo "[1/2] Installing GStreamer packages (Fedora)..."
    sudo dnf install -y \
        gstreamer1 \
        gstreamer1-plugins-base \
        gstreamer1-plugins-good \
        gstreamer1-plugins-bad-free \
        gstreamer1-plugins-bad-freeworld \
        gstreamer1-plugins-ugly \
        gstreamer1-libav \
        gstreamer1-vaapi \
        gstreamer1-devel \
        gstreamer1-plugins-base-devel \
        libva-utils \
        pkg-config

    if command -v nvidia-smi &>/dev/null; then
        echo "[1b] NVIDIA GPU detected — install nvdec via:"
        echo "     sudo dnf install cuda-gstreamer-plugins (from RPM Fusion non-free)"
    fi
    ;;

arch)
    echo "[1/2] Installing GStreamer packages (Arch Linux)..."
    sudo pacman -S --needed --noconfirm \
        gstreamer \
        gst-plugins-base \
        gst-plugins-good \
        gst-plugins-bad \
        gst-plugins-ugly \
        gst-libav \
        gstreamer-vaapi \
        libva-utils \
        base-devel

    if command -v nvidia-smi &>/dev/null; then
        echo "[1b] NVIDIA GPU detected — install nvdec via AUR: cuda-gstreamer"
        echo "     yay -S gst-plugins-bad-cuda  # or similar"
    fi
    ;;
esac

# ── Verify Rust toolchain ──────────────────────────────────────────────────────
echo ""
echo "[2/2] Checking Rust toolchain..."
if command -v cargo &>/dev/null; then
    echo "     ✅ $(cargo --version)"
else
    echo "     Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    echo "     ✅ Rust installed"
fi

# ── Summary ────────────────────────────────────────────────────────────────────
echo ""
echo "=== Setup complete ==="
echo ""
echo "Available GStreamer plugins:"
gst-inspect-1.0 --print-all 2>/dev/null | grep -E "nvdec|vaapidecodebin|avdec_h264|nvh264dec" | head -10 || true

echo ""
echo "VA-API info:"
vainfo 2>/dev/null | head -10 || echo "  vainfo not available or no VA-API device"

echo ""
echo "Next: ./probe.sh   — quick CLI benchmark"
echo "Next: cargo run --release   — Rust latency benchmark"
