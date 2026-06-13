//! USB/IP server performance benchmark.
//!
//! Measures URB buffer pool throughput and batch submission latency.
//! Outputs JSON for CI parsing.
//!
//! ## CI Integration
//!
//! The CI perf gate reads the JSON output and fails if p99 > 5ms.

use std::time::{Duration, Instant};

use clap::Parser;

#[derive(Parser)]
#[command(name = "usbip-bench")]
#[command(about = "USB/IP performance benchmark")]
struct Cli {
    /// Number of iterations (default: 10000)
    #[arg(short, long, default_value_t = 10_000)]
    iterations: u32,

    /// Output format: json or text
    #[arg(short, long, default_value = "json")]
    format: String,
}

#[derive(serde::Serialize)]
struct BenchmarkResult {
    name: String,
    iterations: u32,
    avg_us: f64,
    p50_us: f64,
    p95_us: f64,
    p99_us: f64,
    max_us: f64,
    total_time_s: f64,
}

fn main() {
    let cli = Cli::parse();

    let results = vec![
        bench_pool_acquire_release(cli.iterations, 64),
        bench_pool_acquire_release(cli.iterations, 16 * 1024),
        bench_batcher_push_flush(cli.iterations),
    ];

    match cli.format.as_str() {
        "json" => {
            let output = serde_json::json!({
                "benchmarks": results,
                "p99_pass": results.iter().all(|r| r.p99_us < 5_000.0),
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        },
        _ => {
            for r in &results {
                println!(
                    "{}: avg={:.1}us p50={:.1}us p95={:.1}us p99={:.1}us max={:.1}us ({:.3}s total)",
                    r.name, r.avg_us, r.p50_us, r.p95_us, r.p99_us, r.max_us, r.total_time_s,
                );
            }
        },
    }
}

fn compute_stats(
    samples: &mut [f64],
    name: &str,
    iterations: u32,
    total_time: Duration,
) -> BenchmarkResult {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = samples.len();
    let sum: f64 = samples.iter().sum();
    let avg = sum / n as f64;
    let p50 = samples[(n as f64 * 0.50) as usize];
    let p95 = samples[(n as f64 * 0.95) as usize];
    let p99 = samples[(n as f64 * 0.99) as usize];
    let max = samples[n - 1];

    BenchmarkResult {
        name: name.to_string(),
        iterations,
        avg_us: avg,
        p50_us: p50,
        p95_us: p95,
        p99_us: p99,
        max_us: max,
        total_time_s: total_time.as_secs_f64(),
    }
}

fn bench_pool_acquire_release(iterations: u32, capacity: usize) -> BenchmarkResult {
    use usbip_core::UrbBufferPool;

    let pool = UrbBufferPool::new(capacity, 1024);
    let mut samples = Vec::with_capacity(iterations as usize);

    let start = Instant::now();
    for _ in 0..iterations {
        let t0 = Instant::now();
        let _buf = pool.acquire();
        samples.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
    }
    let total = start.elapsed();

    compute_stats(
        &mut samples,
        &format!("pool_acquire_{}_byte", capacity),
        iterations,
        total,
    )
}

fn bench_batcher_push_flush(iterations: u32) -> BenchmarkResult {
    use usbip_core::protocol::{U32BE, URB_DIR_IN};
    use usbip_core::urb::UsbIpCmdSubmit;
    use usbip_server::UrbBatcher;
    use usbip_server::UrbResult;

    let mut batcher = UrbBatcher::new();
    let mut samples = Vec::with_capacity(iterations as usize);

    let cmd = UsbIpCmdSubmit {
        seqnum: U32BE::new(1),
        devid: U32BE::new(1),
        direction: U32BE::new(1),
        ep: U32BE::new(0x81),
        transfer_flags: U32BE::new(URB_DIR_IN),
        transfer_buffer_length: U32BE::new(64),
        start_frame: U32BE::new(0),
        number_of_packets: U32BE::new(0),
        interval: U32BE::new(0),
        setup: [0u8; 8],
    };
    let result = UrbResult { status: 0, actual_length: 64, data: vec![0xAB; 64] };

    let mut i = 0u32;
    let start = Instant::now();
    while i < iterations {
        let t0 = Instant::now();
        batcher.push(&cmd, &result);
        if i % 8 == 7 {
            batcher.flush();
        }
        samples.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
        i += 1;
    }
    let total = start.elapsed();

    compute_stats(&mut samples, "batcher_push_flush", iterations, total)
}
