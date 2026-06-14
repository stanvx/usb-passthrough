//! Benchmarks for the URB buffer pool — acquire/release throughput and
//! auto-tuning response under varying loads.
//!
//! Run with: cargo bench -p usbip-core

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use usbip_core::pool::UrbBufferPool;

fn bench_acquire_release_single(c: &mut Criterion) {
    let pool = UrbBufferPool::new_bulk(1024);

    c.bench_function("pool/acquire_release_single", |b| {
        b.iter(|| {
            let buf = pool.acquire();
            black_box(buf.data_capacity());
        })
    });
}

fn bench_acquire_release_sequential(c: &mut Criterion) {
    let pool = UrbBufferPool::new_bulk(1024);

    c.bench_function("pool/acquire_release_sequential_1000", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let buf = pool.acquire();
                black_box(buf.data_capacity());
            }
        })
    });
}

fn bench_pool_exhaustion(c: &mut Criterion) {
    // Tiny pool — forces fresh allocation on every acquire
    let pool = UrbBufferPool::new(64, 1);

    c.bench_function("pool/exhaustion_fresh_alloc", |b| {
        b.iter(|| {
            let buf = pool.acquire();
            black_box(buf.data_capacity());
        })
    });
}

fn bench_ewma_rate_tracking(c: &mut Criterion) {
    let mut pool = UrbBufferPool::new(64, 512);

    c.bench_function("pool/ewma_rate_tracking", |b| {
        b.iter(|| {
            for _ in 0..100 {
                pool.record_urb();
            }
            black_box(pool.ewma_rate());
        })
    });
}

fn bench_auto_tune_growth(c: &mut Criterion) {
    let mut group = c.benchmark_group("pool/auto_tune");
    group.sample_size(10);

    let mut pool = UrbBufferPool::new(64, 16);
    // Seed the pool with historical data so the EWMA window closes quickly.
    pool.record_urb();

    group.bench_function("growth_under_load", |b| {
        b.iter(|| {
            // Simulate high-rate URB stream: 1000 URBs/sec
            for _ in 0..200 {
                let buf = pool.acquire();
                black_box(buf.data_capacity());
            }
            // Record a batch to trigger auto-sizing
            for _ in 0..10 {
                pool.record_urb();
            }
            black_box(pool.available());
        })
    });

    group.finish();
}

fn bench_concurrent_acquire(c: &mut Criterion) {
    let pool = UrbBufferPool::new_bulk(1024);

    c.bench_function("pool/concurrent_acquire_8_threads", |b| {
        b.iter(|| {
            let pool_ref = &pool;
            std::thread::scope(|s| {
                for _ in 0..8 {
                    s.spawn(|| {
                        for _ in 0..64 {
                            let buf = pool_ref.acquire();
                            black_box(buf.data_capacity());
                        }
                    });
                }
            });
        })
    });
}

fn bench_buffer_reset(c: &mut Criterion) {
    let pool = UrbBufferPool::new_bulk(1024);

    c.bench_function("pool/buffer_reset", |b| {
        b.iter(|| {
            let mut buf = pool.acquire();
            buf.reset();
            black_box(buf.data_capacity());
        })
    });
}

criterion_group! {
    name = pool_benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(3))
        .warm_up_time(Duration::from_secs(1))
        .sample_size(50);
    targets =
        bench_acquire_release_single,
        bench_acquire_release_sequential,
        bench_pool_exhaustion,
        bench_ewma_rate_tracking,
        bench_auto_tune_growth,
        bench_concurrent_acquire,
        bench_buffer_reset,
}

criterion_main!(pool_benches);
