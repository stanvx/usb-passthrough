//! Reconnect state machine with exponential backoff for network resilience.
//!
//! ## State Machine
//!
//! ```text
//! ┌──────────┐   permanent/fatal error
//! │  Active   │ ──────────────────────────► ┌──────────┐
//! │(forwarding)│                              │  Stopped  │
//! └─────┬────┘                              └──────────┘
//!       │ transient error
//!       ▼
//! ┌──────────┐   delay exhausted
//! │ BackOff  │ ──────────────────────────► ┌──────────┐
//! │ (nth try)│                              │  Active   │
//! └──────────┘                              │(retrying) │
//!                                           └──────────┘
//! ```
//!
//! Distinguishes transient failures (TCP RST, timeout, connection closed)
//! from permanent failures (auth rejected, device not found, protocol error)
//! using [`ErrorCategory`] already defined in `usbip-core`.

use std::time::Duration;

use rand::Rng;
use tracing::warn;
use usbip_core::error::{ErrorCategory, UsbIpError, UsbIpResult};

/// Configuration for automatic reconnection with exponential backoff.
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Maximum number of retry attempts.
    ///
    /// `None` = unlimited retries (default).
    /// `Some(0)` = no retry, fail immediately on first transient error.
    pub max_retries: Option<u32>,
    /// Initial backoff delay in seconds (applied on first retry).
    pub initial_delay_secs: u64,
    /// Maximum backoff delay in seconds (cap).
    pub max_delay_secs: u64,
    /// Backoff multiplier applied each attempt (default: 2.0).
    pub backoff_multiplier: f64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_retries: None, // unlimited
            initial_delay_secs: 1,
            max_delay_secs: 30,
            backoff_multiplier: 2.0,
        }
    }
}

impl ReconnectConfig {
    /// Create a config that disables reconnection entirely.
    pub fn no_retry() -> Self {
        Self { max_retries: Some(0), ..Default::default() }
    }

    /// Create a config with a fixed maximum number of retries.
    pub fn with_max_retries(max: u32) -> Self {
        Self { max_retries: Some(max), ..Default::default() }
    }

    /// Calculate the delay for the nth retry attempt (0-indexed).
    ///
    /// Produces exponential backoff:
    ///   attempt 0 → 1s, attempt 1 → 2s, attempt 2 → 4s, attempt 3 → 8s,
    ///   attempt 4 → 16s, attempt 5+ → 30s (capped)
    ///
    /// Each delay includes ±25% random jitter to avoid thundering-herd.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let raw_secs =
            self.initial_delay_secs as f64 * self.backoff_multiplier.powi(attempt as i32);
        let capped_secs = raw_secs.min(self.max_delay_secs as f64);

        // Add ±25% jitter
        let jitter_range = capped_secs * 0.25;
        let jitter = rand::thread_rng().gen_range(-jitter_range..jitter_range);
        let total = (capped_secs + jitter).max(0.1);

        Duration::from_secs_f64(total)
    }

    /// Returns `true` if another retry is allowed after `attempt` failures.
    pub fn should_retry(&self, attempt: u32) -> bool {
        match self.max_retries {
            None => true, // unlimited
            Some(max) => attempt < max,
        }
    }
}

/// State of the reconnect state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectState {
    /// Actively forwarding URBs or establishing initial connection.
    Active,
    /// Waiting with exponential backoff before next retry.
    BackOff { attempt: u32, next_delay: Duration },
    /// Retries exhausted or permanent error — not reconnecting.
    Stopped,
}

impl ReconnectState {
    /// Advance the state machine given an error and configuration.
    ///
    /// Returns the new state and the recommended delay (if backing off).
    pub fn on_error(self, error: &UsbIpError, config: &ReconnectConfig) -> Self {
        match error.category() {
            ErrorCategory::Transient => {
                let attempt = match self {
                    ReconnectState::Active | ReconnectState::Stopped => 0,
                    ReconnectState::BackOff { attempt, .. } => attempt + 1,
                };

                if config.should_retry(attempt) {
                    let delay = config.delay_for_attempt(attempt);
                    warn!(
                        attempt,
                        delay_ms = delay.as_millis(),
                        error = %error,
                        "transient error, backing off",
                    );
                    ReconnectState::BackOff { attempt, next_delay: delay }
                } else {
                    warn!(attempt, max_retries = ?config.max_retries, "retry limit reached");
                    ReconnectState::Stopped
                }
            },
            // Permanent and Fatal errors stop the state machine immediately.
            ErrorCategory::Permanent | ErrorCategory::Fatal => {
                warn!(category = %error.category(), error = %error, "permanent/fatal error, stopping reconnect");
                ReconnectState::Stopped
            },
        }
    }
}

/// Classify whether a result should trigger a reconnect retry.
///
/// Returns the delay to wait before retrying, or `None` if the error is
/// permanent/fatal or retries are exhausted.
pub fn classify_and_backoff(
    result: &UsbIpResult<()>,
    state: &mut ReconnectState,
    config: &ReconnectConfig,
) -> Option<Duration> {
    match result {
        Ok(()) => {
            // Clean shutdown — not an error, no reconnect.
            *state = ReconnectState::Stopped;
            None
        },
        Err(e) => {
            let new_state = state.clone().on_error(e, config);
            match new_state {
                ReconnectState::BackOff { next_delay, .. } => {
                    *state = new_state;
                    Some(next_delay)
                },
                _ => {
                    *state = new_state;
                    None
                },
            }
        },
    }
}

/// Reconnect outcome: either we should retry after a delay, or stop.
#[derive(Debug)]
pub enum ReconnectDecision {
    /// Retry after the specified delay.
    RetryAfter(Duration),
    /// Stop retrying (permanent error or limit exhausted).
    Stop,
}

/// Evaluate an operation result and the current state to decide next action.
pub fn decide_reconnect(
    result: &UsbIpResult<()>,
    state: &mut ReconnectState,
    config: &ReconnectConfig,
) -> ReconnectDecision {
    match result {
        Ok(()) => ReconnectDecision::Stop,
        Err(e) => {
            let new_state = state.clone().on_error(e, config);
            match new_state {
                ReconnectState::BackOff { next_delay, .. } => {
                    *state = new_state;
                    ReconnectDecision::RetryAfter(next_delay)
                },
                _ => {
                    *state = new_state;
                    ReconnectDecision::Stop
                },
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use usbip_core::error::ErrorKind;

    // ── ReconnectConfig defaults ─────────────────────────────

    #[test]
    fn test_default_config_unlimited_retries() {
        let config = ReconnectConfig::default();
        assert_eq!(config.max_retries, None);
        assert!(config.should_retry(0));
        assert!(config.should_retry(100));
    }

    #[test]
    fn test_no_retry_config() {
        let config = ReconnectConfig::no_retry();
        assert_eq!(config.max_retries, Some(0));
        assert!(!config.should_retry(0));
    }

    #[test]
    fn test_with_max_retries() {
        let config = ReconnectConfig::with_max_retries(3);
        assert_eq!(config.max_retries, Some(3));
        assert!(config.should_retry(0));
        assert!(config.should_retry(2));
        assert!(!config.should_retry(3));
        assert!(!config.should_retry(5));
    }

    // ── Exponential backoff timing ───────────────────────────

    #[test]
    fn test_delay_for_attempt_increases_exponentially() {
        let config = ReconnectConfig::default();
        // Series must be strictly increasing until cap
        let d0 = config.delay_for_attempt(0).as_secs_f64();
        let d1 = config.delay_for_attempt(1).as_secs_f64();
        let d2 = config.delay_for_attempt(2).as_secs_f64();
        let d3 = config.delay_for_attempt(3).as_secs_f64();
        let d4 = config.delay_for_attempt(4).as_secs_f64();

        assert!(d0 < d1, "attempt 0 delay ({d0}) < attempt 1 ({d1})");
        assert!(d1 < d2, "attempt 1 delay ({d1}) < attempt 2 ({d2})");
        assert!(d2 < d3, "attempt 2 delay ({d2}) < attempt 3 ({d3})");
        assert!(d3 < d4, "attempt 3 delay ({d3}) < attempt 4 ({d4})");
    }

    #[test]
    fn test_delay_caps_at_max_delay_secs() {
        let config = ReconnectConfig::default();
        // After enough attempts, delay should be at the cap (30s max jittered)
        let d_high = config.delay_for_attempt(10).as_secs_f64();
        assert!(d_high <= 31.0, "high attempt delay ({d_high}s) should be near 30s cap");
        assert!(d_high >= 0.1, "delay should be positive");
    }

    #[test]
    fn test_delay_minimum_positive() {
        let config = ReconnectConfig::default();
        for attempt in 0..20 {
            let delay = config.delay_for_attempt(attempt);
            assert!(
                delay.as_secs_f64() >= 0.1,
                "delay for attempt {attempt} must be at least 0.1s"
            );
        }
    }

    #[test]
    fn test_delay_with_jitter_is_within_range() {
        let config = ReconnectConfig::default();
        // Check multiple attempts to verify jitter doesn't exceed bounds
        for attempt in 0..5 {
            let delay = config.delay_for_attempt(attempt);
            let base =
                config.initial_delay_secs as f64 * config.backoff_multiplier.powi(attempt as i32);
            let capped = base.min(config.max_delay_secs as f64);
            let min_expected = (capped * 0.75).max(0.1);
            let max_expected = capped * 1.25;

            assert!(
                delay.as_secs_f64() >= min_expected,
                "attempt {attempt}: delay {:.3}s >= {:.3}s (75% of {capped})",
                delay.as_secs_f64(),
                min_expected,
            );
            assert!(
                delay.as_secs_f64() <= max_expected,
                "attempt {attempt}: delay {:.3}s <= {:.3}s (125% of {capped})",
                delay.as_secs_f64(),
                max_expected,
            );
        }
    }

    // ── State machine transitions ────────────────────────────

    fn make_error(category: ErrorCategory) -> UsbIpError {
        let kind = match category {
            ErrorCategory::Transient => ErrorKind::ConnectionClosed,
            ErrorCategory::Permanent => ErrorKind::DeviceNotFound("test".into()),
            ErrorCategory::Fatal => ErrorKind::UrbFailed(0),
        };
        UsbIpError::new(kind, category)
    }

    #[test]
    fn test_state_starts_active() {
        let config = ReconnectConfig::default();
        let state = ReconnectState::Active;
        let err = make_error(ErrorCategory::Transient);
        let new = state.on_error(&err, &config);
        assert!(matches!(new, ReconnectState::BackOff { attempt: 0, .. }));
    }

    #[test]
    fn test_transient_error_transitions_to_backoff() {
        let config = ReconnectConfig::default();
        let state = ReconnectState::Active;
        let err = make_error(ErrorCategory::Transient);
        let new = state.on_error(&err, &config);
        assert!(matches!(new, ReconnectState::BackOff { attempt: 0, .. }));
    }

    #[test]
    fn test_permanent_error_transitions_to_stopped() {
        let config = ReconnectConfig::default();
        let state = ReconnectState::Active;
        let err = make_error(ErrorCategory::Permanent);
        let new = state.on_error(&err, &config);
        assert_eq!(new, ReconnectState::Stopped);
    }

    #[test]
    fn test_fatal_error_transitions_to_stopped() {
        let config = ReconnectConfig::default();
        let state = ReconnectState::Active;
        let err = make_error(ErrorCategory::Fatal);
        let new = state.on_error(&err, &config);
        assert_eq!(new, ReconnectState::Stopped);
    }

    #[test]
    fn test_backoff_increments_attempt() {
        let config = ReconnectConfig::default();
        let state = ReconnectState::BackOff { attempt: 0, next_delay: Duration::from_secs(1) };
        let err = make_error(ErrorCategory::Transient);
        let new = state.on_error(&err, &config);
        assert!(matches!(new, ReconnectState::BackOff { attempt: 1, .. }));
    }

    #[test]
    fn test_backoff_second_transient_increments_again() {
        let config = ReconnectConfig::default();
        let state = ReconnectState::BackOff { attempt: 2, next_delay: Duration::from_secs(4) };
        let err = make_error(ErrorCategory::Transient);
        let new = state.on_error(&err, &config);
        assert!(matches!(new, ReconnectState::BackOff { attempt: 3, .. }));
    }

    #[test]
    fn test_max_retries_exhausted() {
        let config = ReconnectConfig::with_max_retries(2);
        let state = ReconnectState::BackOff { attempt: 1, next_delay: Duration::from_secs(2) };
        let err = make_error(ErrorCategory::Transient);
        let new = state.on_error(&err, &config);
        // attempt 1 + 1 = 2, which is not < 2, so → Stopped
        assert_eq!(new, ReconnectState::Stopped);
    }

    #[test]
    fn test_limited_retries_allows_until_limit() {
        let config = ReconnectConfig::with_max_retries(3);
        let state = ReconnectState::BackOff { attempt: 2, next_delay: Duration::from_secs(4) };
        let err = make_error(ErrorCategory::Transient);
        let new = state.on_error(&err, &config);
        // attempt 2 + 1 = 3, which is not < 3, so → Stopped
        assert_eq!(new, ReconnectState::Stopped);
    }

    // ── classify_and_backoff helper ──────────────────────────

    #[test]
    fn test_classify_ok_stops() {
        let config = ReconnectConfig::default();
        let mut state = ReconnectState::Active;
        let result: UsbIpResult<()> = Ok(());
        let decision = classify_and_backoff(&result, &mut state, &config);
        assert!(decision.is_none());
        assert_eq!(state, ReconnectState::Stopped);
    }

    #[test]
    fn test_classify_transient_returns_delay() {
        let config = ReconnectConfig::default();
        let mut state = ReconnectState::Active;
        let result: UsbIpResult<()> = Err(make_error(ErrorCategory::Transient));
        let decision = classify_and_backoff(&result, &mut state, &config);
        assert!(decision.is_some());
        assert!(matches!(state, ReconnectState::BackOff { attempt: 0, .. }));
    }

    #[test]
    fn test_classify_permanent_returns_none() {
        let config = ReconnectConfig::default();
        let mut state = ReconnectState::Active;
        let result: UsbIpResult<()> = Err(make_error(ErrorCategory::Permanent));
        let decision = classify_and_backoff(&result, &mut state, &config);
        assert!(decision.is_none());
        assert_eq!(state, ReconnectState::Stopped);
    }

    // ── decide_reconnect helper ──────────────────────────────

    #[test]
    fn test_decide_reconnect_ok_stops() {
        let config = ReconnectConfig::default();
        let mut state = ReconnectState::Active;
        let result: UsbIpResult<()> = Ok(());
        match decide_reconnect(&result, &mut state, &config) {
            ReconnectDecision::Stop => {},
            _ => panic!("expected Stop"),
        }
    }

    #[test]
    fn test_decide_reconnect_transient_retries() {
        let config = ReconnectConfig::default();
        let mut state = ReconnectState::Active;
        let result: UsbIpResult<()> = Err(make_error(ErrorCategory::Transient));
        match decide_reconnect(&result, &mut state, &config) {
            ReconnectDecision::RetryAfter(_) => {},
            _ => panic!("expected RetryAfter"),
        }
    }

    #[test]
    fn test_decide_reconnect_permanent_stops() {
        let config = ReconnectConfig::default();
        let mut state = ReconnectState::Active;
        let result: UsbIpResult<()> = Err(make_error(ErrorCategory::Permanent));
        match decide_reconnect(&result, &mut state, &config) {
            ReconnectDecision::Stop => {},
            _ => panic!("expected Stop"),
        }
    }
}
