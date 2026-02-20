#!/bin/bash
# probe.sh — Quick GStreamer decoder probe + benchmark
# Uses gst-launch-1.0 to test H.264 decode with available hardware accelerators.
#
# Does NOT require compiling Rust — uses gst-launch-1.0 directly.
#
# Usage: ./probe.sh [--no-display]
#   --no-display   Run headless (fakesink instead of autovideosink)
#
# Tip: Run as ./probe.sh 2>&1 | tee probe-results.txt

set -e

NO_DISPLAY=false
for arg in "$@"; do
    [[ "$arg" == "--no-display" ]] && NO_DISPLAY=true
done

SINK="autovideosink sync=false"
if $NO_DISPLAY || [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
    SINK="fakesink sync=false"
    echo "(headless mode — using fakesink)"
fi

FRAMES=300  # ~10s @ 30fps
WIDTH=1920
HEIGHT=1080
FPS=30

echo "=== GStreamer Decode PoC ==="
echo ""

# ── Check GStreamer version ────────────────────────────────────────────────────
GST_VERSION=$(gst-launch-1.0 --version 2>/dev/null | head -1)
echo "GStreamer: $GST_VERSION"
echo "Benchmark: ${FRAMES} frames @ ${WIDTH}x${HEIGHT} ${FPS}fps"
echo ""

# ── Helper: check if a GStreamer element exists ────────────────────────────────
has_element() {
    gst-inspect-1.0 "$1" &>/dev/null
}

# ── Helper: benchmark a decoder ───────────────────────────────────────────────
# Args: $1=decoder_name $2=decoder_pipeline_fragment $3=extra_src_flags
benchmark_decoder() {
    local NAME="$1"
    local DECODER_PIPE="$2"
    local EXTRA_FLAGS="${3:-}"

    # Synthetic pipeline:
    #   videotestsrc → capsfilter → x264enc (low-latency) → h264parse → DECODER → $SINK
    # We use videotestsrc as source to avoid needing a file, and x264enc to produce
    # real H.264 bitstream (not all decoders accept raw videotestsrc data).
    #
    # num-buffers controls how many frames to encode/decode.
    # x264enc tune=zerolatency key-int-max=30 ensures rapid keyframes for testing.
    
    local PIPELINE="videotestsrc num-buffers=${FRAMES} ${EXTRA_FLAGS} \
        ! video/x-raw,width=${WIDTH},height=${HEIGHT},framerate=${FPS}/1 \
        ! x264enc tune=zerolatency speed-preset=superfast key-int-max=30 bitrate=8000 \
        ! h264parse \
        ! ${DECODER_PIPE} \
        ! videoconvert \
        ! ${SINK}"

    # Run and capture stderr for stats (gst-launch prints to stderr)
    local START_NS
    START_NS=$(date +%s%N)
    
    if GST_DEBUG=fpsdisplaysink:4 gst-launch-1.0 -e $PIPELINE 2>&1 \
        | grep -v "^Setting pipeline" \
        | tail -5 > /tmp/gst_output_${NAME}.txt; then
        
        local END_NS
        END_NS=$(date +%s%N)
        local ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))
        local AVG_FPS
        AVG_FPS=$(echo "scale=1; ${FRAMES} * 1000 / ${ELAPSED_MS}" | bc 2>/dev/null || echo "?")
        local AVG_LATENCY_MS
        AVG_LATENCY_MS=$(echo "scale=1; ${ELAPSED_MS} / ${FRAMES}" | bc 2>/dev/null || echo "?")
        
        echo "  ✅ ${NAME}"
        echo "       elapsed=${ELAPSED_MS}ms  avg_fps=${AVG_FPS}  avg_frame_time=${AVG_LATENCY_MS}ms"
    else
        echo "  ❌ ${NAME} — pipeline failed"
        echo "       (element may not support this format or is unavailable)"
    fi
}

# ── Step 1: Check available decoders ──────────────────────────────────────────
echo "[1/4] Checking decoder availability..."
echo ""

HAS_NVDEC=false
HAS_NVDEC_NEW=false   # nvh264dec (newer GStreamer)
HAS_VAAPI=false
HAS_VAAPI_DEC=false   # individual vaapih264dec
HAS_SW=false

has_element "nvdec"           && HAS_NVDEC=true       && echo "  ✅ nvdec           — NVIDIA hardware (legacy)"
has_element "nvh264dec"       && HAS_NVDEC_NEW=true   && echo "  ✅ nvh264dec       — NVIDIA hardware (GStreamer 1.18+)"
has_element "vaapidecodebin"  && HAS_VAAPI=true        && echo "  ✅ vaapidecodebin  — VA-API hardware decode"
has_element "vaapih264dec"    && HAS_VAAPI_DEC=true    && echo "  ✅ vaapih264dec    — VA-API H.264 specific"
has_element "avdec_h264"      && HAS_SW=true           && echo "  ✅ avdec_h264      — Software (libavcodec)"
has_element "x264enc" || {
    echo ""
    echo "❌ x264enc not found — cannot encode test stream"
    echo "   Install: sudo apt install gstreamer1.0-plugins-ugly"
    exit 1
}

echo ""
echo "[2/4] Running decode benchmarks (${FRAMES} frames each)..."
echo "      Note: Lower avg_frame_time = better decode performance"
echo ""

# ── Step 2: Benchmark each decoder ────────────────────────────────────────────
RESULTS=""

if $HAS_NVDEC_NEW; then
    echo "--- nvh264dec (NVIDIA) ---"
    benchmark_decoder "nvh264dec" "nvh264dec"
    echo ""
fi

if $HAS_NVDEC; then
    echo "--- nvdec (NVIDIA legacy) ---"
    benchmark_decoder "nvdec" "nvdec"
    echo ""
fi

if $HAS_VAAPI; then
    echo "--- vaapidecodebin (VA-API auto) ---"
    benchmark_decoder "vaapidecodebin" "vaapidecodebin"
    echo ""
fi

if $HAS_VAAPI_DEC; then
    echo "--- vaapih264dec (VA-API H.264) ---"
    benchmark_decoder "vaapih264dec" "vaapih264dec"
    echo ""
fi

if $HAS_SW; then
    echo "--- avdec_h264 (software baseline) ---"
    benchmark_decoder "avdec_h264" "avdec_h264"
    echo ""
fi

# ── Step 3: VA-API device info ─────────────────────────────────────────────────
echo "[3/4] VA-API device info..."
if command -v vainfo &>/dev/null; then
    vainfo 2>&1 | grep -E "VA-API|vainfo|libva|Driver|VAProfile" | head -15
else
    echo "  vainfo not installed (run: sudo apt install vainfo)"
fi
echo ""

# ── Step 4: NVIDIA info ────────────────────────────────────────────────────────
echo "[4/4] NVIDIA GPU info..."
if command -v nvidia-smi &>/dev/null; then
    nvidia-smi --query-gpu=name,driver_version,memory.total --format=csv,noheader 2>/dev/null || true
    echo ""
    # Check NVDEC capability
    if nvidia-smi --query-gpu=encoder.stats.sessionCount,decoder.utilization.gpu \
        --format=csv,noheader 2>/dev/null | head -1; then
        echo "  (NVDEC monitoring available)"
    fi
else
    echo "  nvidia-smi not available"
fi
echo ""

# ── Summary ────────────────────────────────────────────────────────────────────
echo "=== Summary ==="
echo ""
echo "Target latency budget: < 20ms decode (from 80ms total Wi-Fi budget)"
echo ""
echo "Recommendations for duallink-decoder:"

if $HAS_NVDEC_NEW || $HAS_NVDEC; then
    echo "  1. PRIMARY:  nvh264dec / nvdec   — NVIDIA hardware"
fi
if $HAS_VAAPI || $HAS_VAAPI_DEC; then
    echo "  2. FALLBACK: vaapidecodebin       — VA-API (vendor-neutral)"
fi
if $HAS_SW; then
    echo "  3. LAST RESORT: avdec_h264        — Software"
fi

echo ""
echo "Copy these results to: ../linux.instructions.md (golden tips)"
echo ""
echo "Next: cargo run --release   — Rust benchmark with per-frame latency"
