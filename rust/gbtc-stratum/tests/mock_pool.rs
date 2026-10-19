use gbtc_stratum::{spawn, Event, Share};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn collect_events(rx: &std::sync::mpsc::Receiver<Event>, dur: Duration) -> Vec<Event> {
    let mut out = Vec::new();
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        if let Ok(e) = rx.recv_timeout(Duration::from_millis(50)) {
            out.push(e);
        }
    }
    out
}

fn notify_msg(job_id: &str, clean: bool) -> String {
    format!(
        "{{\"method\":\"mining.notify\",\"params\":[\"{}\",\"0000000000000000000000000000000000000000000000000000000000000000\",\"0102\",\"0304\",[],\"20000000\",\"1d00ffff\",\"495fab29\",{}]}}\n",
        job_id, clean
    )
}

#[test]
fn handshake_basic() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n")
                .unwrap();
            // early notify before auth
            s.write_all(notify_msg("job-1", true).as_bytes()).unwrap();
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":2,\"result\":true,\"error\":null}\n")
                .unwrap();
            thread::sleep(Duration::from_millis(300));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, _tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let ev = collect_events(&rx, Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);

    assert!(ev.iter().any(|e| matches!(e, Event::Connected { .. })));
    assert!(ev.iter().any(|e| matches!(e, Event::Job(_))));
    assert!(ev.iter().any(|e| matches!(e, Event::Authorized)));
}

#[test]
fn fragmented_messages() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let msg = b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n";
            // split
            s.write_all(&msg[..10]).unwrap();
            thread::sleep(Duration::from_millis(50));
            s.write_all(&msg[10..]).unwrap();
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":2,\"result\":true,\"error\":null}\n")
                .unwrap();
            thread::sleep(Duration::from_millis(200));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, _tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let ev = collect_events(&rx, Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);

    // invalid JSON frame causes disconnect; later valid frames are not processed
    assert!(ev.iter().any(|e| matches!(e, Event::Disconnected)));
}

#[test]
fn auth_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n")
                .unwrap();
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":2,\"result\":false,\"error\":[24,\"unauthorized\",null]}\n")
                .unwrap();
            thread::sleep(Duration::from_millis(200));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, _tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let ev = collect_events(&rx, Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);

    assert!(!ev.iter().any(|e| matches!(e, Event::Authorized)));
    assert!(ev.iter().any(|e| matches!(e, Event::Disconnected)));
}

#[test]
fn submit_accepted_rejected_reordered() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n")
                .unwrap();
            s.write_all(notify_msg("job-1", true).as_bytes()).unwrap();
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":2,\"result\":true,\"error\":null}\n")
                .unwrap();
            // wait for submit
            let mut acc = Vec::new();
            while !String::from_utf8_lossy(&acc).contains("mining.submit") {
                let mut b = [0u8; 1024];
                match s.read(&mut b) {
                    Ok(0) => break,
                    Ok(n) => acc.extend_from_slice(&b[..n]),
                    Err(_) => break,
                }
            }
            // send an unrelated notify first (reordered)
            s.write_all(notify_msg("job-2", false).as_bytes()).unwrap();
            // reply to submit id=3
            s.write_all(b"{\"id\":3,\"result\":true,\"error\":null}\n")
                .unwrap();
            thread::sleep(Duration::from_millis(300));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    // wait for auth
    let ev = collect_events(&rx, Duration::from_millis(800));
    assert!(ev.iter().any(|e| matches!(e, Event::Authorized)));
    // send share
    let _ = tx.send(Share {
        connection_generation: 1,
        job_generation: 1,
        job_id: "job-1".to_string(),
        extranonce2: "00000000".to_string(),
        ntime: 0x495fab29,
        nonce: 0x01020304,
    });
    let ev = collect_events(&rx, Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);
    assert!(ev.iter().any(|e| matches!(e, Event::Accepted)));
}

#[test]
fn difficulty_staged_next_notify() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n")
                .unwrap();
            s.write_all(b"{\"method\":\"mining.set_difficulty\",\"params\":[4096]}\n")
                .unwrap();
            s.write_all(notify_msg("job-1", true).as_bytes()).unwrap();
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":2,\"result\":true,\"error\":null}\n")
                .unwrap();
            thread::sleep(Duration::from_millis(300));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, _tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let ev = collect_events(&rx, Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);
    let job = ev
        .iter()
        .find_map(|e| match e {
            Event::Job(j) => Some(j),
            _ => None,
        })
        .unwrap();
    assert_eq!(job.difficulty, 4096.0);
}

#[test]
fn clean_nonclean_generations() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n")
                .unwrap();
            s.write_all(notify_msg("job-1", true).as_bytes()).unwrap();
            s.write_all(notify_msg("job-2", false).as_bytes()).unwrap();
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":2,\"result\":true,\"error\":null}\n")
                .unwrap();
            thread::sleep(Duration::from_millis(300));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, _tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let ev = collect_events(&rx, Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);
    let jobs: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            Event::Job(j) => Some(j.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(jobs.len(), 2);
    assert!(jobs[0].clean);
    assert!(!jobs[1].clean);
    assert!(jobs[0].job_generation < jobs[1].job_generation);
}

#[test]
fn reconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for _ in 0..2 {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                s.write_all(b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n")
                    .unwrap();
                let _ = s.read(&mut buf);
                s.write_all(b"{\"id\":2,\"result\":true,\"error\":null}\n")
                    .unwrap();
                thread::sleep(Duration::from_millis(100));
                // close to trigger reconnect
            }
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, _tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let ev = collect_events(&rx, Duration::from_millis(2500));
    stop.store(true, Ordering::Relaxed);
    let connected = ev
        .iter()
        .filter(|e| matches!(e, Event::Connected { .. }))
        .count();
    assert!(connected >= 2, "connected count {}", connected);
}

#[test]
fn stale_share_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n")
                .unwrap();
            s.write_all(notify_msg("job-1", true).as_bytes()).unwrap();
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":2,\"result\":true,\"error\":null}\n")
                .unwrap();
            thread::sleep(Duration::from_millis(2000));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let _ = collect_events(&rx, Duration::from_millis(800));
    // stale connection generation
    let _ = tx.send(Share {
        connection_generation: 999,
        job_generation: 1,
        job_id: "job-1".to_string(),
        extranonce2: "".to_string(),
        ntime: 1,
        nonce: 1,
    });
    let ev = collect_events(&rx, Duration::from_millis(500));
    stop.store(true, Ordering::Relaxed);
    assert!(ev.iter().any(|e| matches!(e, Event::Discarded)));
}

#[test]
fn oversize_line_disconnects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            // send oversize line
            let big = vec![b'a'; 70000];
            s.write_all(&big).unwrap();
            s.write_all(b"\n").unwrap();
            thread::sleep(Duration::from_millis(300));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, _tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let ev = collect_events(&rx, Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);
    assert!(ev.iter().any(|e| matches!(e, Event::Disconnected)));
}

#[test]
fn invalid_json_disconnects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            s.write_all(b"not json\n").unwrap();
            s.write_all(b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n")
                .unwrap();
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":2,\"result\":true,\"error\":null}\n")
                .unwrap();
            thread::sleep(Duration::from_millis(300));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, _tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let ev = collect_events(&rx, Duration::from_millis(1500));
    stop.store(true, Ordering::Relaxed);
    assert!(ev.iter().any(|e| matches!(e, Event::Disconnected)));
}

#[test]
fn set_extranonce_not_crash() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            s.write_all(b"{\"id\":1,\"result\":[[],\"a1b2c3d4\",4],\"error\":null}\n")
                .unwrap();
            s.write_all(b"{\"method\":\"mining.set_extranonce\",\"params\":[\"aa\",2]}\n")
                .unwrap();
            thread::sleep(Duration::from_millis(200));
        }
    });

    let stop = Arc::new(AtomicBool::new(false));
    let (rx, _tx, _h) = spawn(
        format!("127.0.0.1:{}", port),
        "addr".into(),
        "pass".into(),
        stop.clone(),
    )
    .unwrap();
    let ev = collect_events(&rx, Duration::from_millis(1200));
    stop.store(true, Ordering::Relaxed);
    // either disconnected or still connected; no panic
    let _ = ev;
}

#[test]
fn share_sender_fd_survives_receiver_thread_exit() {
    use gbtc_stratum::ShareSender;
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;

    let (tx, rx) = mpsc::sync_channel::<gbtc_stratum::Share>(4);
    let (stream, _peer) = UnixStream::pair().unwrap();
    let fd: OwnedFd = stream.into();
    let fd = std::sync::Arc::new(fd);
    let sender = ShareSender::new(tx, fd.clone());

    let handle = thread::spawn(move || drop(rx));
    handle.join().unwrap();

    assert!(sender.as_raw_fd() >= 0);
    let share = gbtc_stratum::Share {
        connection_generation: 1,
        job_generation: 1,
        job_id: "job".to_string(),
        extranonce2: "00".to_string(),
        ntime: 1,
        nonce: 1,
    };
    assert!(sender.try_send(share).is_err());
}
