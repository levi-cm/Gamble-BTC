use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Shutdown {
    inner: Arc<Inner>,
}

struct Inner {
    flag: Arc<AtomicBool>,
    mutex: Mutex<()>,
    condvar: Condvar,
    callbacks: Mutex<Vec<Arc<dyn Fn() + Send + Sync>>>,
    legacy: bool,
}

impl Shutdown {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                flag: Arc::new(AtomicBool::new(false)),
                mutex: Mutex::new(()),
                condvar: Condvar::new(),
                callbacks: Mutex::new(Vec::new()),
                legacy: false,
            }),
        }
    }

    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self {
            inner: Arc::new(Inner {
                flag,
                mutex: Mutex::new(()),
                condvar: Condvar::new(),
                callbacks: Mutex::new(Vec::new()),
                legacy: true,
            }),
        }
    }

    pub fn flag(&self) -> Arc<AtomicBool> {
        self.inner.flag.clone()
    }

    pub fn is_stopped(&self) -> bool {
        self.inner.flag.load(Ordering::Relaxed)
    }

    pub fn is_legacy(&self) -> bool {
        self.inner.legacy
    }

    pub fn request_stop(&self) {
        self.inner.flag.store(true, Ordering::Relaxed);
        self.notify();
    }

    pub fn notify(&self) {
        let callbacks: Vec<Arc<dyn Fn() + Send + Sync>> =
            self.inner.callbacks.lock().unwrap().clone();
        for callback in callbacks {
            callback();
        }
        let guard = self.inner.mutex.lock().unwrap();
        self.inner.condvar.notify_all();
        drop(guard);
    }

    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        if self.is_stopped() {
            return true;
        }
        let deadline = Instant::now() + timeout;
        let mut guard = self.inner.mutex.lock().unwrap();
        while !self.inner.flag.load(Ordering::Relaxed) {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;
            let slice = if self.inner.legacy {
                remaining.min(Duration::from_millis(100))
            } else {
                remaining
            };
            let result = self.inner.condvar.wait_timeout(guard, slice).unwrap();
            guard = result.0;
        }
        drop(guard);
        self.inner.flag.load(Ordering::Relaxed)
    }

    pub fn wait(&self, timeout: Duration) {
        self.wait_notify(timeout);
    }

    pub fn wait_notify(&self, timeout: Duration) {
        let effective = if self.inner.legacy {
            timeout.min(Duration::from_millis(100))
        } else {
            timeout
        };
        let guard = self.inner.mutex.lock().unwrap();
        let _ = self.inner.condvar.wait_timeout(guard, effective);
    }

    /// Wait for a predicate under the same lock used by notify, including after stop.
    pub fn wait_until<F>(&self, predicate: F, timeout: Duration) -> bool
    where
        F: Fn() -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut guard = self.inner.mutex.lock().unwrap();
        while !predicate() {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let result = self
                .inner
                .condvar
                .wait_timeout(guard, deadline - now)
                .unwrap();
            guard = result.0;
            if result.1.timed_out() {
                break;
            }
        }
        predicate()
    }

    pub fn on_stop<F>(&self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.inner
            .callbacks
            .lock()
            .unwrap()
            .push(Arc::new(callback));
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_stop_checks_preserve_requested_deadline() {
        let shutdown = Shutdown::from_flag(Arc::new(AtomicBool::new(false)));
        let started = Instant::now();
        assert!(!shutdown.wait_timeout(Duration::from_millis(220)));
        assert!(started.elapsed() >= Duration::from_millis(220));
    }
    #[test]
    fn legacy_flag_still_interrupts_long_deadline() {
        let flag = Arc::new(AtomicBool::new(false));
        let shutdown = Shutdown::from_flag(flag.clone());
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            flag.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        assert!(shutdown.wait_timeout(Duration::from_secs(10)));
        assert!(started.elapsed() < Duration::from_secs(1));
        writer.join().unwrap();
    }
}
