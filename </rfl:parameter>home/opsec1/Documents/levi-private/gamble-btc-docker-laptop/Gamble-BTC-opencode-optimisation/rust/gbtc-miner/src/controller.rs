use gbtc_core::backend::{BackendError, ComputeBackend, WorkBatch};
use gbtc_core::job::MiningJob;
use gbtc_core::work::{raw_hash_meets_target, work_hash};
use gbtc_http::Status;
use gbtc_stratum::{Event, Share};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const JOB_REGISTRY_CAPACITY: usize = 64;
pub const DEDUP_PER_JOB_CAPACITY: usize = 4096;
pub const SUBMISSION_QUEUE_CAPACITY: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerError {
    pub message: String,
}

impl ControllerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ControllerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ControllerError {}

impl From<BackendError> for ControllerError {
    fn from(e: BackendError) -> Self {
        ControllerError::new(e.to_string())
    }
}

impl From<String> for ControllerError {
    fn from(s: String) -> Self {
        ControllerError::new(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonceRange {
    pub base: u32,
    pub count: u32,
}

impl NonceRange {
    pub fn end(&self) -> u64 {
        self.base as u64 + self.count as u64
    }
}

pub fn nonce_range_for_cursor(cursor: u64, batch_count: u32) -> Option<NonceRange> {
    if cursor >= (1u64 << 32) {
        return None;
    }
    let remaining = (1u64 << 32) - cursor;
    let count = (batch_count as u64).min(remaining);
    if count == 0 {
        return None;
    }
    Some(NonceRange {
        base: cursor as u32,
        count: count as u32,
    })
}

pub fn increment_extranonce2(buf: &mut [u8]) -> Result<(), String> {
    for i in 0..buf.len() {
        if buf[i] != 0xff {
            buf[i] = buf[i].wrapping_add(1);
            for byte in &mut buf[..i] {
                *byte = 0;
            }
            return Ok(());
        }
    }
    Err("extranonce2 exhausted".to_string())
}

pub struct Controller<B> {
    backend: B,
    network_rx: Receiver<Event>,
    share_tx: SyncSender<Share>,
    status: Option<Arc<Mutex<Status>>>,
    stop: Arc<AtomicBool>,
    batch_nonces: u32,

    connection_generation: Option<u64>,
    authorized: bool,
    extranonce1: Vec<u8>,
    extranonce2_size: usize,

    jobs: Vec<MiningJob>,
    current_job_generation: Option<u64>,
    cursor: u64,
    extranonce2: Vec<u8>,
    extranonce_exhausted: bool,

    dedup: HashMap<u64, HashSet<(Vec<u8>, u32, u32)>>,
    discarded: u64,
    submitted: u64,
    accepted: u64,
    rejected: u64,
    unresolved: u64,
    lifetime_hashes: u64,
}

impl<B: ComputeBackend> Controller<B> {
    pub fn new(
        backend: B,
        network_rx: Receiver<Event>,
        share_tx: SyncSender<Share>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        Self::with_options(backend, network_rx, share_tx, None, stop, 1u32 << 24)
    }

    pub fn with_status(
        backend: B,
        network_rx: Receiver<Event>,
        share_tx: SyncSender<Share>,
        status: Arc<Mutex<Status>>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        Self::with_options(
            backend,
            network_rx,
            share_tx,
            Some(status),
            stop,
            1u32 << 24,
        )
    }

    pub fn with_options(
        backend: B,
        network_rx: Receiver<Event>,
        share_tx: SyncSender<Share>,
        status: Option<Arc<Mutex<Status>>>,
        stop: Arc<AtomicBool>,
        batch_nonces: u32,
    ) -> Self {
        Self {
            backend,
            network_rx,
            share_tx,
            status,
            stop,
            batch_nonces,
            connection_generation: None,
            authorized: false,
            extranonce1: Vec::new(),
            extranonce2_size: 0,
            jobs: Vec::new(),
            current_job_generation: None,
            cursor: 0,
            extranonce2: Vec::new(),
            extranonce_exhausted: false,
            dedup: HashMap::new(),
            discarded: 0,
            submitted: 0,
            accepted: 0,
            rejected: 0,
            unresolved: 0,
            lifetime_hashes: 0,
        }
    }

    pub fn tick(&mut self) -> Result<bool, String> {
        if self.stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        self.drain_events().map_err(|e| e.to_string())?;
        if !self.can_work() {
            if let Some(status) = &self.status {
                let mut g = status.lock().unwrap();
                g.data["mh"] = serde_json::json!(0.0);
            }
            return Ok(false);
        }
        let job_gen = match self.current_job_generation {
            Some(g) => g,
            None => return Ok(false),
        };
        let job = match self.jobs.iter().find(|j| j.job_generation == job_gen).cloned() {
            Some(j) => j,
            None => {
                self.current_job_generation = None;
                return Ok(false);
            }
        };
        if self.extranonce_exhausted {
            return Ok(false);
        }
        let range = match nonce_range_for_cursor(self.cursor, self.batch_nonces) {
            Some(r) => r,
            None => {
                if self.extranonce2.is_empty() {
                    self.extranonce2 = vec![0u8; self.extranonce2_size.clamp(1, 8)];
                }
                match increment_extranonce2(&mut self.extranonce2) {
                    Ok(()) => self.cursor = 0,
                    Err(_) => self.extranonce_exhausted = true,
                }
                return Ok(true);
            }
        };

        let en1 = self.extranonce1.clone();
        let mut en2 = self.extranonce2.clone();
        if en2.len() != self.extranonce2_size && self.extranonce2_size > 0 {
            let mut v = vec![0u8; self.extranonce2_size];
            let n = en2.len().min(v.len());
            v[..n].copy_from_slice(&en2[..n]);
            self.extranonce2 = v.clone();
            en2 = v;
        }
        let en2 = self.extranonce2.clone();
        let work = job
            .work(&en1, &en2, range.base, range.count)
            .map_err(|e| e.to_string())?;

        let batch_base = range.base;
        let batch_count = range.count;
        self.cursor += range.count as u64;

        let batch_extranonce2 = en2.clone();
        let batch_job = job.clone();
        let batch_conn = self.connection_generation.unwrap_or(0);

        let batch_start = Instant::now();
        let result = match self.backend.run_batch(&work) {
            Ok(r) => r,
            Err(e) => {
                if self.stop.load(Ordering::Relaxed) {
                    return Ok(false);
                }
                return Err(e.to_string());
            }
        };

        self.drain_events().map_err(|e| e.to_string())?;

        if result.unique_hashes != batch_count as u64 {
            if self.stop.load(Ordering::Relaxed) {
                return Ok(false);
            }
            return Err(format!(
                "backend contract violation: unique_hashes {} != requested {}",
                result.unique_hashes, batch_count
            ));
        }
        for &nonce in &result.candidates {
            let end = batch_base as u64 + batch_count as u64;
            if (nonce as u64) < batch_base as u64 || (nonce as u64) >= end {
                if self.stop.load(Ordering::Relaxed) {
                    return Ok(false);
                }
                return Err("backend contract violation: candidate out of range".to_string());
            }
        }

        for &nonce in &result.candidates {
            let hash = work_hash(&work, nonce);
            if !raw_hash_meets_target(&hash, &batch_job.target) {
                self.discarded += 1;
                self.update_status_discarded();
                continue;
            }
            let job_valid = self.jobs.iter().any(|j| {
                j.job_generation == batch_job.job_generation
                    && j.connection_generation == batch_conn
            });
            let submission_valid = self.is_submission_valid(batch_job.job_generation);
            if !job_valid || !submission_valid {
                self.discarded += 1;
                self.update_status_discarded();
                continue;
            }
            let key = (batch_extranonce2.clone(), batch_job.ntime, nonce);
            let set = self.dedup.entry(batch_job.job_generation).or_default();
            if set.len() >= DEDUP_PER_JOB_CAPACITY {
                if self.stop.load(Ordering::Relaxed) {
                    return Ok(false);
                }
                return Err("dedup capacity exceeded".to_string());
            }
            if !set.insert(key) {
                self.discarded += 1;
                self.update_status_discarded();
                continue;
            }

            let share = Share {
                connection_generation: batch_conn,
                job_generation: batch_job.job_generation,
                job_id: batch_job.job_id.clone(),
                extranonce2: gbtc_core::job::encode_hex(&batch_extranonce2),
                ntime: batch_job.ntime,
                nonce,
            };

            match self.share_tx.try_send(share) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    self.discarded += 1;
                    self.update_status_discarded();
                }
                Err(TrySendError::Disconnected(_)) => {
                    return Err("share channel disconnected".to_string());
                }
            }
        }

        if self.current_job_generation == Some(batch_job.job_generation)
            && self.extranonce2 == batch_extranonce2
            && self.cursor >= (1u64 << 32)
        {
            self.cursor = 1u64 << 32;
            if self.extranonce2.is_empty() {
                self.extranonce2 = vec![0u8; self.extranonce2_size.clamp(1, 8)];
            }
            match increment_extranonce2(&mut self.extranonce2) {
                Ok(()) => self.cursor = 0,
                Err(_) => self.extranonce_exhausted = true,
            }
        }

        let elapsed = batch_start.elapsed().as_secs_f64();
        self.lifetime_hashes += result.unique_hashes;
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.update_hashes(result.unique_hashes, elapsed);
        }
        self.update_status_hashes(result.unique_hashes);

        Ok(true)
    }

    pub fn run(&mut self) -> Result<(), String> {
        let mut idle_sleep = Duration::from_millis(10);
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            match self.tick() {
                Ok(true) => {
                    idle_sleep = Duration::from_millis(10);
                }
                Ok(false) => {
                    thread::sleep(idle_sleep);
                    idle_sleep = (idle_sleep * 2).min(Duration::from_millis(100));
                }
                Err(e) => {
                    if self.stop.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    return Err(e);
                }
            }
        }
    }

    pub fn run_detailed(&mut self) -> Result<(), ControllerError> {
        self.run().map_err(ControllerError::new)
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    pub fn can_work(&self) -> bool {
        if self.connection_generation.is_none() || !self.authorized {
            return false;
        }
        if self.jobs.is_empty() {
            return false;
        }
        if self.current_job_generation.is_none() {
            return false;
        }
        true
    }

    fn is_submission_valid(&self, job_gen: u64) -> bool {
        self.jobs.iter().any(|j| j.job_generation == job_gen)
    }

    pub fn drain_events(&mut self) -> Result<(), ControllerError> {
        loop {
            match self.network_rx.try_recv() {
                Ok(ev) => self.handle_event(ev)?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(ControllerError::new("network channel disconnected"));
                }
            }
        }
        Ok(())
    }

    pub fn handle_event(&mut self, ev: Event) -> Result<(), ControllerError> {
        match ev {
            Event::Connected {
                connection_generation,
                extranonce1,
                extranonce2_size,
            } => {
                self.connection_generation = Some(connection_generation);
                self.authorized = false;
                self.extranonce1 = extranonce1;
                self.extranonce2_size = extranonce2_size;
                self.extranonce2 = vec![0u8; extranonce2_size.clamp(1, 8)];
                self.cursor = 0;
                self.extranonce_exhausted = false;
                self.jobs
                    .retain(|j| j.connection_generation == connection_generation);
                let valid_gens: HashSet<u64> =
                    self.jobs.iter().map(|j| j.job_generation).collect();
                self.dedup.retain(|g, _| valid_gens.contains(g));
                if !self
                    .jobs
                    .iter()
                    .any(|j| Some(j.job_generation) == self.current_job_generation)
                {
                    self.current_job_generation = None;
                }
                self.select_newest_job(connection_generation);
                self.update_status_connection();
            }
            Event::Job(job) => {
                let retain = match self.connection_generation {
                    Some(conn) => job.connection_generation == conn,
                    None => true,
                };
                if !retain {
                    return Ok(());
                }
                let duplicate = self.jobs.iter().any(|j| {
                    j.job_id == job.job_id
                        && j.connection_generation == job.connection_generation
                        && j.ntime == job.ntime
                        && j.target == job.target
                });
                if duplicate {
                    return Ok(());
                }
                let clean = job.clean;
                if clean {
                    self.jobs
                        .retain(|j| j.job_generation >= job.job_generation);
                    let valid_gens: HashSet<u64> =
                        self.jobs.iter().map(|j| j.job_generation).collect();
                    self.dedup.retain(|g, _| valid_gens.contains(g));
                }
                if self.jobs.len() >= JOB_REGISTRY_CAPACITY {
                    if let Some(old) = self.jobs.first().cloned() {
                        self.jobs.remove(0);
                        self.dedup.remove(&old.job_generation);
                    }
                }
                self.jobs.push(job.clone());
                self.dedup.entry(job.job_generation).or_default();
                if self.connection_generation == Some(job.connection_generation) {
                    self.current_job_generation = Some(job.job_generation);
                    self.cursor = 0;
                    self.extranonce2 = vec![0u8; self.extranonce2_size.clamp(1, 8)];
                    self.extranonce_exhausted = false;
                }
                let _ = clean;
                self.update_status_job(&job);
            }
            Event::Authorized => {
                if self.connection_generation.is_some() {
                    self.authorized = true;
                }
                if self.current_job_generation.is_none() {
                    if let Some(conn) = self.connection_generation {
                        self.select_newest_job(conn);
                    }
                }
            }
            Event::Submitted => {
                self.submitted += 1;
                self.update_status_submitted();
            }
            Event::Accepted => {
                self.accepted += 1;
                self.update_status_accepted();
            }
            Event::Rejected => {
                self.rejected += 1;
                self.update_status_rejected();
            }
            Event::Discarded => {
                self.discarded += 1;
                self.update_status_discarded();
            }
            Event::Unresolved => {
                self.unresolved += 1;
                self.update_status_unresolved();
            }
            Event::Disconnected => {
                self.connection_generation = None;
                self.authorized = false;
                self.jobs.clear();
                self.dedup.clear();
                self.current_job_generation = None;
                self.cursor = 0;
                self.extranonce_exhausted = false;
                if let Some(status) = &self.status {
                    let mut g = status.lock().unwrap();
                    g.data["mh"] = serde_json::json!(0.0);
                }
            }
        }
        Ok(())
    }

    fn select_newest_job(&mut self, conn: u64) {
        if let Some(job) = self
            .jobs
            .iter()
            .filter(|j| j.connection_generation == conn)
            .max_by_key(|j| j.job_generation)
            .cloned()
        {
            self.current_job_generation = Some(job.job_generation);
            self.cursor = 0;
            self.extranonce2 = vec![0u8; self.extranonce2_size.clamp(1, 8)];
            self.extranonce_exhausted = false;
        }
    }

    fn update_status_hashes(&self, hashes: u64) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["lifetime_hashes"] = serde_json::json!(self.lifetime_hashes);
            let _ = hashes;
        }
    }

    fn update_status_submitted(&self) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["submitted"] = serde_json::json!(self.submitted);
        }
    }
    fn update_status_accepted(&self) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["accepted"] = serde_json::json!(self.accepted);
        }
    }
    fn update_status_rejected(&self) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["rejected"] = serde_json::json!(self.rejected);
        }
    }
    fn update_status_discarded(&self) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["discarded"] = serde_json::json!(self.discarded);
        }
    }
    fn update_status_unresolved(&self) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["unresolved"] = serde_json::json!(self.unresolved);
        }
    }
    fn update_status_connection(&self) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["pool"] = serde_json::json!(g.data.get("pool").cloned().unwrap_or_default());
        }
    }
    fn update_status_job(&self, job: &MiningJob) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["job"] = serde_json::json!(job.job_id.clone());
            g.data["ntime"] = serde_json::json!(job.ntime);
            g.data["nbits"] = serde_json::json!(job.nbits);
            g.data["merkle_branches"] = serde_json::json!(job.merkle.len());
            g.data["diff"] = serde_json::json!(job.difficulty);
        }
    }
}

pub fn run_controller<B: ComputeBackend>(
    backend: B,
    network_rx: Receiver<Event>,
    share_tx: SyncSender<Share>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut c = Controller::new(backend, network_rx, share_tx, stop);
    c.run()
}

pub fn run_controller_with_status<B: ComputeBackend>(
    backend: B,
    network_rx: Receiver<Event>,
    share_tx: SyncSender<Share>,
    status: Arc<Mutex<Status>>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut c = Controller::with_status(backend, network_rx, share_tx, status, stop);
    c.run()
}

pub fn make_synthetic_work_batch(batch_nonces: u32) -> WorkBatch {
    let mut w = WorkBatch {
        midstate: [0; 8],
        tail3: [0; 3],
        target: [0; 8],
        nonce_base: 0,
        nonce_count: batch_nonces,
    };
    gbtc_core::make_synthetic_work(&mut w, batch_nonces);
    w
}

impl From<ControllerError> for String {
    fn from(e: ControllerError) -> Self {
        e.to_string()
    }
}
