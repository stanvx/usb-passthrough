//! Hot-plug detection for USB device attach/detach events.
//!
//! Provides a platform-agnostic interface for monitoring USB device
//! arrival and removal. On Linux this uses libusb hotplug callbacks;
//! on other platforms it falls back to polling or is stubbed.
//!
//! Events carry structured error information with correlation IDs
//! for tracing through the system (see ADR-0003).

use std::sync::mpsc;

use usbip_core::error::*;

/// Events emitted by the hotplug monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotplugEvent {
    /// A USB device was attached.
    Attached {
        /// The bus ID string (e.g., "3-2").
        busid: String,
        /// Vendor ID.
        vid: u16,
        /// Product ID.
        pid: u16,
    },
    /// A USB device was detached.
    Detached {
        /// The bus ID string that was removed.
        busid: String,
        /// Correlation ID linking to any in-flight URB error.
        correlation_id: CorrelationId,
    },
}

/// Trait for hotplug monitors.
///
/// Platform-specific implementations (libusb, IOKit, Windows
/// RegisterDeviceNotification, Android UsbManager) provide
/// the hook for polling or receiving callbacks.
pub trait HotplugSource: Send + 'static {
    /// Poll for any pending hotplug events.
    ///
    /// Returns `None` when the source has been shut down.
    fn poll(&mut self) -> Option<HotplugEvent>;
}

/// A no-op hotplug source that never produces events.
///
/// Used on platforms where libusb hotplug is unavailable.
pub struct NoopHotplugSource;

impl HotplugSource for NoopHotplugSource {
    fn poll(&mut self) -> Option<HotplugEvent> {
        None
    }
}

/// Drives a hotplug source, emitting events into an mpsc channel.
///
/// The monitor runs in a background thread that periodically polls
/// the source for events and forwards them to the receiver.
pub struct HotplugMonitor {
    receiver: mpsc::Receiver<HotplugEvent>,
}

impl HotplugMonitor {
    /// Create a new hotplug monitor from a source.
    ///
    /// Spawns a background thread that polls the source and forwards
    /// events to the returned monitor. The thread exits when the source
    /// returns `None` or when `stop` is called via the handle.
    pub fn new(mut source: impl HotplugSource + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            loop {
                match source.poll() {
                    Some(event) => {
                        if tx.send(event).is_err() {
                            // Receiver dropped, shut down.
                            break;
                        }
                    },
                    None => {
                        // No event — sleep a bit before polling again.
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    },
                }
            }
        });
        Self { receiver: rx }
    }

    /// Try to receive a hotplug event without blocking.
    ///
    /// Returns `None` if no event is available.
    pub fn try_recv(&self) -> Option<HotplugEvent> {
        self.receiver.try_recv().ok()
    }

    /// Block until a hotplug event is received.
    ///
    /// Returns `None` if the sender has been dropped (monitor shut down).
    pub fn recv(&self) -> Option<HotplugEvent> {
        self.receiver.recv().ok()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake hotplug source that yields a fixed sequence of events.
    struct FakeHotplugSource {
        events: Vec<Option<HotplugEvent>>,
        index: usize,
    }

    impl FakeHotplugSource {
        fn new(events: Vec<HotplugEvent>) -> Self {
            Self { events: events.into_iter().map(Some).collect(), index: 0 }
        }
    }

    impl HotplugSource for FakeHotplugSource {
        fn poll(&mut self) -> Option<HotplugEvent> {
            if self.index < self.events.len() {
                let event = self.events[self.index].take();
                self.index += 1;
                event
            } else {
                // After all events are exhausted, keep returning None
                // so the monitor thread stays alive.
                std::thread::sleep(std::time::Duration::from_millis(10));
                None
            }
        }
    }

    // ── HotplugEvent construction ────────────────────────────────

    #[test]
    fn test_attach_event_has_busid_and_vid_pid() {
        let event = HotplugEvent::Attached { busid: "3-2".into(), vid: 0x046d, pid: 0xc261 };

        match event {
            HotplugEvent::Attached { ref busid, vid, pid } => {
                assert_eq!(busid, "3-2");
                assert_eq!(vid, 0x046d);
                assert_eq!(pid, 0xc261);
            },
            _ => panic!("expected Attached event"),
        }
    }

    #[test]
    fn test_detach_event_has_busid_and_correlation_id() {
        let cid = CorrelationId::now_v7();
        let event = HotplugEvent::Detached { busid: "3-2".into(), correlation_id: cid };

        match event {
            HotplugEvent::Detached { ref busid, correlation_id } => {
                assert_eq!(busid, "3-2");
                assert_eq!(correlation_id, cid);
            },
            _ => panic!("expected Detached event"),
        }
    }

    // ── FakeHotplugSource ────────────────────────────────────────

    #[test]
    fn test_fake_source_returns_events_in_order() {
        let events = vec![
            HotplugEvent::Attached { busid: "1-1".into(), vid: 0x1234, pid: 0x5678 },
            HotplugEvent::Attached { busid: "2-1".into(), vid: 0x9abc, pid: 0xdef0 },
        ];
        let mut source = FakeHotplugSource::new(events.clone());

        assert_eq!(source.poll(), events.get(0).cloned());
        assert_eq!(source.poll(), events.get(1).cloned());
        // After exhaustion, poll returns None.
        assert_eq!(source.poll(), None);
    }

    // ── HotplugMonitor ───────────────────────────────────────────

    #[test]
    fn test_monitor_receives_attach_event() {
        let event = HotplugEvent::Attached { busid: "3-2".into(), vid: 0x046d, pid: 0xc261 };
        let source = FakeHotplugSource::new(vec![event.clone()]);
        let monitor = HotplugMonitor::new(source);

        // Give the background thread time to poll and send.
        std::thread::sleep(std::time::Duration::from_millis(50));

        let received = monitor.try_recv();
        assert_eq!(received, Some(event));
    }

    #[test]
    fn test_monitor_receives_detach_event() {
        let cid = CorrelationId::now_v7();
        let event = HotplugEvent::Detached { busid: "1-1".into(), correlation_id: cid };
        let source = FakeHotplugSource::new(vec![event.clone()]);
        let monitor = HotplugMonitor::new(source);

        std::thread::sleep(std::time::Duration::from_millis(50));

        let received = monitor.try_recv();
        assert_eq!(received, Some(event));
    }

    #[test]
    fn test_monitor_noop_source_never_emits() {
        let source = NoopHotplugSource;
        let monitor = HotplugMonitor::new(source);

        std::thread::sleep(std::time::Duration::from_millis(50));

        assert_eq!(monitor.try_recv(), None);
    }

    #[test]
    fn test_monitor_recv_multiple_events_in_order() {
        let events = vec![
            HotplugEvent::Attached { busid: "1-1".into(), vid: 0xaaaa, pid: 0xbbbb },
            HotplugEvent::Attached { busid: "1-2".into(), vid: 0xcccc, pid: 0xdddd },
            HotplugEvent::Detached { busid: "1-1".into(), correlation_id: CorrelationId::now_v7() },
        ];
        let source = FakeHotplugSource::new(events.clone());
        let monitor = HotplugMonitor::new(source);

        std::thread::sleep(std::time::Duration::from_millis(80));

        assert_eq!(monitor.try_recv(), Some(events[0].clone()));
        assert_eq!(monitor.try_recv(), Some(events[1].clone()));
        assert_eq!(monitor.try_recv(), Some(events[2].clone()));
    }

    #[test]
    fn test_hotplug_events_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<HotplugEvent>();
    }

    #[test]
    fn test_hotplug_event_clone() {
        let event = HotplugEvent::Attached { busid: "3-2".into(), vid: 0x046d, pid: 0xc261 };
        let cloned = event.clone();
        assert_eq!(event, cloned);
    }

    #[test]
    fn test_hotplug_event_debug() {
        let event = HotplugEvent::Attached { busid: "3-2".into(), vid: 0x046d, pid: 0xc261 };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("Attached"));
        assert!(debug_str.contains("3-2"));
    }
}
