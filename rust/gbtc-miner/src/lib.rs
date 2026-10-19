pub mod bench;
pub mod config;
pub mod controller;

pub use bench::*;
pub use config::*;
pub use controller::*;

pub mod watchdog {
    use gbtc_core::shutdown::Shutdown;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    pub fn watchdog<F>(
        stop: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
        deadline: Duration,
        on_fatal: F,
    ) -> thread::JoinHandle<()>
    where
        F: FnOnce() + Send + 'static,
    {
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if finished.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            let start = Instant::now();
            while start.elapsed() < deadline {
                if finished.load(Ordering::Relaxed) {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
            if !finished.load(Ordering::Relaxed) {
                on_fatal();
            }
        })
    }

    pub fn watchdog_with_shutdown<F>(
        shutdown: Shutdown,
        finished: Arc<AtomicBool>,
        deadline: Duration,
        on_fatal: F,
    ) -> thread::JoinHandle<()>
    where
        F: FnOnce() + Send + 'static,
    {
        thread::spawn(move || {
            // Block until stop is requested (no periodic polling in production).
            while !shutdown.is_stopped() {
                if shutdown.wait_timeout(Duration::from_secs(24 * 60 * 60)) {
                    break;
                }
            }
            let deadline_at = Instant::now() + deadline;
            while !finished.load(Ordering::Relaxed) {
                let now = Instant::now();
                if now >= deadline_at {
                    break;
                }
                let remaining = deadline_at - now;
                shutdown.wait_until(|| finished.load(Ordering::Relaxed), remaining);
            }
            if !finished.load(Ordering::Relaxed) {
                on_fatal();
            }
        })
    }
}

pub use watchdog::*;

#[cfg(test)]
mod watchdog_tests {
    use super::watchdog::{watchdog, watchdog_with_shutdown};
    use gbtc_core::shutdown::Shutdown;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    };
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn deadline_begins_only_after_stop() {
        let stop = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let handle = watchdog(
            stop.clone(),
            finished,
            Duration::from_millis(30),
            move || {
                tx.send(()).unwrap();
            },
        );
        assert!(rx.recv_timeout(Duration::from_millis(150)).is_err());
        stop.store(true, Ordering::Relaxed);
        rx.recv_timeout(Duration::from_millis(400)).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn completed_cleanup_disarms_watchdog() {
        let stop = Arc::new(AtomicBool::new(true));
        let finished = Arc::new(AtomicBool::new(true));
        let handle = watchdog(stop, finished, Duration::from_millis(10), || {
            panic!("already finished")
        });
        handle.join().unwrap();
    }

    #[test]
    fn watchdog_disarms_promptly_after_cleanup() {
        let shutdown = Shutdown::new();
        let finished = Arc::new(AtomicBool::new(false));
        let handle = watchdog_with_shutdown(
            shutdown.clone(),
            finished.clone(),
            Duration::from_secs(10),
            || panic!("fatal"),
        );
        shutdown.request_stop();
        let shutdown2 = shutdown.clone();
        let finished2 = finished.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            finished2.store(true, Ordering::Relaxed);
            shutdown2.notify();
        });
        let start = Instant::now();
        handle.join().unwrap();
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
