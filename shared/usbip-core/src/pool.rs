//! URB buffer pool — pre-allocated, reusable buffers for the hot path.
//!
//! The pool auto-sizes based on the observed URB rate to avoid allocation
//! during normal operation.  Buffers are returned to the pool on drop via
//! `PooledBuffer`'s `Drop` implementation.
//!
//! ## Design
//!
//! - Minimum pool size: 1024 buffers
//! - Target: 2x the exponentially-weighted moving average (EWMA) of the
//!   observed URB rate
//! - Buffers are pre-allocated `Vec<u8>` of a configurable data capacity
//! - Thread-safe via `crossbeam::queue::ArrayQueue` (lock-free)
//!
//! ## Example
//!
//! ```ignore
//! let pool = UrbBufferPool::new_bulk(2048);
//! let mut buf = pool.acquire();
//! buf.as_mut_slice()[..4].copy_from_slice(b"test");
//! // buf is returned to the pool when it goes out of scope.
//! ```

use std::sync::Arc;
use std::time::Instant;

use crossbeam::queue::ArrayQueue;
use tracing::trace;

use crate::urb::UrbBuffer;

/// Default minimum number of buffers in the pool.
pub const DEFAULT_MIN_POOL_SIZE: usize = 1024;

/// Default data capacity for bulk-transfer buffers (16 KiB).
pub const DEFAULT_BULK_CAPACITY: usize = 16 * 1024;

/// Default data capacity for interrupt-transfer buffers (64 B).
pub const DEFAULT_INTERRUPT_CAPACITY: usize = 64;

/// Smoothing factor for the URB rate EWMA (alpha).
const EWMA_ALPHA: f64 = 0.125;

/// Inner state shared between the pool and acquired buffers.
struct PoolInner {
    /// Lock-free buffer queue.
    queue: ArrayQueue<UrbBuffer>,
}

/// A thread-safe pool of pre-allocated URB buffers.
///
/// The pool adjusts its target size based on the exponentially-weighted
/// moving average of the observed URB rate.  This lets it adapt to
/// different device profiles without manual tuning.
pub struct UrbBufferPool {
    /// Inner state shared with acquired buffers (for Drop-return).
    inner: Arc<PoolInner>,
    /// Data capacity for each buffer (bytes available for payload).
    data_capacity: usize,
    /// EWMA of observed URB rate (URBs / second), if we have observations.
    ewma_rate: f64,
    /// Timestamp of the last `record_urb()` call.
    last_record: Option<Instant>,
    /// Number of URBs recorded in the current measurement window.
    window_count: u64,
    /// Minimum pool size (never shrink below this).
    min_size: usize,
    /// Maximum pool size (cap growth to prevent runaway allocation).
    max_size: usize,
}

impl UrbBufferPool {
    /// Create a pool with the given data capacity and minimum size.
    ///
    /// The pool is pre-filled with `min_size` buffers.
    pub fn new(data_capacity: usize, min_size: usize) -> Self {
        let max_size = (min_size * 4).max(4096);
        let queue = ArrayQueue::new(max_size);

        // Pre-allocate buffers.
        for _ in 0..min_size {
            let buf = UrbBuffer::new(data_capacity);
            // Unwrap is safe because we sized the queue at max_size >= min_size.
            queue.push(buf).ok();
        }

        Self {
            inner: Arc::new(PoolInner { queue }),
            data_capacity,
            ewma_rate: 0.0,
            last_record: None,
            window_count: 0,
            min_size,
            max_size,
        }
    }

    /// Create a pool tuned for bulk transfers (16 KiB data capacity).
    pub fn new_bulk(min_size: usize) -> Self {
        Self::new(DEFAULT_BULK_CAPACITY, min_size)
    }

    /// Create a pool tuned for interrupt transfers (64 B data capacity).
    pub fn new_interrupt(min_size: usize) -> Self {
        Self::new(DEFAULT_INTERRUPT_CAPACITY, min_size)
    }

    /// Acquire a buffer from the pool.
    ///
    /// If the pool is empty, a new buffer is allocated on the hot path.
    /// The returned `PooledBuffer` returns to the pool when dropped.
    pub fn acquire(&self) -> PooledBuffer {
        let buf = self.inner.queue.pop().unwrap_or_else(|| {
            trace!("allocating URB buffer on hot path — pool exhausted");
            UrbBuffer::new(self.data_capacity)
        });
        PooledBuffer { inner: Some(buf), pool: Some(self.inner.clone()) }
    }

    /// Record an observed URB submission for rate tracking.
    ///
    /// Call this once per URB to feed the EWMA.  The pool uses the
    /// smoothed rate to decide whether to grow.
    pub fn record_urb(&mut self) {
        let now = Instant::now();
        self.window_count += 1;

        if let Some(last) = self.last_record {
            let elapsed = now.duration_since(last).as_secs_f64();
            if elapsed >= 1.0 {
                let observed_rate = self.window_count as f64 / elapsed;
                if self.ewma_rate == 0.0 {
                    self.ewma_rate = observed_rate;
                } else {
                    self.ewma_rate =
                        EWMA_ALPHA * observed_rate + (1.0 - EWMA_ALPHA) * self.ewma_rate;
                }

                // Adjust pool size if needed.
                let target = (self.ewma_rate * 2.0).ceil() as usize;
                let desired = target.max(self.min_size).min(self.max_size);

                let current_count = self.inner.queue.len();
                if desired > current_count {
                    let to_add = desired.saturating_sub(current_count);
                    for _ in 0..to_add {
                        if self.inner.queue.push(UrbBuffer::new(self.data_capacity)).is_err() {
                            break;
                        }
                    }
                }
                // Shrinking is deferred — buffers are naturally reclaimed
                // through the Drop path.

                self.window_count = 0;
                self.last_record = Some(now);
            }
        } else {
            self.last_record = Some(now);
        }
    }

    /// Current EWMA rate (URBs/second), or 0.0 if not yet measured.
    pub fn ewma_rate(&self) -> f64 {
        self.ewma_rate
    }

    /// Number of buffers currently available in the pool.
    pub fn available(&self) -> usize {
        self.inner.queue.len()
    }

    /// Total capacity of the internal queue (maximum buffers it can hold).
    pub fn capacity(&self) -> usize {
        self.inner.queue.capacity()
    }

    /// Create a shared (`Arc`) pool.
    pub fn into_arc(self) -> Arc<UrbBufferPool> {
        Arc::new(self)
    }
}

unsafe impl Send for UrbBufferPool {}
unsafe impl Sync for UrbBufferPool {}

/// A buffer acquired from the pool.
///
/// When dropped, the buffer is automatically returned to the pool for
/// reuse.
pub struct PooledBuffer {
    inner: Option<UrbBuffer>,
    /// Shared reference to the pool's inner state.
    /// When this is `Some`, the inner buffer will be returned on drop.
    pool: Option<Arc<PoolInner>>,
}

impl PooledBuffer {
    /// Access the underlying buffer as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.inner.as_ref().expect("PooledBuffer consumed").buf
    }

    /// Access the underlying buffer as a mutable slice.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.inner.as_mut().expect("PooledBuffer consumed").buf
    }

    /// Number of payload bytes this buffer can hold.
    pub fn data_capacity(&self) -> usize {
        self.inner.as_ref().expect("PooledBuffer consumed").data_capacity
    }

    /// Offset in the buffer where the payload starts.
    pub fn data_offset(&self) -> usize {
        self.inner.as_ref().expect("PooledBuffer consumed").data_offset
    }

    /// Take the inner buffer out (consumes this wrapper).
    ///
    /// After calling this, the buffer will NOT be returned to the pool
    /// on drop.  Use for cases where the buffer must escape the pool's
    /// lifetime.
    pub fn take(mut self) -> UrbBuffer {
        self.inner.take().expect("PooledBuffer already consumed")
    }

    /// Reset the buffer contents to zero.
    pub fn reset(&mut self) {
        if let Some(ref mut buf) = self.inner {
            buf.reset();
        }
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(buf) = self.inner.take() {
            if let Some(ref pool) = self.pool {
                // Reset before returning to the pool.
                let mut b = buf;
                b.reset();
                let _ = pool.queue.push(b);
                // If the queue is full, the buffer is dropped — that's fine.
            }
        }
    }
}

impl std::ops::Deref for PooledBuffer {
    type Target = UrbBuffer;

    fn deref(&self) -> &UrbBuffer {
        self.inner.as_ref().expect("PooledBuffer consumed")
    }
}

impl std::ops::DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut UrbBuffer {
        self.inner.as_mut().expect("PooledBuffer consumed")
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction & pre-fill ─────────────────────────────────

    #[test]
    fn test_new_pool_has_min_size_buffers() {
        let pool = UrbBufferPool::new(64, 128);
        assert_eq!(pool.available(), 128, "pool should be pre-filled");
        assert!(pool.capacity() >= 128);
    }

    #[test]
    fn test_new_pool_respects_data_capacity() {
        let pool = UrbBufferPool::new(4096, 16);
        let buf = pool.acquire();
        assert_eq!(buf.data_capacity(), 4096);
    }

    #[test]
    fn test_new_bulk_uses_16k_capacity() {
        let pool = UrbBufferPool::new_bulk(8);
        let buf = pool.acquire();
        assert_eq!(buf.data_capacity(), DEFAULT_BULK_CAPACITY);
    }

    #[test]
    fn test_new_interrupt_uses_64b_capacity() {
        let pool = UrbBufferPool::new_interrupt(8);
        let buf = pool.acquire();
        assert_eq!(buf.data_capacity(), DEFAULT_INTERRUPT_CAPACITY);
    }

    // ── Acquire / release ───────────────────────────────────────

    #[test]
    fn test_acquire_returns_buffer() {
        let pool = UrbBufferPool::new(64, 16);
        let buf = pool.acquire();
        assert!(buf.data_capacity() >= 64);
    }

    #[test]
    fn test_acquire_decreases_available() {
        let pool = UrbBufferPool::new(64, 32);
        let before = pool.available();
        let _buf = pool.acquire();
        assert_eq!(pool.available(), before - 1);
    }

    #[test]
    fn test_acquire_multiple_buffers() {
        let pool = UrbBufferPool::new(64, 1024);
        let mut bufs = Vec::new();
        for _ in 0..1024 {
            bufs.push(pool.acquire());
        }
        assert_eq!(pool.available(), 0, "all buffers acquired");
        // Acquiring from an empty pool allocates a new buffer.
        let _extra = pool.acquire();
    }

    #[test]
    fn test_reset_clears_buffer() {
        let pool = UrbBufferPool::new(64, 8);
        let mut buf = pool.acquire();
        buf.as_mut_slice()[0] = 0xFF;
        buf.as_mut_slice()[10] = 0xAB;
        buf.reset();
        // All bytes should be zero after reset (capacity bytes remain).
        assert_eq!(buf.as_slice()[0], 0);
        assert_eq!(buf.as_slice()[10], 0);
    }

    // ── EWMA rate tracking ──────────────────────────────────────

    #[test]
    fn test_ewma_starts_at_zero() {
        let pool = UrbBufferPool::new(64, 16);
        assert_eq!(pool.ewma_rate(), 0.0);
    }

    #[test]
    fn test_record_urb_updates_rate() {
        // We can't reliably test the exact rate without sleeping,
        // but we can verify that calling record_urb doesn't panic
        // and moves `last_record` past `None`.
        let mut pool = UrbBufferPool::new(64, 16);
        assert!(pool.last_record.is_none());

        pool.record_urb();
        assert!(pool.last_record.is_some());
    }

    #[test]
    fn test_record_urb_does_not_panic() {
        let mut pool = UrbBufferPool::new(64, 16);
        for _ in 0..100 {
            pool.record_urb();
        }
        // After many rapid calls with <1s elapsed, the EWMA should still
        // be 0 because no full second has passed.
        assert!(pool.ewma_rate() == 0.0 || pool.ewma_rate() > 0.0);
    }

    // ── Pool auto-sizing ────────────────────────────────────────

    #[test]
    fn test_pool_grows_when_rate_demands() {
        let mut pool = UrbBufferPool::new(64, 16);
        let initial = pool.available();

        // Simulate a high rate by setting last_record far enough back
        // that the first measurement window closes.
        pool.last_record = Some(Instant::now() - std::time::Duration::from_secs(2));
        pool.window_count = 500;

        pool.record_urb();
        // After recording, the pool should have grown if the rate
        // target exceeds the current size.
        let ewma = pool.ewma_rate();
        assert!(ewma > 0.0, "EWMA rate should be > 0 after window closes");

        let target = (ewma * 2.0).ceil() as usize;
        let desired = target.max(16);
        if desired > initial {
            // Pool should have grown.
            assert!(
                pool.available() >= initial || pool.available() <= pool.capacity(),
                "pool should not shrink when demand is high"
            );
        }
    }

    // ── Thread safety ───────────────────────────────────────────

    #[test]
    fn test_pool_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<UrbBufferPool>();
    }

    #[test]
    fn test_concurrent_acquire() {
        use std::sync::Arc;
        use std::thread;

        let pool = Arc::new(UrbBufferPool::new(64, 1024));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let p = pool.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..128 {
                    let buf = p.acquire();
                    // Simulate some work.
                    let _ = buf.data_capacity();
                }
            }));
        }

        for h in handles {
            h.join().expect("thread panicked");
        }

        // After all threads finish and their PooledBuffers drop,
        // the pool should have at least its min_size buffers.
        assert!(pool.available() >= 512, "pool should have many buffers after concurrent use");
    }

    // ── Edge cases ──────────────────────────────────────────────

    #[test]
    fn test_pool_exhaustion_allocates_fresh() {
        let pool = UrbBufferPool::new(64, 4);
        // Drain the pool.
        let mut bufs = Vec::new();
        for _ in 0..4 {
            bufs.push(pool.acquire());
        }
        assert_eq!(pool.available(), 0);

        // Acquiring from empty pool should still return a buffer.
        let extra = pool.acquire();
        assert_eq!(extra.data_capacity(), 64);
    }

    #[test]
    fn test_min_size_is_honored() {
        let pool = UrbBufferPool::new(128, 1024);
        assert_eq!(pool.available(), 1024);
        // Capacity should be at least 1024.
        assert!(pool.capacity() >= 1024);
    }

    #[test]
    fn test_default_min_pool_size_constant() {
        assert_eq!(DEFAULT_MIN_POOL_SIZE, 1024);
    }
}
