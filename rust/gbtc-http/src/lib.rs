//! Bounded local HTTP and SSE service.

use gbtc_core::shutdown::Shutdown;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const HISTORY_CAPACITY: usize = 300;

pub struct Status {
    pub data: Value,
    samples: [f64; HISTORY_CAPACITY],
    head: usize,
    count: usize,
    seq: u64,
    start: Instant,
}

pub type SharedStatus = Arc<Mutex<Status>>;

impl Default for Status {
    fn default() -> Self {
        Self::new()
    }
}

impl Status {
    pub fn new() -> Self {
        let data = json!({
            "mh": 0.0,
            "min": 0.0,
            "max": 0.0,
            "avg": 0.0,
            "samples": 0,
            "uptime": 0,
            "submitted": 0u64,
            "accepted": 0u64,
            "rejected": 0u64,
            "lifetime_hashes": 0u64,
            "diff": 1.0,
            "job": "",
            "ntime": 0u32,
            "nbits": 0u32,
            "merkle_branches": 0,
            "pool": "",
            "worker": "",
            "addr": "",
            "backend": "",
            "backend_api": "",
            "device_path": "",
            "device_vendor": "",
            "device_name": "",
            "driver_name": "",
            "fallback_reason": "",
            "net_diff": 0.0,
            "net_hashrate": 0.0,
            "eta_block_s": 0.0,
            "prob_block_per_day": 0.0,
            "prob_block_24h": 0.0
        });
        Self {
            data,
            samples: [0.0; HISTORY_CAPACITY],
            head: 0,
            count: 0,
            seq: 0,
            start: Instant::now(),
        }
    }

    pub fn update_hashes(&mut self, hashes: u64, elapsed: f64) {
        let mh = if elapsed > 0.0 {
            let value = hashes as f64 / 1_000_000.0 / elapsed;
            if value.is_finite() {
                value
            } else {
                0.0
            }
        } else {
            0.0
        };
        self.data["mh"] = json!(mh);
    }

    pub fn sample(&mut self) {
        let mh = self.data.get("mh").and_then(Value::as_f64).unwrap_or(0.0);
        let mh = if mh.is_finite() { mh } else { 0.0 };

        self.samples[self.head] = mh;
        self.head = (self.head + 1) % HISTORY_CAPACITY;
        if self.count < HISTORY_CAPACITY {
            self.count += 1;
        }

        let start = (self.head + HISTORY_CAPACITY - self.count) % HISTORY_CAPACITY;
        let (min, max, sum) = if self.count == 0 {
            (0.0, 0.0, 0.0)
        } else {
            let mut min = self.samples[start];
            let mut max = self.samples[start];
            let mut sum = 0.0;
            for i in 0..self.count {
                let value = self.samples[(start + i) % HISTORY_CAPACITY];
                if value < min {
                    min = value;
                }
                if value > max {
                    max = value;
                }
                sum += value;
            }
            (min, max, sum)
        };

        self.data["mh"] = json!(mh);
        self.data["min"] = json!(min);
        self.data["max"] = json!(max);
        self.data["avg"] = json!(if self.count > 0 {
            sum / self.count as f64
        } else {
            0.0
        });
        self.data["samples"] = json!(self.count);
        self.seq += 1;
    }

    pub fn snapshot(&self, include_history: bool) -> Value {
        let mut snap = self.data.clone();

        let uptime = self.start.elapsed().as_secs();
        snap["uptime"] = json!(uptime);

        let mh = sanitize_f64(snap.get("mh").and_then(Value::as_f64).unwrap_or(0.0));
        snap["mh"] = json!(mh);
        let min = sanitize_f64(snap.get("min").and_then(Value::as_f64).unwrap_or(0.0));
        snap["min"] = json!(min);
        let max = sanitize_f64(snap.get("max").and_then(Value::as_f64).unwrap_or(0.0));
        snap["max"] = json!(max);
        let avg = sanitize_f64(snap.get("avg").and_then(Value::as_f64).unwrap_or(0.0));
        snap["avg"] = json!(avg);

        let nbits = snap.get("nbits").and_then(Value::as_u64).unwrap_or(0) as u32;
        let net_diff = nbits_to_difficulty(nbits);
        let net_hashrate = if net_diff > 0.0 {
            net_diff * 4_294_967_296.0 / 600.0
        } else {
            0.0
        };
        let your_h = mh * 1_000_000.0;
        let eta_block_s = if your_h > 0.0 && net_diff > 0.0 {
            net_diff * 4_294_967_296.0 / your_h
        } else {
            0.0
        };
        let prob_block_per_day = if net_hashrate > 0.0 && your_h > 0.0 {
            your_h / net_hashrate * 144.0
        } else {
            0.0
        };
        let prob_block_24h = if prob_block_per_day.is_finite() {
            1.0 - (-prob_block_per_day).exp()
        } else {
            0.0
        };

        snap["net_diff"] = json!(sanitize_f64(net_diff));
        snap["net_hashrate"] = json!(sanitize_f64(net_hashrate));
        snap["eta_block_s"] = json!(sanitize_f64(eta_block_s));
        snap["prob_block_per_day"] = json!(sanitize_f64(prob_block_per_day));
        snap["prob_block_24h"] = json!(sanitize_f64(prob_block_24h));

        if include_history {
            let start_idx = (self.head + HISTORY_CAPACITY - self.count) % HISTORY_CAPACITY;
            let mut history = Vec::with_capacity(self.count);
            for i in 0..self.count {
                history.push(json!(sanitize_f64(
                    self.samples[(start_idx + i) % HISTORY_CAPACITY]
                )));
            }
            snap["history"] = json!(history);
        } else if let Some(object) = snap.as_object_mut() {
            object.remove("history");
        }

        snap
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }
}

fn sanitize_f64(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn nbits_to_difficulty(nbits: u32) -> f64 {
    let mant = nbits & 0x00ffffff;
    let exp = (nbits >> 24) & 0xff;
    if mant == 0 || exp == 0 {
        return 0.0;
    }
    let value = (0xffff as f64) / (mant as f64) * 2.0f64.powf(8.0 * (0x1d as f64 - exp as f64));
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

pub fn serve(
    bind: SocketAddr,
    status: SharedStatus,
    stop: Arc<AtomicBool>,
) -> std::io::Result<JoinHandle<()>> {
    serve_with_shutdown(bind, status, Shutdown::from_flag(stop))
}

pub fn serve_with_shutdown(
    bind: SocketAddr,
    status: SharedStatus,
    shutdown: Shutdown,
) -> std::io::Result<JoinHandle<()>> {
    let listener = TcpListener::bind(bind)?;
    let _ = listener.set_nonblocking(true);
    let handle = thread::spawn(move || run_server(listener, status, shutdown));
    Ok(handle)
}

fn run_server(listener: TcpListener, status: SharedStatus, shutdown: Shutdown) {
    let active = Arc::new(AtomicUsize::new(0));
    let sampler_status = status.clone();
    let sampler_shutdown = shutdown.clone();
    let sampler = thread::spawn(move || loop {
        if sampler_shutdown.wait_timeout(Duration::from_secs(1)) {
            break;
        }
        let mut guard = sampler_status.lock().unwrap();
        guard.sample();
    });

    let stop = shutdown.flag();
    #[cfg(unix)]
    let listener_fd = {
        use std::os::unix::io::AsRawFd;
        listener.as_raw_fd()
    };
    #[cfg(unix)]
    let wake_fd_raw = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
    #[cfg(unix)]
    let wake_fd: Option<Arc<OwnedFd>> = if wake_fd_raw >= 0 {
        Some(Arc::new(unsafe { OwnedFd::from_raw_fd(wake_fd_raw) }))
    } else {
        None
    };
    #[cfg(not(unix))]
    let wake_fd: Option<Arc<OwnedFd>> = None;

    #[cfg(unix)]
    if let Some(fd) = &wake_fd {
        let fd = fd.clone();
        shutdown.on_stop(move || {
            let val: u64 = 1;
            unsafe {
                libc::write(fd.as_raw_fd(), &val as *const _ as *const _, 8);
            }
        });
    }

    let mut clients = Vec::<JoinHandle<()>>::new();
    while !stop.load(Ordering::Relaxed) {
        clients.retain(|handle| !handle.is_finished());

        #[cfg(unix)]
        {
            let poll_timeout = if shutdown.is_legacy() { 100 } else { -1 };
            let mut pollfds = [
                libc::pollfd {
                    fd: listener_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: wake_fd.as_ref().map(|fd| fd.as_raw_fd()).unwrap_or(-1),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let nfds = if wake_fd.is_some() { 2 } else { 1 };
            let pret = unsafe { libc::poll(pollfds.as_mut_ptr(), nfds, poll_timeout) };
            if pret < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() != std::io::ErrorKind::Interrupted {
                    thread::sleep(Duration::from_millis(50));
                }
                continue;
            }
            if nfds == 2 && (pollfds[1].revents & libc::POLLIN) != 0 {
                if let Some(fd) = &wake_fd {
                    let mut buf = [0u8; 8];
                    unsafe {
                        libc::read(fd.as_raw_fd(), buf.as_mut_ptr() as *mut _, 8);
                    }
                }
            }
            if (pollfds[0].revents & libc::POLLIN) == 0 {
                continue;
            }
        }

        #[cfg(not(unix))]
        {
            thread::sleep(Duration::from_millis(50));
        }

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    if active.load(Ordering::Relaxed) >= 16 {
                        drop(stream);
                        continue;
                    }
                    active.fetch_add(1, Ordering::Relaxed);
                    let active_clone = active.clone();
                    let status_clone = status.clone();
                    let shutdown_clone = shutdown.clone();
                    clients.push(thread::spawn(move || {
                        handle_client(stream, status_clone, shutdown_clone);
                        active_clone.fetch_sub(1, Ordering::Relaxed);
                    }));
                }
                Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(_) => {
                    thread::sleep(Duration::from_millis(10));
                    break;
                }
            }
        }
    }

    let _ = sampler.join();
    for client in clients {
        let _ = client.join();
    }
}

fn handle_client(mut stream: TcpStream, status: SharedStatus, shutdown: Shutdown) {
    let stop = shutdown.flag();
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    set_small_send_buffer(&stream);

    let request = match read_request(&mut stream, stop.as_ref()) {
        Ok(bytes) => bytes,
        Err(_) => {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            send_empty(&mut stream, "400 Bad Request");
            return;
        }
    };

    let header_end = match find_header_end(&request) {
        Some(index) => index,
        None => {
            send_empty(&mut stream, "400 Bad Request");
            return;
        }
    };
    let text = match std::str::from_utf8(&request[..header_end]) {
        Ok(text) => text,
        Err(_) => {
            send_empty(&mut stream, "400 Bad Request");
            return;
        }
    };
    let request_line = text.lines().next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    let version = parts.next().unwrap_or("");
    if method.is_empty()
        || !path.starts_with('/')
        || !["HTTP/1.0", "HTTP/1.1"].contains(&version)
        || parts.next().is_some()
    {
        send_empty(&mut stream, "400 Bad Request");
        return;
    }

    let is_get = method == "GET";
    let is_head = method == "HEAD";

    if method == "OPTIONS" {
        let response = "HTTP/1.1 204 No Content\r\nAllow: GET, HEAD, OPTIONS\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, HEAD, OPTIONS\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = write_all(&mut stream, response.as_bytes());
        return;
    }

    if is_get && http_path_is(path, "/events") {
        handle_sse(&mut stream, status, shutdown);
        return;
    }

    if (is_get || is_head) && http_path_is(path, "/status.json") {
        let snapshot = {
            let guard = status.lock().unwrap();
            guard.snapshot(true)
        };
        let body = snapshot.to_string();
        send_body(
            &mut stream,
            "200 OK",
            "application/json",
            body.as_bytes(),
            is_get,
        );
        return;
    }

    if (is_get || is_head) && is_index_path(path) {
        send_body(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes(),
            is_get,
        );
        return;
    }

    if (is_get || is_head) && http_path_is(path, "/favicon.ico") {
        send_empty(&mut stream, "204 No Content");
        return;
    }

    if !is_get && !is_head {
        send_empty(&mut stream, "405 Method Not Allowed");
    } else {
        send_empty(&mut stream, "404 Not Found");
    }
}

fn handle_sse(stream: &mut TcpStream, status: SharedStatus, shutdown: Shutdown) {
    let stop = shutdown.flag();
    let header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache, no-store\r\nConnection: close\r\nX-Accel-Buffering: no\r\nAccess-Control-Allow-Origin: *\r\n\r\nretry: 3000\n\n";
    if write_all(stream, header.as_bytes()).is_err() {
        return;
    }

    let initial = { status.lock().unwrap().snapshot(true) };
    let message = format!("data: {}\n\n", initial);
    if write_all(stream, message.as_bytes()).is_err() {
        return;
    }

    while !stop.load(Ordering::Relaxed) {
        if shutdown.wait_timeout(Duration::from_secs(1)) {
            return;
        }
        if stop.load(Ordering::Relaxed) {
            return;
        }

        let snapshot = {
            let guard = status.lock().unwrap();
            guard.snapshot(false)
        };
        let message = format!("data: {}\n\n", snapshot);
        if write_all(stream, message.as_bytes()).is_err() {
            return;
        }
    }
}

fn read_request(stream: &mut TcpStream, stop: &AtomicBool) -> std::io::Result<Vec<u8>> {
    let started = Instant::now();
    let mut buffer = Vec::new();
    let mut temporary = [0u8; 1024];
    loop {
        if stop.load(Ordering::Relaxed) || started.elapsed() >= Duration::from_secs(5) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "request deadline",
            ));
        }
        if let Some(end) = find_header_end(&buffer) {
            if end > 16 * 1024 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "headers too large",
                ));
            }
            break;
        }
        if buffer.len() >= 16 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "request headers too large",
            ));
        }
        match stream.read(&mut temporary) {
            Ok(0) => break,
            Ok(count) => buffer.extend_from_slice(&temporary[..count]),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut
                    || error.kind() == std::io::ErrorKind::Interrupted =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    if !buffer.windows(4).any(|window| window == b"\r\n\r\n") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "incomplete request",
        ));
    }
    Ok(buffer)
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn http_path_is(path: &str, want: &str) -> bool {
    if let Some(rest) = path.strip_prefix(want) {
        rest.is_empty() || rest.starts_with('?')
    } else {
        false
    }
}

fn is_index_path(path: &str) -> bool {
    http_path_is(path, "/") || http_path_is(path, "/index") || http_path_is(path, "/index.html")
}

#[cfg(unix)]
fn set_small_send_buffer(stream: &TcpStream) {
    use std::os::unix::io::AsRawFd;
    unsafe {
        extern "C" {
            fn setsockopt(
                sockfd: std::os::raw::c_int,
                level: std::os::raw::c_int,
                optname: std::os::raw::c_int,
                optval: *const std::ffi::c_void,
                optlen: u32,
            ) -> std::os::raw::c_int;
        }
        let fd = stream.as_raw_fd();
        let size: i32 = 1024;
        let _ = setsockopt(
            fd,
            1,
            7,
            &size as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&size) as u32,
        );
    }
}

#[cfg(not(unix))]
fn set_small_send_buffer(_stream: &TcpStream) {}

fn write_all(stream: &mut TcpStream, mut buffer: &[u8]) -> std::io::Result<()> {
    let started = Instant::now();
    while !buffer.is_empty() {
        let remaining = Duration::from_millis(500)
            .checked_sub(started.elapsed())
            .filter(|v| !v.is_zero())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "response deadline")
            })?;
        stream.set_write_timeout(Some(remaining))?;
        let written = stream.write(buffer)?;
        if written == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "write zero",
            ));
        }
        buffer = &buffer[written..];
    }
    Ok(())
}

fn send_empty(stream: &mut TcpStream, status: &str) {
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        status
    );
    let _ = write_all(stream, response.as_bytes());
}

fn send_body(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    send_body: bool,
) {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    );
    let _ = write_all(stream, header.as_bytes());
    if send_body {
        let _ = write_all(stream, body);
    }
}

static INDEX_HTML: &str = include_str!("../assets/index.html");
