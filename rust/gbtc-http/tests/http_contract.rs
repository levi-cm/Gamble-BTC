use gbtc_core::shutdown::Shutdown;
use gbtc_http::{serve, serve_with_shutdown, SharedStatus, Status};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn read_response(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut buffer = Vec::new();
    let mut temporary = [0u8; 4096];
    loop {
        match stream.read(&mut temporary) {
            Ok(0) => break,
            Ok(count) => buffer.extend_from_slice(&temporary[..count]),
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buffer).to_string()
}

fn request(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let _ = stream.shutdown(std::net::Shutdown::Write);
    read_response(&mut stream)
}

#[test]
fn routes_head_errors_and_query() {
    let port = free_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let status: SharedStatus = Arc::new(Mutex::new(Status::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let handle = serve(addr, status, stop.clone()).unwrap();

    let index = request(port, "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    assert!(index.starts_with("HTTP/1.1 200 OK"));
    assert!(index.contains("<title>gamble-btc</title>"));

    let head = request(port, "HEAD / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    assert!(head.starts_with("HTTP/1.1 200 OK"));
    assert!(head.contains("Content-Length: "));
    assert!(!head.contains("<title>gamble-btc</title>"));

    let status_body = request(
        port,
        "GET /status.json?x=1 HTTP/1.1\r\nHost: localhost\r\n\r\n",
    );
    assert!(status_body.starts_with("HTTP/1.1 200 OK"));
    assert!(status_body.contains("\"mh\""));

    let favicon = request(port, "GET /favicon.ico HTTP/1.1\r\nHost: localhost\r\n\r\n");
    assert!(favicon.starts_with("HTTP/1.1 204 No Content"));

    let options = request(
        port,
        "OPTIONS /anything HTTP/1.1\r\nHost: localhost\r\n\r\n",
    );
    assert!(options.starts_with("HTTP/1.1 204 No Content"));
    assert!(options.contains("Access-Control-Allow-Origin: *"));
    assert!(options.contains("Access-Control-Allow-Methods: GET, HEAD, OPTIONS"));

    let not_found = request(port, "GET /nope HTTP/1.1\r\nHost: localhost\r\n\r\n");
    assert!(not_found.starts_with("HTTP/1.1 404 Not Found"));

    let method_not_allowed = request(port, "POST / HTTP/1.1\r\nHost: localhost\r\n\r\n");
    assert!(method_not_allowed.starts_with("HTTP/1.1 405 Method Not Allowed"));

    let malformed = request(port, "GET\r\n\r\n");
    assert!(malformed.starts_with("HTTP/1.1 400 Bad Request"));

    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn status_keys_history_ring_and_escape() {
    let mut status = Status::new();
    status.data["job"] = serde_json::json!("a\"b\nc<script>");
    status.data["pool"] = serde_json::json!("p&ool");
    status.update_hashes(2_000_000, 2.0);
    assert_eq!(status.data["mh"].as_f64().unwrap(), 1.0);

    for _ in 0..305 {
        status.sample();
    }
    let snapshot = status.snapshot(true);
    let required = [
        "mh",
        "min",
        "max",
        "avg",
        "samples",
        "uptime",
        "submitted",
        "accepted",
        "rejected",
        "lifetime_hashes",
        "diff",
        "job",
        "ntime",
        "nbits",
        "merkle_branches",
        "pool",
        "worker",
        "addr",
        "backend",
        "backend_api",
        "device_path",
        "device_vendor",
        "device_name",
        "driver_name",
        "fallback_reason",
        "net_diff",
        "net_hashrate",
        "eta_block_s",
        "prob_block_per_day",
        "prob_block_24h",
        "history",
    ];
    for key in required {
        assert!(snapshot.get(key).is_some(), "missing {}", key);
    }
    let history = snapshot.get("history").unwrap().as_array().unwrap();
    assert_eq!(history.len(), 300);

    let text = snapshot.to_string();
    assert!(text.contains("\\\""));
    assert!(text.contains("\\n"));
    assert!(!text.contains("NaN"));
    assert!(!text.contains("Infinity"));

    let no_history = status.snapshot(false);
    assert!(no_history.get("history").is_none());
}

#[test]
fn sse_initial_and_incremental() {
    let port = free_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let status: SharedStatus = Arc::new(Mutex::new(Status::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let handle = serve(addr, status.clone(), stop.clone()).unwrap();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .write_all(b"GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();

    let mut buffer = String::new();
    let mut temporary = [0u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match stream.read(&mut temporary) {
            Ok(0) => break,
            Ok(count) => {
                buffer.push_str(&String::from_utf8_lossy(&temporary[..count]));
                if buffer.matches("data: ").count() >= 2 {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    assert!(buffer.contains("retry: 3000"));
    assert!(buffer.contains("data: "));
    assert!(buffer.matches("data: ").count() >= 2);
    assert!(buffer.contains("\"history\""));

    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn stop_does_not_wait_forever() {
    let port = free_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let status: SharedStatus = Arc::new(Mutex::new(Status::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let handle = serve(addr, status, stop.clone()).unwrap();

    stop.store(true, Ordering::Relaxed);
    let join = thread::spawn(move || handle.join().unwrap());
    assert!(join.join().is_ok());
}

#[test]
fn slow_reader_disconnects() {
    let port = free_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let status: SharedStatus = Arc::new(Mutex::new(Status::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let handle = serve(addr, status, stop.clone()).unwrap();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .write_all(b"GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut temporary = [0u8; 64];
    let _ = stream.read(&mut temporary);
    thread::sleep(Duration::from_secs(3));
    stop.store(true, Ordering::Relaxed);
    let join_result = handle.join();
    assert!(join_result.is_ok());

    let _ = stream.shutdown(std::net::Shutdown::Both);
}

#[test]
fn malformed_request_version_is_rejected() {
    let port = free_port();
    let status = Arc::new(Mutex::new(Status::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let server = serve(
        SocketAddr::from(([127, 0, 0, 1], port)),
        status,
        stop.clone(),
    )
    .unwrap();
    for line in ["GET /", "GET / HTTP/9.9", "GET / HTTP/1.1 extra"] {
        assert!(request(port, &format!("{line}\r\n\r\n")).starts_with("HTTP/1.1 400"));
    }
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
}

#[test]
fn shutdown_closes_client_waiting_for_headers() {
    let port = free_port();
    let status = Arc::new(Mutex::new(Status::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let server = serve(
        SocketAddr::from(([127, 0, 0, 1], port)),
        status,
        stop.clone(),
    )
    .unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(b"GET /").unwrap();
    thread::sleep(Duration::from_millis(100));
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let mut byte = [0u8; 1];
    assert_eq!(stream.read(&mut byte).unwrap(), 0);
}

#[test]
fn stops_promptly_on_shutdown() {
    let port = free_port();
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let status: SharedStatus = Arc::new(Mutex::new(Status::new()));
    let shutdown = Shutdown::new();
    let handle = serve_with_shutdown(addr, status, shutdown.clone()).unwrap();

    shutdown.request_stop();
    let start = Instant::now();
    handle.join().unwrap();
    assert!(start.elapsed() < Duration::from_millis(500));
}
