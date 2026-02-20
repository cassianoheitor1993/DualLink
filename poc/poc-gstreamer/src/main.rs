// Sprint 0.3 — GStreamer H.264 Decode Latency Benchmark
//
// Measures per-frame decode latency for each available hardware accelerator.
// Pipeline: videotestsrc → x264enc → tee → {decoder → appsink} (one at a time)
//
// Run: cargo run --release
// Output: per-decoder avg/p50/p99 latency + fps + verdict

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::prelude::*;
use gstreamer_app::AppSink;

const FRAMES: u32 = 300;
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FPS: u32 = 30;
const LATENCY_TARGET_MS: f64 = 20.0; // DualLink Wi-Fi budget leaves ~20ms for decode

// ── Decoder descriptors ───────────────────────────────────────────────────────

struct DecoderDesc {
    name: &'static str,
    element: &'static str,
    tier: &'static str,
}

static DECODERS: &[DecoderDesc] = &[
    DecoderDesc {
        name: "NVIDIA nvh264dec (HW)",
        element: "nvh264dec",
        tier: "primary",
    },
    DecoderDesc {
        name: "NVIDIA nvdec (legacy)",
        element: "nvdec",
        tier: "primary",
    },
    DecoderDesc {
        name: "VA-API vaapidecodebin (HW)",
        element: "vaapidecodebin",
        tier: "fallback",
    },
    DecoderDesc {
        name: "VA-API vaapih264dec (HW)",
        element: "vaapih264dec",
        tier: "fallback",
    },
    DecoderDesc {
        name: "Software avdec_h264 (CPU)",
        element: "avdec_h264",
        tier: "last_resort",
    },
];

// ── Result types ─────────────────────────────────────────────────────────────

#[derive(Debug)]
struct BenchResult {
    name: String,
    element: String,
    tier: String,
    available: bool,
    frames_decoded: u32,
    elapsed_ms: u64,
    avg_fps: f64,
    avg_frame_ms: f64,
    p50_ms: f64,
    p99_ms: f64,
    meets_target: bool,
}

// ── Check element availability ────────────────────────────────────────────────

fn has_element(name: &str) -> bool {
    gst::ElementFactory::find(name).is_some()
}

// ── Build benchmark pipeline ─────────────────────────────────────────────────
//
// Pipeline: videotestsrc → capsfilter → x264enc → h264parse → DECODER → appsink
//
// appsink records arrival time; PTS set by videotestsrc starts at 0 and increments
// by 1/FPS per frame. We compare wallclock delta with expected PTS delta to compute
// the pipeline's end-to-end processing latency (encode + decode).
// For a decode-only estimate: subtract the known x264enc encode time (measured separately).

fn run_benchmark(desc: &DecoderDesc) -> Result<BenchResult> {
    let name = desc.name.to_string();
    let element = desc.element.to_string();
    let tier = desc.tier.to_string();

    // Build pipeline string
    let pipeline_str = format!(
        "videotestsrc num-buffers={frames} \
         ! video/x-raw,width={w},height={h},framerate={fps}/1 \
         ! x264enc tune=zerolatency speed-preset=superfast key-int-max=30 bitrate=8000 \
         ! h264parse \
         ! {decoder} \
         ! videoconvert \
         ! appsink name=mysink max-buffers=10 drop=false sync=false",
        frames = FRAMES,
        w = WIDTH,
        h = HEIGHT,
        fps = FPS,
        decoder = desc.element,
    );

    let pipeline = gst::parse::launch(&pipeline_str)
        .context(format!("Failed to build pipeline for {}", desc.element))?
        .dynamic_cast::<gst::Pipeline>()
        .map_err(|_| anyhow::anyhow!("Not a pipeline"))?;

    // Grab appsink
    let appsink = pipeline
        .by_name("mysink")
        .context("No appsink named 'mysink'")?
        .dynamic_cast::<AppSink>()
        .map_err(|_| anyhow::anyhow!("Not an appsink"))?;

    // Shared state: per-frame arrival timestamps
    let frame_times: Arc<Mutex<Vec<Duration>>> = Arc::new(Mutex::new(Vec::with_capacity(FRAMES as usize)));
    let frame_times_clone = Arc::clone(&frame_times);
    let start_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let start_time_clone = Arc::clone(&start_time);

    // Frame count for detecting first frame
    let frame_count: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let frame_count_clone = Arc::clone(&frame_count);

    // Install new-sample callback
    appsink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let _sample = sink.pull_sample().map_err(|_| gst::FlowError::Error)?;

                let mut start = start_time_clone.lock().unwrap();
                let mut count = frame_count_clone.lock().unwrap();

                if start.is_none() {
                    *start = Some(Instant::now());
                }

                *count += 1;
                let elapsed = start.unwrap().elapsed();
                frame_times_clone.lock().unwrap().push(elapsed);

                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    // Run pipeline
    let wall_start = Instant::now();
    pipeline.set_state(gst::State::Playing)?;

    // Wait for EOS or error
    let bus = pipeline.bus().context("No bus")?;
    loop {
        if let Some(msg) = bus.timed_pop(gst::ClockTime::from_seconds(30)) {
            match msg.view() {
                gst::MessageView::Eos(_) => break,
                gst::MessageView::Error(err) => {
                    pipeline.set_state(gst::State::Null)?;
                    return Err(anyhow::anyhow!(
                        "Pipeline error: {} — {:?}",
                        err.error(),
                        err.debug()
                    ));
                }
                _ => {}
            }
        } else {
            // Timeout
            pipeline.set_state(gst::State::Null)?;
            return Err(anyhow::anyhow!("Pipeline timed out after 30s"));
        }
    }

    pipeline.set_state(gst::State::Null)?;
    let elapsed_ms = wall_start.elapsed().as_millis() as u64;

    // Calculate stats from per-frame timestamps
    let times = frame_times.lock().unwrap();
    let frames_decoded = times.len() as u32;
    if frames_decoded == 0 {
        return Err(anyhow::anyhow!("No frames were decoded"));
    }

    // Inter-frame durations (time between consecutive decoded frames)
    let mut frame_durations: Vec<f64> = times
        .windows(2)
        .map(|w| (w[1].as_micros() as f64 - w[0].as_micros() as f64) / 1000.0)
        .collect();
    frame_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let avg_frame_ms = if frame_durations.is_empty() {
        elapsed_ms as f64 / frames_decoded as f64
    } else {
        frame_durations.iter().sum::<f64>() / frame_durations.len() as f64
    };

    let p50_ms = percentile(&frame_durations, 50.0);
    let p99_ms = percentile(&frame_durations, 99.0);
    let avg_fps = frames_decoded as f64 / (elapsed_ms as f64 / 1000.0);

    Ok(BenchResult {
        name,
        element,
        tier,
        available: true,
        frames_decoded,
        elapsed_ms,
        avg_fps,
        avg_frame_ms,
        p50_ms,
        p99_ms,
        meets_target: avg_frame_ms <= LATENCY_TARGET_MS,
    })
}

fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((pct / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    gst::init().context("Failed to initialize GStreamer")?;

    println!("=== DualLink Sprint 0.3 — GStreamer H.264 Decode Benchmark ===");
    println!();
    println!(
        "Config: {}x{} @ {}fps, {} frames per decoder",
        WIDTH, HEIGHT, FPS, FRAMES
    );
    println!("Target: avg frame time < {}ms (Wi-Fi decode budget)", LATENCY_TARGET_MS);
    println!();

    // Probe available decoders
    println!("[1/3] Probing decoder availability...");
    println!();
    let available: Vec<&DecoderDesc> = DECODERS
        .iter()
        .filter(|d| {
            let found = has_element(d.element);
            let icon = if found { "✅" } else { "❌" };
            println!("  {} {:<40} ({})", icon, d.name, d.element);
            found
        })
        .collect();

    println!();
    if available.is_empty() {
        eprintln!("No decoders available! Run ./setup.sh first.");
        return Ok(());
    }

    // Check that x264enc is available for encoding test stream
    if !has_element("x264enc") {
        eprintln!("❌ x264enc not found — cannot generate test stream.");
        eprintln!("   Install: sudo apt install gstreamer1.0-plugins-ugly");
        return Ok(());
    }

    // Run benchmarks
    println!("[2/3] Running benchmarks ({} frames each)...", FRAMES);
    println!();

    let mut results: Vec<BenchResult> = Vec::new();

    for desc in &available {
        print!("  Benchmarking {} ... ", desc.name);
        std::io::Write::flush(&mut std::io::stdout()).ok();

        match run_benchmark(desc) {
            Ok(result) => {
                let verdict = if result.meets_target { "✅" } else { "⚠️" };
                println!(
                    "{} avg={:.1}ms  p50={:.1}ms  p99={:.1}ms  fps={:.1}",
                    verdict, result.avg_frame_ms, result.p50_ms, result.p99_ms, result.avg_fps
                );
                results.push(result);
            }
            Err(e) => {
                println!("❌ FAILED: {}", e);
                results.push(BenchResult {
                    name: desc.name.to_string(),
                    element: desc.element.to_string(),
                    tier: desc.tier.to_string(),
                    available: true,
                    frames_decoded: 0,
                    elapsed_ms: 0,
                    avg_fps: 0.0,
                    avg_frame_ms: f64::MAX,
                    p50_ms: 0.0,
                    p99_ms: 0.0,
                    meets_target: false,
                });
            }
        }
    }

    println!();

    // Print summary table
    println!("[3/3] Results Summary");
    println!();
    println!(
        "  {:<40} {:>8}  {:>8}  {:>8}  {:>8}  {:>6}",
        "Decoder", "avg(ms)", "p50(ms)", "p99(ms)", "fps", "target"
    );
    println!("  {}", "-".repeat(82));

    for r in &results {
        if r.frames_decoded == 0 {
            println!(
                "  {:<40} {:>8}  {:>8}  {:>8}  {:>8}  {:>6}",
                r.name, "FAIL", "—", "—", "—", "❌"
            );
        } else {
            let target = if r.meets_target { "✅" } else { "⚠️ SLOW" };
            println!(
                "  {:<40} {:>8.1}  {:>8.1}  {:>8.1}  {:>8.1}  {:>6}",
                r.name, r.avg_frame_ms, r.p50_ms, r.p99_ms, r.avg_fps, target
            );
        }
    }

    println!();

    // Recommend decoder priority
    println!("Recommended decoder priority for duallink-decoder:");
    println!();

    let hw_primary: Vec<&BenchResult> = results
        .iter()
        .filter(|r| r.tier == "primary" && r.frames_decoded > 0)
        .collect();
    let hw_fallback: Vec<&BenchResult> = results
        .iter()
        .filter(|r| r.tier == "fallback" && r.frames_decoded > 0)
        .collect();
    let sw: Vec<&BenchResult> = results
        .iter()
        .filter(|r| r.tier == "last_resort" && r.frames_decoded > 0)
        .collect();

    let mut priority = 1;
    for r in &hw_primary {
        println!("  {}. PRIMARY: {}  (avg {:.1}ms)", priority, r.element, r.avg_frame_ms);
        priority += 1;
    }
    for r in &hw_fallback {
        println!("  {}. FALLBACK: {}  (avg {:.1}ms)", priority, r.element, r.avg_frame_ms);
        priority += 1;
    }
    for r in &sw {
        println!("  {}. LAST RESORT: {}  (avg {:.1}ms)", priority, r.element, r.avg_frame_ms);
    }

    println!();
    println!("→ Copy results to: ../../.github/instructions/golden-tips/linux.instructions.md");
    println!("→ Insert as GT-2001 with decoder priority order for duallink-decoder");

    Ok(())
}
