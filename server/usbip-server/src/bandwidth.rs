//! Per-client bandwidth throttling via a token-bucket algorithm.
//!
//! Each client connection gets its own [`TokenBucket`] that tracks how many
//! bytes it may send per second.  When a client exceeds its allocated rate
//! the bucket forces an async sleep before the next URB response is written.

use std::time::{Duration, Instant};

/// Configuration that describes a bandwidth cap.
///
/// `bytes_per_sec == 0` means *unlimited* (no throttling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BandwidthLimit {
    /// Maximum bytes per second (0 = unlimited).
    pub bytes_per_sec: u64,
}

impl BandwidthLimit {
    /// Unlimited bandwidth cap — everything passes immediately.
    pub const fn unlimited() -> Self {
        Self { bytes_per_sec: 0 }
    }

    /// Returns `true` when the limit is zero (i.e. disabled).
    pub const fn is_unlimited(&self) -> bool {
        self.bytes_per_sec == 0
    }

    /// Convert to an optional token bucket (returns `None` for unlimited).
    pub fn into_bucket(self) -> Option<TokenBucket> {
        if self.is_unlimited() {
            None
        } else {
            Some(TokenBucket::new(self.bytes_per_sec))
        }
    }
}

impl Default for BandwidthLimit {
    fn default() -> Self {
        Self::unlimited()
    }
}

// ── Token bucket ─────────────────────────────────────────────────────

/// A token-bucket rate limiter suitable for bandwidth throttling.
///
/// Tokens represent *bytes* that may be sent.  The bucket refills at
/// `rate` bytes/second up to a burst capacity equal to `rate` (one
/// second's worth of tokens).
///
/// # Example
///
/// ```ignore
/// let mut bucket = TokenBucket::new(1024);           // 1 kB/s
/// bucket.throttle(512).await;                         // spends ~0.5 s
/// ```
#[derive(Debug)]
pub struct TokenBucket {
    /// Current token count (fractional).
    tokens: f64,
    /// Capacity of the bucket (one second's worth of tokens).
    max_tokens: f64,
    /// Refill rate in tokens per nanosecond.
    refill_per_ns: f64,
    /// Wall-clock of the last [`refill`] call.
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket that allows `bytes_per_sec`.
    ///
    /// The bucket starts full so the first write is not penalised.
    ///
    /// Passing `0` creates a degenerate bucket where all calls to
    /// [`consume`] return `None` (unlimited).
    pub fn new(bytes_per_sec: u64) -> Self {
        if bytes_per_sec == 0 {
            // Degenerate — unlimited.
            return Self {
                tokens: 0.0,
                max_tokens: 0.0,
                refill_per_ns: 0.0,
                last_refill: Instant::now(),
            };
        }
        let rate = bytes_per_sec as f64;
        Self {
            tokens: rate,
            max_tokens: rate,
            refill_per_ns: rate / 1_000_000_000.0,
            last_refill: Instant::now(),
        }
    }

    /// Refill the bucket based on elapsed time since the last call.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed_ns = now.duration_since(self.last_refill).as_nanos() as f64;
        if elapsed_ns > 0.0 {
            self.tokens = (self.tokens + elapsed_ns * self.refill_per_ns).min(self.max_tokens);
            self.last_refill = now;
        }
    }

    /// Consume `amount` tokens and return the delay the caller should
    /// wait before proceeding.
    ///
    /// * Returns `Duration::ZERO` when enough tokens are available.
    /// * Returns a positive `Duration` when the caller must sleep to
    ///   let the bucket refill.
    /// * Returns `None` when the limit is zero (unlimited).
    #[must_use]
    pub fn consume(&mut self, amount: u64) -> Option<Duration> {
        if self.max_tokens == 0.0 {
            return None; // unlimited
        }
        self.refill();

        let amount = amount as f64;
        if self.tokens >= amount {
            self.tokens -= amount;
            Some(Duration::ZERO)
        } else {
            let deficit = amount - self.tokens;
            self.tokens = 0.0;
            let delay_ns = (deficit / self.refill_per_ns) as u64;
            Some(Duration::from_nanos(delay_ns))
        }
    }

    /// Async version of [`consume`] — sleeps for the required delay.
    ///
    /// Returns immediately when there is no bandwidth limit.
    pub async fn throttle(&mut self, amount: u64) {
        if let Some(delay) = self.consume(amount) {
            if delay > Duration::ZERO {
                tokio::time::sleep(delay).await;
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // ── BandwidthLimit ───────────────────────────────────────────────

    #[test]
    fn unlimited_by_default() {
        let lim = BandwidthLimit::unlimited();
        assert!(lim.is_unlimited());
        assert_eq!(lim.bytes_per_sec, 0);
    }

    #[test]
    fn limited_is_not_unlimited() {
        let lim = BandwidthLimit { bytes_per_sec: 10_000 };
        assert!(!lim.is_unlimited());
    }

    #[test]
    fn into_bucket_returns_none_for_unlimited() {
        assert!(BandwidthLimit::unlimited().into_bucket().is_none());
    }

    #[test]
    fn into_bucket_returns_some_for_limited() {
        assert!(BandwidthLimit { bytes_per_sec: 1000 }.into_bucket().is_some());
    }

    // ── TokenBucket — basic ──────────────────────────────────────────

    #[test]
    fn consume_small_amount_returns_zero_delay() {
        let mut bucket = TokenBucket::new(1_000_000); // 1 MB/s
        let delay = bucket.consume(100).unwrap();
        assert_eq!(delay, Duration::ZERO);
    }

    #[test]
    fn consume_exact_bucket_full() {
        let mut bucket = TokenBucket::new(1000); // 1000 B/s
        let delay = bucket.consume(1000).unwrap();
        assert_eq!(delay, Duration::ZERO);
    }

    #[test]
    fn consume_over_limit_returns_nonzero_delay() {
        let mut bucket = TokenBucket::new(1000); // 1000 B/s
        let delay = bucket.consume(2000).unwrap();
        assert!(delay > Duration::ZERO);
        // Should need about 1 second to refill 1000 tokens
        assert!(delay >= Duration::from_millis(900));
        assert!(delay <= Duration::from_millis(1100));
    }

    #[test]
    fn consume_multiple_accumulates_debt() {
        let mut bucket = TokenBucket::new(1000); // 1000 B/s
        // First 1000 is free (bucket starts full)
        assert_eq!(bucket.consume(1000).unwrap(), Duration::ZERO);
        // Second 1000 should need ~1 second
        let delay = bucket.consume(1000).unwrap();
        assert!(delay >= Duration::from_millis(900));
    }

    #[test]
    fn bucket_refills_over_time() {
        let mut bucket = TokenBucket::new(1000);
        // Drain the bucket
        assert_eq!(bucket.consume(1000).unwrap(), Duration::ZERO);
        // Simulate time passing by forcing a long-ago last_refill
        bucket.last_refill = Instant::now() - Duration::from_secs(2);
        bucket.refill();
        // Should have ~2000 tokens now (capped at max_tokens = 1000)
        assert!((bucket.tokens - 1000.0).abs() < 0.001);
        // Consume again — should be instant
        assert_eq!(bucket.consume(1000).unwrap(), Duration::ZERO);
    }

    #[test]
    fn consume_exact_after_partial_refill() {
        let mut bucket = TokenBucket::new(1000);
        bucket.consume(500).unwrap(); // spend half
        assert!((bucket.tokens - 500.0).abs() < 0.001);
    }

    #[test]
    fn negative_or_zero_bytes_ok() {
        let mut bucket = TokenBucket::new(1000);
        assert_eq!(bucket.consume(0).unwrap(), Duration::ZERO);
    }

    #[test]
    fn high_bandwidth_low_delay() {
        let mut bucket = TokenBucket::new(1_000_000_000); // 1 GB/s
        let delay = bucket.consume(65_536).unwrap(); // 64 KB
        assert_eq!(delay, Duration::ZERO);
    }

    #[test]
    fn low_bandwidth_high_delay() {
        let mut bucket = TokenBucket::new(1); // 1 B/s
        let delay = bucket.consume(10).unwrap();
        assert!(delay >= Duration::from_secs(9));
        assert!(delay <= Duration::from_secs(11));
    }

    // ── Async throttle ───────────────────────────────────────────────

    #[tokio::test]
    async fn throttle_free_returns_immediately() {
        let mut bucket = TokenBucket::new(1_000_000);
        let before = Instant::now();
        bucket.throttle(100).await;
        assert!(before.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn throttle_delays_when_exceeded() {
        let mut bucket = TokenBucket::new(100); // 100 B/s
        // Spend all tokens
        bucket.throttle(100).await;
        // Trying to send another 100 should sleep ~1 s
        let before = Instant::now();
        bucket.throttle(100).await;
        let elapsed = before.elapsed();
        assert!(elapsed >= Duration::from_millis(800));
        assert!(elapsed <= Duration::from_secs(2));
    }

    #[tokio::test]
    async fn throttle_no_bucket_no_delay() {
        // unlimited scenario — consume returns None
        let mut bucket = TokenBucket::new(0);
        let before = Instant::now();
        // consume returns None for unlimited (bytes_per_sec = 0)
        assert!(bucket.consume(10_000).is_none());
        assert!(before.elapsed() < Duration::from_millis(10));
    }
}
