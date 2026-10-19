use gbtc_core::job::MiningJob;
use gbtc_core::shutdown::Shutdown;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown as TcpShutdown, TcpStream, ToSocketAddrs};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SendError, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const LINE_LIMIT: usize = 65536;
const READ_TIMEOUT: Duration = Duration::from_millis(100);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const PENDING_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_CAP: usize = 256;
const SHARE_CAP: usize = 64;
const SHARE_DRAIN_LIMIT: usize = SHARE_CAP;
const MAX_PENDING: usize = 128;
const MAX_JOBS: usize = 64;

pub type ClientChannels = (Receiver<Event>, ShareSender, JoinHandle<()>);

pub struct Client;

#[derive(Clone)]
pub struct ShareSender {
    inner: SyncSender<Share>,
    wake_fd: Option<Arc<OwnedFd>>,
}

impl ShareSender {
    pub fn new(inner: SyncSender<Share>, wake_fd: Arc<OwnedFd>) -> Self {
        Self {
            inner,
            wake_fd: Some(wake_fd),
        }
    }

    pub fn try_send(&self, share: Share) -> Result<(), TrySendError<Share>> {
        let result = self.inner.try_send(share);
        if result.is_ok() {
            self.wake();
        }
        result
    }

    pub fn send(&self, share: Share) -> Result<(), SendError<Share>> {
        let result = self.inner.send(share);
        if result.is_ok() {
            self.wake();
        }
        result
    }

    fn wake(&self) {
        if let Some(fd) = &self.wake_fd {
            let val: u64 = 1;
            unsafe {
                libc::write(fd.as_raw_fd(), &val as *const _ as *const _, 8);
            }
        }
    }
}

impl AsRawFd for ShareSender {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        self.wake_fd.as_ref().map(|fd| fd.as_raw_fd()).unwrap_or(-1)
    }
}

impl From<SyncSender<Share>> for ShareSender {
    fn from(inner: SyncSender<Share>) -> Self {
        Self {
            inner,
            wake_fd: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Share {
    pub connection_generation: u64,
    pub job_generation: u64,
    pub job_id: String,
    pub extranonce2: String,
    pub ntime: u32,
    pub nonce: u32,
}

#[derive(Debug, Clone)]
pub enum Event {
    Connected {
        connection_generation: u64,
        extranonce1: Vec<u8>,
        extranonce2_size: usize,
    },
    Job(MiningJob),
    Authorized,
    Submitted,
    Accepted,
    Rejected,
    Discarded,
    Unresolved,
    Disconnected,
}

struct State {
    conn_gen: u64,
    authorized: bool,
    extranonce1: Vec<u8>,
    extranonce2_size: usize,
    jobs: Vec<MiningJob>,
    pending: HashMap<i64, Instant>,
    staged_difficulty: f64,
    staged_target: [u32; 8],
    staged_target_gen: u64,
    job_gen: u64,
}

pub fn spawn(
    url: String,
    address: String,
    password: String,
    stop: Arc<AtomicBool>,
) -> Result<ClientChannels, String> {
    spawn_with_shutdown(url, address, password, Shutdown::from_flag(stop))
}

pub fn spawn_with_shutdown(
    url: String,
    address: String,
    password: String,
    shutdown: Shutdown,
) -> Result<ClientChannels, String> {
    parse_url(&url)?;
    let (event_tx, event_rx) = mpsc::sync_channel(EVENT_CAP);
    let (share_raw, share_rx) = mpsc::sync_channel(SHARE_CAP);
    let wake_raw = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
    if wake_raw < 0 {
        return Err("eventfd failed".to_string());
    }
    let wake_fd = unsafe { OwnedFd::from_raw_fd(wake_raw) };
    let wake_fd = Arc::new(wake_fd);
    let share_tx = ShareSender::new(share_raw, wake_fd.clone());
    let shutdown_for_thread = shutdown.clone();
    let wake_for_thread = wake_fd.clone();
    let handle = thread::spawn(move || {
        run_client(
            url,
            address,
            password,
            shutdown_for_thread,
            event_tx,
            share_rx,
            wake_for_thread,
        );
    });
    Ok((event_rx, share_tx, handle))
}

fn run_client(
    url: String,
    address: String,
    password: String,
    shutdown: Shutdown,
    event_tx: SyncSender<Event>,
    share_rx: Receiver<Share>,
    wake_fd: Arc<OwnedFd>,
) {
    let stop = shutdown.flag();
    let wake_for_stop = wake_fd.clone();
    shutdown.on_stop(move || {
        let val: u64 = 1;
        unsafe {
            libc::write(wake_for_stop.as_raw_fd(), &val as *const _ as *const _, 8);
        }
    });

    let mut conn_gen: u64 = 0;
    let mut backoff = Duration::from_secs(1);

    while !stop.load(Ordering::Relaxed) {
        let mut next_id: i64 = 1;
        let stream = match connect_url(&url, &stop) {
            Ok(s) => s,
            Err(_) => {
                shutdown.wait_timeout(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        backoff = Duration::from_secs(1);

        conn_gen += 1;
        let current_conn = conn_gen;

        let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
        let mut stream = stream;
        let mut read_buf: Vec<u8> = Vec::new();

        let default_target =
            gbtc_core::target_from_difficulty(1.0).unwrap_or([0, 0xffff0000, 0, 0, 0, 0, 0, 0]);
        let mut st = State {
            conn_gen: current_conn,
            authorized: false,
            extranonce1: Vec::new(),
            extranonce2_size: 0,
            jobs: Vec::new(),
            pending: HashMap::new(),
            staged_difficulty: 1.0,
            staged_target: default_target,
            staged_target_gen: 0,
            job_gen: 0,
        };

        let mut disconnect = false;

        // subscribe
        let sid = next_id;
        next_id += 1;
        if send_json(
            &mut stream,
            &serde_json::json!({"id": sid, "method": "mining.subscribe", "params": ["gamble-btc/0.2"]}),
        )
        .is_err()
        {
            let _ = stream.shutdown(TcpShutdown::Both);
            let _ = send_event(&event_tx, Event::Disconnected, &stop);
            shutdown.wait_timeout(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        }

        let mut subscribed = false;
        let mut early_jobs: Vec<MiningJob> = Vec::new();
        let sub_start = Instant::now();
        'sub: loop {
            if stop.load(Ordering::Relaxed) {
                disconnect = true;
                break 'sub;
            }
            if sub_start.elapsed() >= HANDSHAKE_TIMEOUT {
                break 'sub;
            }
            match read_line(&mut stream, &mut read_buf) {
                Ok(Some(line)) => match serde_json::from_str::<Value>(&line) {
                    Ok(v) => {
                        if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
                            match method {
                                "mining.set_difficulty" => {
                                    if !apply_set_difficulty(&mut st, &v) {
                                        disconnect = true;
                                        break 'sub;
                                    }
                                }
                                "mining.notify" => match apply_notify(&mut st, &v) {
                                    Ok(job) => {
                                        if job.clean {
                                            early_jobs.clear();
                                        }
                                        if early_jobs.len() >= MAX_JOBS {
                                            early_jobs.remove(0);
                                        }
                                        early_jobs.push(job);
                                    }
                                    Err(_) => {
                                        disconnect = true;
                                        break 'sub;
                                    }
                                },
                                "mining.set_extranonce" => {
                                    disconnect = true;
                                    break 'sub;
                                }
                                _ => {}
                            }
                        } else if v.get("id").and_then(|x| x.as_i64()) == Some(sid) {
                            if parse_subscribe(&v, &mut st.extranonce1, &mut st.extranonce2_size) {
                                subscribed = true;
                            }
                            break 'sub;
                        }
                    }
                    Err(_) => {
                        disconnect = true;
                        break 'sub;
                    }
                },
                Ok(None) => {}
                Err(_) => {
                    disconnect = true;
                    break 'sub;
                }
            }
        }

        if disconnect {
            let _ = stream.shutdown(TcpShutdown::Both);
            if !send_event(&event_tx, Event::Disconnected, &stop) {
                return;
            }
            shutdown.wait_timeout(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        }
        if !subscribed {
            let _ = stream.shutdown(TcpShutdown::Both);
            if !send_event(&event_tx, Event::Disconnected, &stop) {
                return;
            }
            shutdown.wait_timeout(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        }

        if !send_event(
            &event_tx,
            Event::Connected {
                connection_generation: current_conn,
                extranonce1: st.extranonce1.clone(),
                extranonce2_size: st.extranonce2_size,
            },
            &stop,
        ) {
            return;
        }
        for job in early_jobs.drain(..) {
            if !send_event(&event_tx, Event::Job(job), &stop) {
                return;
            }
        }

        // authorize
        let aid = next_id;
        next_id += 1;
        if send_json(
            &mut stream,
            &serde_json::json!({"id": aid, "method": "mining.authorize", "params": [address.clone(), password.clone()]}),
        )
        .is_err()
        {
            let _ = stream.shutdown(TcpShutdown::Both);
            if !send_event(&event_tx, Event::Disconnected, &stop) {
                return;
            }
            shutdown.wait_timeout(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        }

        let mut authed = false;
        let auth_start = Instant::now();
        'auth: loop {
            if stop.load(Ordering::Relaxed) {
                disconnect = true;
                break 'auth;
            }
            if auth_start.elapsed() >= HANDSHAKE_TIMEOUT {
                break 'auth;
            }
            match read_line(&mut stream, &mut read_buf) {
                Ok(Some(line)) => match serde_json::from_str::<Value>(&line) {
                    Ok(v) => {
                        if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
                            match method {
                                "mining.set_difficulty" => {
                                    if !apply_set_difficulty(&mut st, &v) {
                                        disconnect = true;
                                        break 'auth;
                                    }
                                }
                                "mining.notify" => match apply_notify(&mut st, &v) {
                                    Ok(job) => {
                                        if !send_event(&event_tx, Event::Job(job), &stop) {
                                            return;
                                        }
                                    }
                                    Err(_) => {
                                        disconnect = true;
                                        break 'auth;
                                    }
                                },
                                "mining.set_extranonce" => {
                                    disconnect = true;
                                    break 'auth;
                                }
                                _ => {}
                            }
                        } else if v.get("id").and_then(|x| x.as_i64()) == Some(aid) {
                            if parse_bool_response(&v).unwrap_or(false) {
                                authed = true;
                            }
                            break 'auth;
                        }
                    }
                    Err(_) => {
                        disconnect = true;
                        break 'auth;
                    }
                },
                Ok(None) => {}
                Err(_) => {
                    disconnect = true;
                    break 'auth;
                }
            }
        }

        if disconnect {
            let _ = stream.shutdown(TcpShutdown::Both);
            if !send_event(&event_tx, Event::Disconnected, &stop) {
                return;
            }
            shutdown.wait_timeout(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        }
        if !authed {
            let _ = stream.shutdown(TcpShutdown::Both);
            if !send_event(&event_tx, Event::Disconnected, &stop) {
                return;
            }
            shutdown.wait_timeout(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
            continue;
        }

        st.authorized = true;
        if !send_event(&event_tx, Event::Authorized, &stop) {
            return;
        }

        // production connected state: nonblocking + readiness
        let _ = stream.set_nonblocking(true);
        #[cfg(unix)]
        let stream_fd = {
            use std::os::unix::io::AsRawFd;
            stream.as_raw_fd()
        };

        disconnect = false;
        'conn: loop {
            if stop.load(Ordering::Relaxed) {
                break 'conn;
            }

            // Pending submission response deadline.
            let now = Instant::now();
            let expired: Vec<i64> = st
                .pending
                .iter()
                .filter(|(_, when)| now.duration_since(**when) >= PENDING_TIMEOUT)
                .map(|(id, _)| *id)
                .collect();
            if !expired.is_empty() {
                for _ in &expired {
                    if !send_event(&event_tx, Event::Unresolved, &stop) {
                        return;
                    }
                }
                for id in expired {
                    st.pending.remove(&id);
                }
                disconnect = true;
            }
            if disconnect {
                break 'conn;
            }

            #[cfg(unix)]
            {
                let legacy = shutdown.is_legacy();
                let mut timeout_ms: i32 = if legacy { 100 } else { -1 };
                if !st.pending.is_empty() {
                    let mut min_remaining = PENDING_TIMEOUT;
                    for when in st.pending.values() {
                        let elapsed = now.duration_since(*when);
                        if elapsed >= PENDING_TIMEOUT {
                            min_remaining = Duration::ZERO;
                        } else {
                            let rem = PENDING_TIMEOUT - elapsed;
                            if rem < min_remaining {
                                min_remaining = rem;
                            }
                        }
                    }
                    let ms = min_remaining.as_millis() as i32;
                    if ms >= 0 && (timeout_ms < 0 || ms < timeout_ms) {
                        timeout_ms = ms;
                    }
                    if legacy && timeout_ms > 100 {
                        timeout_ms = 100;
                    }
                }
                let mut pollfds = [
                    libc::pollfd {
                        fd: stream_fd,
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: wake_fd.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                let pret = unsafe { libc::poll(pollfds.as_mut_ptr(), 2, timeout_ms) };
                if pret < 0 {
                    let err = std::io::Error::last_os_error();
                    if err.kind() != std::io::ErrorKind::Interrupted {
                        break 'conn;
                    }
                } else {
                    if (pollfds[1].revents & libc::POLLIN) != 0 {
                        let mut buf = [0u8; 8];
                        unsafe {
                            libc::read(wake_fd.as_raw_fd(), buf.as_mut_ptr() as *mut _, 8);
                        }
                    }
                }
            }

            #[cfg(not(unix))]
            {
                thread::sleep(Duration::from_millis(10));
            }

            // Drain all complete incoming frames before outgoing share work.
            loop {
                if stop.load(Ordering::Relaxed) {
                    disconnect = true;
                    break;
                }
                match read_line(&mut stream, &mut read_buf) {
                    Ok(Some(line)) => match serde_json::from_str::<Value>(&line) {
                        Ok(v) => {
                            if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
                                match method {
                                    "mining.set_difficulty" => {
                                        if !apply_set_difficulty(&mut st, &v) {
                                            disconnect = true;
                                            break;
                                        }
                                    }
                                    "mining.notify" => match apply_notify(&mut st, &v) {
                                        Ok(job) => {
                                            if !send_event(&event_tx, Event::Job(job), &stop) {
                                                return;
                                            }
                                        }
                                        Err(_) => {
                                            disconnect = true;
                                            break;
                                        }
                                    },
                                    "mining.set_extranonce" => {
                                        disconnect = true;
                                        break;
                                    }
                                    _ => {}
                                }
                            } else if let Some(id) = v.get("id").and_then(|x| x.as_i64()) {
                                if st.pending.remove(&id).is_some() {
                                    if parse_bool_response(&v).unwrap_or(false) {
                                        if !send_event(&event_tx, Event::Accepted, &stop) {
                                            return;
                                        }
                                    } else if !send_event(&event_tx, Event::Rejected, &stop) {
                                        return;
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            disconnect = true;
                            break;
                        }
                    },
                    Ok(None) => break,
                    Err(_) => {
                        disconnect = true;
                        break;
                    }
                }
            }
            if disconnect {
                break 'conn;
            }

            // Drain a bounded amount of queued share work.
            for _ in 0..SHARE_DRAIN_LIMIT {
                if st.pending.len() >= MAX_PENDING {
                    break;
                }
                match share_rx.try_recv() {
                    Ok(share) => {
                        if share.connection_generation != current_conn {
                            if !send_event(&event_tx, Event::Discarded, &stop) {
                                return;
                            }
                            continue;
                        }
                        if !st.authorized {
                            if !send_event(&event_tx, Event::Discarded, &stop) {
                                return;
                            }
                            continue;
                        }
                        let job = st
                            .jobs
                            .iter()
                            .find(|j| {
                                j.job_generation == share.job_generation && j.job_id == share.job_id
                            })
                            .cloned();
                        let Some(job) = job else {
                            if !send_event(&event_tx, Event::Discarded, &stop) {
                                return;
                            }
                            continue;
                        };
                        if job.ntime != share.ntime {
                            if !send_event(&event_tx, Event::Discarded, &stop) {
                                return;
                            }
                            continue;
                        }
                        if !valid_extranonce2(&share.extranonce2, st.extranonce2_size) {
                            if !send_event(&event_tx, Event::Discarded, &stop) {
                                return;
                            }
                            continue;
                        }
                        let id = next_id;
                        next_id += 1;
                        let msg = serde_json::json!({
                            "id": id,
                            "method": "mining.submit",
                            "params": [
                                address.clone(),
                                share.job_id.clone(),
                                share.extranonce2.clone(),
                                format!("{:08x}", share.ntime),
                                format!("{:08x}", share.nonce.swap_bytes())
                            ]
                        });
                        if send_json(&mut stream, &msg).is_err() {
                            disconnect = true;
                            break;
                        }
                        st.pending.insert(id, Instant::now());
                        if !send_event(&event_tx, Event::Submitted, &stop) {
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break 'conn,
                }
            }
            if disconnect {
                break 'conn;
            }
        }

        let _ = stream.shutdown(TcpShutdown::Both);

        // Resolve every pending submission from this connection once.
        let pending_ids: Vec<i64> = st.pending.keys().copied().collect();
        for _ in pending_ids {
            if !send_event(&event_tx, Event::Unresolved, &stop) {
                return;
            }
        }
        st.pending.clear();

        if !send_event(&event_tx, Event::Disconnected, &stop) {
            return;
        }

        if !stop.load(Ordering::Relaxed) {
            shutdown.wait_timeout(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    }
}

fn apply_set_difficulty(st: &mut State, v: &Value) -> bool {
    let params = match v.get("params").and_then(|p| p.as_array()) {
        Some(p) => p,
        None => return false,
    };
    if params.len() != 1 {
        return false;
    }
    let d = match params[0].as_f64() {
        Some(d) => d,
        None => return false,
    };
    match gbtc_core::target_from_difficulty(d) {
        Ok(t) => {
            st.staged_difficulty = d;
            st.staged_target = t;
            st.staged_target_gen = st.staged_target_gen.wrapping_add(1);
            true
        }
        Err(_) => false,
    }
}

fn apply_notify(st: &mut State, v: &Value) -> Result<MiningJob, ()> {
    let params = match v.get("params").and_then(|p| p.as_array()) {
        Some(p) if p.len() == 9 => p,
        _ => return Err(()),
    };
    let job_id = match params[0].as_str() {
        Some(s) if !s.is_empty() && s.len() <= 63 => s,
        _ => return Err(()),
    };
    let prev_hex = match params[1].as_str() {
        Some(s) => s,
        None => return Err(()),
    };
    let prev = match gbtc_core::job::decode_hex(prev_hex) {
        Ok(b) if b.len() == 32 => b,
        _ => return Err(()),
    };
    let mut prevhash = [0u8; 32];
    prevhash.copy_from_slice(&prev);

    let cb1_hex = match params[2].as_str() {
        Some(s) => s,
        None => return Err(()),
    };
    let cb1 = match gbtc_core::job::decode_hex(cb1_hex) {
        Ok(b) if b.len() <= 511 => b,
        _ => return Err(()),
    };

    let cb2_hex = match params[3].as_str() {
        Some(s) => s,
        None => return Err(()),
    };
    let cb2 = match gbtc_core::job::decode_hex(cb2_hex) {
        Ok(b) if b.len() <= 511 => b,
        _ => return Err(()),
    };

    let branches = match params[4].as_array() {
        Some(a) if a.len() <= 32 => a,
        _ => return Err(()),
    };
    let mut merkle: Vec<[u8; 32]> = Vec::with_capacity(branches.len());
    for b in branches {
        let h = match b.as_str() {
            Some(s) => s,
            None => return Err(()),
        };
        let bytes = match gbtc_core::job::decode_hex(h) {
            Ok(b) if b.len() == 32 => b,
            _ => return Err(()),
        };
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        merkle.push(arr);
    }

    let version_hex = match params[5].as_str() {
        Some(s) if s.len() == 8 => s,
        _ => return Err(()),
    };
    let version_bytes = match gbtc_core::job::decode_hex(version_hex) {
        Ok(b) if b.len() == 4 => b,
        _ => return Err(()),
    };
    let version = u32::from_be_bytes([
        version_bytes[0],
        version_bytes[1],
        version_bytes[2],
        version_bytes[3],
    ]);

    let nbits_hex = match params[6].as_str() {
        Some(s) if s.len() == 8 => s,
        _ => return Err(()),
    };
    let nbits_bytes = match gbtc_core::job::decode_hex(nbits_hex) {
        Ok(b) if b.len() == 4 => b,
        _ => return Err(()),
    };
    let nbits = u32::from_be_bytes([
        nbits_bytes[0],
        nbits_bytes[1],
        nbits_bytes[2],
        nbits_bytes[3],
    ]);

    let exponent = nbits >> 24;
    let mantissa = nbits & 0x007fffff;
    if mantissa == 0
        || nbits & 0x00800000 != 0
        || exponent > 34
        || (mantissa > 0xff && exponent > 33)
        || (mantissa > 0xffff && exponent > 32)
        || (exponent <= 3 && mantissa >> (8 * (3 - exponent)) == 0)
    {
        return Err(());
    }

    let ntime_hex = match params[7].as_str() {
        Some(s) if s.len() == 8 => s,
        _ => return Err(()),
    };
    let ntime_bytes = match gbtc_core::job::decode_hex(ntime_hex) {
        Ok(b) if b.len() == 4 => b,
        _ => return Err(()),
    };
    let ntime = u32::from_be_bytes([
        ntime_bytes[0],
        ntime_bytes[1],
        ntime_bytes[2],
        ntime_bytes[3],
    ]);

    let clean = match params[8].as_bool() {
        Some(b) => b,
        None => return Err(()),
    };

    let job = MiningJob {
        connection_generation: st.conn_gen,
        job_generation: st.job_gen + 1,
        target_generation: st.staged_target_gen,
        job_id: job_id.to_string(),
        prevhash,
        coinb1: cb1,
        coinb2: cb2,
        merkle,
        version,
        nbits,
        ntime,
        target: st.staged_target,
        difficulty: st.staged_difficulty,
        clean,
    };

    // Repeated identical work retains its nonce coordinate and generation.
    if let Some(existing) = st.jobs.iter().find(|j| {
        j.job_id == job.job_id
            && j.prevhash == job.prevhash
            && j.coinb1 == job.coinb1
            && j.coinb2 == job.coinb2
            && j.merkle == job.merkle
            && j.version == job.version
            && j.nbits == job.nbits
            && j.ntime == job.ntime
            && j.target == job.target
    }) {
        let mut repeated = existing.clone();
        repeated.clean = clean;
        if clean {
            st.jobs
                .retain(|j| j.job_generation == repeated.job_generation);
        }
        return Ok(repeated);
    }
    st.job_gen += 1;
    if clean {
        st.jobs.clear();
    }
    if st.jobs.len() >= MAX_JOBS {
        st.jobs.remove(0);
    }
    st.jobs.push(job.clone());
    Ok(job)
}

fn parse_subscribe(v: &Value, en1: &mut Vec<u8>, en2_size: &mut usize) -> bool {
    let result = match v.get("result").and_then(|r| r.as_array()) {
        Some(a) if a.len() >= 3 => a,
        _ => return false,
    };
    let en1_hex = match result[1].as_str() {
        Some(s) => s,
        None => return false,
    };
    let en2 = match result[2].as_u64() {
        Some(n) => n,
        None => return false,
    };
    if !(1..=8).contains(&en2) {
        return false;
    }
    if en1_hex.len() < 2 || en1_hex.len() > 32 || en1_hex.len() % 2 != 0 {
        return false;
    }
    if !en1_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    let bytes = match gbtc_core::job::decode_hex(en1_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };
    if bytes.len() > 16 {
        return false;
    }
    if let Some(err) = v.get("error") {
        if !err.is_null() {
            return false;
        }
    }
    *en1 = bytes;
    *en2_size = en2 as usize;
    true
}

fn parse_bool_response(v: &Value) -> Option<bool> {
    let result = v.get("result")?;
    if !result.is_boolean() {
        return None;
    }
    let accepted = result.as_bool().unwrap();
    if accepted {
        if let Some(err) = v.get("error") {
            if !err.is_null() {
                return None;
            }
        }
    }
    Some(accepted)
}

fn connect_url(url: &str, stop: &AtomicBool) -> Result<TcpStream, String> {
    let (host, port) = parse_url(url)?;
    if stop.load(Ordering::Relaxed) {
        return Err("stopped".to_string());
    }
    let addrs = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| "resolve failed".to_string())?;
    if stop.load(Ordering::Relaxed) {
        return Err("stopped".to_string());
    }
    for addr in addrs {
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".to_string());
        }
        if let Ok(stream) = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            let _ = stream.set_nodelay(true);
            return Ok(stream);
        }
    }
    Err("connect failed".to_string())
}

fn parse_url(url: &str) -> Result<(String, u16), String> {
    let s = url.trim();
    let mut rest = s;
    if let Some(idx) = s.find("://") {
        let scheme = &s[..idx];
        if scheme != "stratum+tcp" {
            return Err("unsupported scheme".to_string());
        }
        rest = &s[idx + 3..];
    }

    if rest.is_empty() {
        return Err("missing host".to_string());
    }
    if rest.contains('@') {
        return Err("credentials are not allowed".to_string());
    }
    if rest.contains('/') || rest.contains('?') || rest.contains('#') {
        return Err("path is not allowed".to_string());
    }

    let (host, port_text) = if let Some(stripped) = rest.strip_prefix('[') {
        let close = stripped
            .find(']')
            .ok_or_else(|| "invalid host".to_string())?;
        let host = &stripped[..close];
        let after = &stripped[close + 1..];
        if !after.starts_with(':') {
            return Err("missing port".to_string());
        }
        (host.to_string(), &after[1..])
    } else {
        let colon = rest.rfind(':').ok_or_else(|| "missing port".to_string())?;
        (rest[..colon].to_string(), &rest[colon + 1..])
    };

    if host.is_empty()
        || host.contains('[')
        || host.contains(']')
        || host.chars().any(char::is_whitespace)
    {
        return Err("invalid host".to_string());
    }
    let port: u16 = port_text.parse().map_err(|_| "invalid port".to_string())?;
    if port == 0 {
        return Err("port must be nonzero".to_string());
    }
    Ok((host, port))
}

fn read_line(stream: &mut TcpStream, buf: &mut Vec<u8>) -> Result<Option<String>, String> {
    if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        if pos > LINE_LIMIT {
            return Err("oversize line".to_string());
        }
        let mut line_bytes = buf[..pos].to_vec();
        buf.drain(..pos + 1);
        if line_bytes.last() == Some(&b'\r') {
            line_bytes.pop();
        }
        let s = String::from_utf8(line_bytes).map_err(|_| "invalid utf8".to_string())?;
        return Ok(Some(s));
    }

    if buf.len() > LINE_LIMIT {
        return Err("oversize line".to_string());
    }

    let mut tmp = [0u8; 4096];
    match stream.read(&mut tmp) {
        Ok(0) => Err("eof".to_string()),
        Ok(n) => {
            buf.extend_from_slice(&tmp[..n]);
            if buf.len() > LINE_LIMIT {
                return Err("oversize line".to_string());
            }
            if buf.contains(&b'\n') {
                read_line(stream, buf)
            } else {
                Ok(None)
            }
        }
        Err(e)
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut
                || e.kind() == std::io::ErrorKind::Interrupted =>
        {
            Ok(None)
        }
        Err(e) => Err(e.to_string()),
    }
}

fn send_json(stream: &mut TcpStream, value: &Value) -> Result<(), String> {
    let mut s = serde_json::to_string(value).map_err(|e| e.to_string())?;
    s.push('\n');
    stream.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    Ok(())
}

fn send_event(tx: &SyncSender<Event>, mut event: Event, stop: &AtomicBool) -> bool {
    loop {
        match tx.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Full(e)) => {
                if stop.load(Ordering::Relaxed) {
                    return false;
                }
                event = e;
                thread::sleep(Duration::from_millis(1));
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
}

fn valid_extranonce2(value: &str, width: usize) -> bool {
    if value.len() != width.saturating_mul(2) {
        return false;
    }
    value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod framing_tests {
    use super::*;
    #[test]
    fn newly_read_frames_are_visible_before_submission() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        server.write_all(b"first\nsecond\n").unwrap();
        let mut buffer = Vec::new();
        assert_eq!(
            read_line(&mut client, &mut buffer).unwrap().as_deref(),
            Some("first")
        );
        assert_eq!(
            read_line(&mut client, &mut buffer).unwrap().as_deref(),
            Some("second")
        );
    }
}
