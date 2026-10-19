use gbtc_core::backend::{BackendError, BatchTicket, ComputeBackend, WorkBatch};
use gbtc_core::job::MiningJob;
use gbtc_core::work::{build_header_midstate, raw_hash_meets_target, work_hash};
use gbtc_http::Status;
use gbtc_stratum::{Event, Share, ShareSender};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const JOB_REGISTRY_CAPACITY: usize = 64;
pub const DEDUP_PER_JOB_CAPACITY: usize = 4096;
pub const SUBMISSION_QUEUE_CAPACITY: usize = 128;
pub const PREPARED_CACHE_CAPACITY: usize = 64;

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

#[derive(Clone)]
struct OutstandingBatch {
    ticket: BatchTicket,
    job: Arc<MiningJob>,
    conn: u64,
    extranonce2: Vec<u8>,
    range: NonceRange,
    work: WorkBatch,
    estimated_duration: Duration,
    submitted_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct WorkIdentity {
    conn: u64,
    job_gen: u64,
    target_generation: u64,
    job_ptr: usize,
    extranonce1: Vec<u8>,
    extranonce2: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
struct PreparedWork {
    midstate: [u32; 8],
    tail3: [u32; 3],
}

pub struct Controller<B> {
    backend: B,
    network_rx: Receiver<Event>,
    share_tx: ShareSender,
    status: Option<Arc<Mutex<Status>>>,
    stop: Arc<AtomicBool>,
    batch_nonces: u32,
    queue_budget: Duration,

    connection_generation: Option<u64>,
    authorized: bool,
    extranonce1: Vec<u8>,
    extranonce2_size: usize,

    jobs: Vec<Arc<MiningJob>>,
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

    outstanding: Vec<OutstandingBatch>,
    prepared_cache: HashMap<WorkIdentity, PreparedWork>,
    last_throughput: Option<f64>,
    rate_window_start: Instant,
    rate_window_hashes: u64,
    prep_wall_ns: u64,
    validation_wall_ns: u64,
    status_wall_ns: u64,
}

impl<B: ComputeBackend> Controller<B> {
    pub fn new(
        backend: B,
        network_rx: Receiver<Event>,
        share_tx: impl Into<ShareSender>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        Self::with_options(backend, network_rx, share_tx, None, stop, 1u32 << 24)
    }

    pub fn with_status(
        backend: B,
        network_rx: Receiver<Event>,
        share_tx: impl Into<ShareSender>,
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
        share_tx: impl Into<ShareSender>,
        status: Option<Arc<Mutex<Status>>>,
        stop: Arc<AtomicBool>,
        batch_nonces: u32,
    ) -> Self {
        if let Some(status) = &status {
            let mut g = status.lock().unwrap();
            for key in [
                "submitted",
                "accepted",
                "rejected",
                "discarded",
                "unresolved",
            ] {
                if g.data.get(key).is_none() {
                    g.data[key] = serde_json::json!(0u64);
                }
            }
            if g.data.get("lifetime_hashes").is_none() {
                g.data["lifetime_hashes"] = serde_json::json!(0u64);
            }
            for key in ["prep_wall_ns", "validation_wall_ns", "status_wall_ns"] {
                if g.data.get(key).is_none() {
                    g.data[key] = serde_json::json!(0u64);
                }
            }
            if g.data.get("backend_telemetry").is_none() {
                g.data["backend_telemetry"] = serde_json::json!({});
            }
            if g.data.get("diff").is_none() {
                g.data["diff"] = serde_json::json!(1.0);
            }
            if g.data.get("mh").is_none() {
                g.data["mh"] = serde_json::json!(0.0);
            }
        }
        Self {
            backend,
            network_rx,
            share_tx: share_tx.into(),
            status,
            stop,
            batch_nonces,
            queue_budget: Duration::from_millis(200),
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
            outstanding: Vec::new(),
            prepared_cache: HashMap::new(),
            last_throughput: None,
            rate_window_start: Instant::now(),
            rate_window_hashes: 0,
            prep_wall_ns: 0,
            validation_wall_ns: 0,
            status_wall_ns: 0,
        }
    }

    pub fn with_queue_budget(mut self, budget: Duration) -> Self {
        self.queue_budget = budget;
        self
    }

    pub fn tick(&mut self) -> Result<bool, String> {
        if self.stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        if self.backend.queue_capacity() <= 1 {
            self.tick_sync()
        } else {
            self.pipeline_tick()
        }
    }

    fn tick_sync(&mut self) -> Result<bool, String> {
        self.drain_events().map_err(|e| e.to_string())?;
        if !self.can_work() {
            self.update_status_no_work();
            return Ok(false);
        }
        let Some(job_gen) = self.current_job_generation else {
            return Ok(false);
        };
        let job = match self
            .jobs
            .iter()
            .find(|j| j.job_generation == job_gen)
            .cloned()
        {
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
                    Ok(()) => {
                        self.cursor = 0;
                    }
                    Err(_) => {
                        self.extranonce_exhausted = true;
                    }
                }
                return Ok(false);
            }
        };

        let en1 = self.extranonce1.clone();
        let en2_before = self.extranonce2.clone();
        if en2_before.len() != self.extranonce2_size && self.extranonce2_size > 0 {
            let mut v = vec![0u8; self.extranonce2_size];
            let n = en2_before.len().min(v.len());
            v[..n].copy_from_slice(&en2_before[..n]);
            self.extranonce2 = v;
        }
        let en2 = self.extranonce2.clone();
        let prep_start = Instant::now();
        let work = job.work(&en1, &en2, range.base, range.count)?;
        self.prep_wall_ns = self
            .prep_wall_ns
            .saturating_add(prep_start.elapsed().as_nanos() as u64);

        self.cursor += range.count as u64;
        let batch_start = Instant::now();
        let batch_extranonce2 = en2.clone();
        let batch_job = job.clone();
        let batch_conn = self.connection_generation.unwrap_or(0);
        let batch_job_gen = batch_job.job_generation;

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

        if result.unique_hashes != range.count as u64 {
            return Err("backend unique_hashes does not match requested count".to_string());
        }
        let validation_start = Instant::now();
        let range_end = range.base as u64 + range.count as u64;
        for &nonce in &result.candidates {
            if (nonce as u64) < range.base as u64 || (nonce as u64) >= range_end {
                return Err("backend candidate out of range".to_string());
            }
        }

        for &nonce in &result.candidates {
            let hash = work_hash(&work, nonce);
            if !raw_hash_meets_target(&hash, &batch_job.target) {
                self.discarded += 1;
                self.update_status_discarded();
                continue;
            }
            let job_valid = self.authorized
                && self.connection_generation == Some(batch_conn)
                && self.jobs.iter().any(|j| {
                    j.job_generation == batch_job.job_generation
                        && j.connection_generation == batch_conn
                        && j.target_generation == batch_job.target_generation
                });
            let submission_valid = self.is_submission_valid(
                batch_job.job_generation,
                batch_conn,
                batch_job.target_generation,
            );
            if !job_valid || !submission_valid {
                self.discarded += 1;
                self.update_status_discarded();
                continue;
            }
            let key = (batch_extranonce2.clone(), batch_job.ntime, nonce);
            let set = self.dedup.entry(batch_job.job_generation).or_default();
            if set.len() >= DEDUP_PER_JOB_CAPACITY {
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

        self.validation_wall_ns = self
            .validation_wall_ns
            .saturating_add(validation_start.elapsed().as_nanos() as u64);

        let job_changed = self.current_job_generation != Some(batch_job_gen);
        if !job_changed && self.cursor >= (1u64 << 32) {
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
        let status_start = Instant::now();
        self.lifetime_hashes += result.unique_hashes;
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.update_hashes(result.unique_hashes, elapsed);
        }
        self.update_status_hashes(result.unique_hashes);
        self.update_status_telemetry();
        self.status_wall_ns = self
            .status_wall_ns
            .saturating_add(status_start.elapsed().as_nanos() as u64);

        Ok(true)
    }

    fn pipeline_tick(&mut self) -> Result<bool, String> {
        if self.stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        self.drain_events().map_err(|e| e.to_string())?;

        let mut progressed = false;

        if !self.outstanding.is_empty()
            && (self.outstanding.len() >= self.backend.queue_capacity() || !self.can_work())
        {
            progressed |= self.retire_one()?;
        }

        if self.can_work() {
            progressed |= self.fill_free_slots()?;
        }

        if !progressed && !self.outstanding.is_empty() {
            progressed |= self.retire_one()?;
        }

        if !progressed && !self.can_work() {
            self.update_status_no_work();
        }

        Ok(progressed)
    }

    fn fill_free_slots(&mut self) -> Result<bool, String> {
        let mut progressed = false;
        while self.outstanding.len() < self.backend.queue_capacity() && self.can_work() {
            if self.extranonce_exhausted {
                break;
            }
            if !self.budget_allows_new() {
                break;
            }
            let desired = self.dispatch_count();
            let old_cursor = self.cursor;
            let old_extranonce = self.extranonce2.clone();
            let old_exhausted = self.extranonce_exhausted;
            let reserved = match self.reserve_range(desired) {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(e) => return Err(e),
            };
            let (range, job, conn, en2, work) = reserved;
            let estimated = self.estimated_duration_for_count(range.count);
            let ticket = match self.backend.submit_batch(&work) {
                Ok(ticket) => ticket,
                Err(error) => {
                    self.cursor = old_cursor;
                    self.extranonce2 = old_extranonce;
                    self.extranonce_exhausted = old_exhausted;
                    return Err(error.to_string());
                }
            };
            self.outstanding.push(OutstandingBatch {
                ticket,
                job,
                conn,
                extranonce2: en2,
                range,
                work,
                estimated_duration: estimated,
                submitted_at: Instant::now(),
            });
            progressed = true;
        }
        Ok(progressed)
    }

    #[allow(clippy::type_complexity)]
    fn reserve_range(
        &mut self,
        desired_count: u32,
    ) -> Result<Option<(NonceRange, Arc<MiningJob>, u64, Vec<u8>, WorkBatch)>, String> {
        if !self.can_work() {
            return Ok(None);
        }
        let job_gen = match self.current_job_generation {
            Some(g) => g,
            None => return Ok(None),
        };
        let job = match self
            .jobs
            .iter()
            .find(|j| j.job_generation == job_gen)
            .cloned()
        {
            Some(j) => j,
            None => {
                self.current_job_generation = None;
                return Ok(None);
            }
        };
        let conn = match self.connection_generation {
            Some(c) => c,
            None => return Ok(None),
        };

        let en1 = self.extranonce1.clone();
        if self.extranonce2.len() != self.extranonce2_size && self.extranonce2_size > 0 {
            let mut v = vec![0u8; self.extranonce2_size.clamp(1, 8)];
            let n = self.extranonce2.len().min(v.len());
            v[..n].copy_from_slice(&self.extranonce2[..n]);
            self.extranonce2 = v;
        }

        // Validate immutable job identity before mutating nonce state.
        if !job.difficulty.is_finite() || job.difficulty < 1.0 || job.difficulty > 1e30 {
            return Err("invalid job difficulty".to_string());
        }

        // handle cursor exhaustion
        if self.cursor >= (1u64 << 32) && !self.extranonce_exhausted {
            if self.extranonce2.is_empty() {
                self.extranonce2 = vec![0u8; self.extranonce2_size.clamp(1, 8)];
            }
            match increment_extranonce2(&mut self.extranonce2) {
                Ok(()) => self.cursor = 0,
                Err(_) => self.extranonce_exhausted = true,
            }
        }
        if self.extranonce_exhausted {
            return Ok(None);
        }

        let remaining = (1u64 << 32) - self.cursor;
        if remaining == 0 {
            if self.extranonce2.is_empty() {
                self.extranonce2 = vec![0u8; self.extranonce2_size.clamp(1, 8)];
            }
            match increment_extranonce2(&mut self.extranonce2) {
                Ok(()) => self.cursor = 0,
                Err(_) => {
                    self.extranonce_exhausted = true;
                    return Ok(None);
                }
            }
        }

        let count = (desired_count as u64).min((1u64 << 32) - self.cursor) as u32;
        if count == 0 {
            return Ok(None);
        }
        let range = NonceRange {
            base: self.cursor as u32,
            count,
        };

        let en2 = self.extranonce2.clone();

        // Bound prepared cache over extranonce changes.
        let identity = WorkIdentity {
            conn,
            job_gen: job.job_generation,
            target_generation: job.target_generation,
            job_ptr: Arc::as_ptr(&job) as usize,
            extranonce1: en1.clone(),
            extranonce2: en2.clone(),
        };

        let prep_start = Instant::now();
        if !self.prepared_cache.contains_key(&identity)
            && self.prepared_cache.len() >= PREPARED_CACHE_CAPACITY
        {
            if let Some(key) = self.prepared_cache.keys().next().cloned() {
                self.prepared_cache.remove(&key);
            }
        }
        let prepared = match self.prepared_cache.get(&identity).copied() {
            Some(prepared) => prepared,
            None => {
                let header = job.header(&en1, &en2)?;
                let (midstate, tail3) = build_header_midstate(&header)?;
                let prepared = PreparedWork { midstate, tail3 };
                self.prepared_cache.insert(identity, prepared);
                prepared
            }
        };

        // Advance cursor only after successful preparation.
        self.cursor += count as u64;
        self.prep_wall_ns = self
            .prep_wall_ns
            .saturating_add(prep_start.elapsed().as_nanos() as u64);

        let work = WorkBatch {
            midstate: prepared.midstate,
            tail3: prepared.tail3,
            target: job.target,
            nonce_base: range.base,
            nonce_count: range.count,
        };

        if work.validate().is_err() {
            // Should not happen for validated range; report error.
            return Err("invalid work batch".to_string());
        }

        Ok(Some((range, job, conn, en2, work)))
    }

    fn retire_one(&mut self) -> Result<bool, String> {
        if self.outstanding.is_empty() {
            return Ok(false);
        }
        let ob = self.outstanding.remove(0);

        let gpu_before = self.backend.telemetry().gpu_ns;
        let result = self
            .backend
            .retire_batch(ob.ticket)
            .map_err(|e| e.to_string())?;
        let residence = ob.submitted_at.elapsed();
        let gpu_after = self.backend.telemetry().gpu_ns;
        let execution = match (gpu_before, gpu_after) {
            (Some(before), Some(after)) if after > before => Duration::from_nanos(after - before),
            _ => residence, // Conservative when execution-only timestamps are unavailable.
        };
        let tp = result.unique_hashes as f64 / execution.as_secs_f64();
        if tp.is_finite() && tp > 0.0 {
            self.last_throughput = Some(tp);
        }

        // drain clean-job/protocol events after retire before refill/validation
        self.drain_events().map_err(|e| e.to_string())?;

        // refill freed slots; never ignore errors
        self.fill_free_slots()?;

        // validation timing
        let validation_start = Instant::now();

        if result.unique_hashes != ob.range.count as u64 {
            return Err("backend unique_hashes does not match requested count".to_string());
        }
        let range_base = ob.range.base as u64;
        let range_end = range_base + ob.range.count as u64;
        for &nonce in &result.candidates {
            if (nonce as u64) < range_base || (nonce as u64) >= range_end {
                return Err("backend candidate out of range".to_string());
            }
        }

        for &nonce in &result.candidates {
            let hash = work_hash(&ob.work, nonce);
            if !raw_hash_meets_target(&hash, &ob.job.target) {
                self.discarded += 1;
                self.update_status_discarded();
                continue;
            }
            let job_valid = self.authorized
                && self.connection_generation == Some(ob.conn)
                && self.jobs.iter().any(|j| {
                    j.job_generation == ob.job.job_generation
                        && j.connection_generation == ob.conn
                        && Arc::ptr_eq(j, &ob.job)
                });
            let submission_valid =
                self.is_submission_valid(ob.job.job_generation, ob.conn, ob.job.target_generation);
            if !job_valid || !submission_valid {
                self.discarded += 1;
                self.update_status_discarded();
                continue;
            }
            let key = (ob.extranonce2.clone(), ob.job.ntime, nonce);
            let set = self.dedup.entry(ob.job.job_generation).or_default();
            if set.len() >= DEDUP_PER_JOB_CAPACITY {
                return Err("dedup capacity exceeded".to_string());
            }
            if !set.insert(key) {
                self.discarded += 1;
                self.update_status_discarded();
                continue;
            }

            let share = Share {
                connection_generation: ob.conn,
                job_generation: ob.job.job_generation,
                job_id: ob.job.job_id.clone(),
                extranonce2: gbtc_core::job::encode_hex(&ob.extranonce2),
                ntime: ob.job.ntime,
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
        self.validation_wall_ns = self
            .validation_wall_ns
            .saturating_add(validation_start.elapsed().as_nanos() as u64);

        let status_start = Instant::now();
        self.lifetime_hashes = self.lifetime_hashes.saturating_add(result.unique_hashes);
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            self.rate_window_hashes = self.rate_window_hashes.saturating_add(result.unique_hashes);
            let elapsed = self.rate_window_start.elapsed();
            if elapsed >= Duration::from_secs(1) {
                g.update_hashes(self.rate_window_hashes, elapsed.as_secs_f64());
                self.rate_window_start = Instant::now();
                self.rate_window_hashes = 0;
            }
        }
        self.update_status_hashes(result.unique_hashes);
        self.update_status_telemetry();
        self.status_wall_ns = self
            .status_wall_ns
            .saturating_add(status_start.elapsed().as_nanos() as u64);

        Ok(true)
    }

    fn is_submission_valid(&self, job_gen: u64, conn: u64, target_generation: u64) -> bool {
        self.authorized
            && self.connection_generation == Some(conn)
            && self.jobs.iter().any(|j| {
                j.job_generation == job_gen
                    && j.connection_generation == conn
                    && j.target_generation == target_generation
            })
    }

    fn budget_allows_new(&self) -> bool {
        if self.last_throughput.is_none() {
            return self.outstanding.is_empty();
        }
        let queued_ms: u128 = self
            .outstanding
            .iter()
            .map(|o| o.estimated_duration.as_millis())
            .sum();
        let new_ms = self
            .estimated_duration_for_count(self.dispatch_count())
            .as_millis();
        queued_ms + new_ms <= self.queue_budget.as_millis()
    }

    fn dispatch_count(&self) -> u32 {
        let base = if let Some(tp) = self.last_throughput {
            let per_slot_ms =
                self.queue_budget.as_millis() as f64 / self.backend.queue_capacity().max(1) as f64;
            let count = (tp * per_slot_ms / 1000.0) as u64;
            count.min(self.batch_nonces as u64).max(1) as u32
        } else {
            (self.batch_nonces / 4)
                .clamp(1, 1_048_576)
                .min(self.batch_nonces)
        };
        base.min(self.batch_nonces).max(1)
    }

    fn estimated_duration_for_count(&self, count: u32) -> Duration {
        match self.last_throughput {
            Some(tp) if tp > 0.0 => {
                let secs = count as f64 / tp;
                let d = Duration::from_secs_f64(secs);
                if d < Duration::from_millis(1) {
                    Duration::from_millis(1)
                } else {
                    d
                }
            }
            _ => Duration::from_millis(1),
        }
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn submitted_count(&self) -> u64 {
        self.submitted
    }
    pub fn discarded_count(&self) -> u64 {
        self.discarded
    }
    pub fn accepted_count(&self) -> u64 {
        self.accepted
    }
    pub fn rejected_count(&self) -> u64 {
        self.rejected
    }
    pub fn unresolved_count(&self) -> u64 {
        self.unresolved
    }
    pub fn lifetime_hashes_count(&self) -> u64 {
        self.lifetime_hashes
    }
    pub fn current_job_generation(&self) -> Option<u64> {
        self.current_job_generation
    }
    pub fn cursor(&self) -> u64 {
        self.cursor
    }
    pub fn extranonce_exhausted(&self) -> bool {
        self.extranonce_exhausted
    }
    pub fn jobs_len(&self) -> usize {
        self.jobs.len()
    }
    pub fn prepared_cache_len(&self) -> usize {
        self.prepared_cache.len()
    }
    pub fn prep_wall_ns(&self) -> u64 {
        self.prep_wall_ns
    }
    pub fn validation_wall_ns(&self) -> u64 {
        self.validation_wall_ns
    }
    pub fn status_wall_ns(&self) -> u64 {
        self.status_wall_ns
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    pub fn run(&mut self) -> Result<(), String> {
        self.run_detailed().map_err(|e| e.to_string())
    }

    pub fn run_detailed(&mut self) -> Result<(), ControllerError> {
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
                Err(e) => return Err(ControllerError::new(e)),
            }
        }
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

    pub fn drain_events(&mut self) -> Result<(), ControllerError> {
        loop {
            match self.network_rx.try_recv() {
                Ok(ev) => self.handle_event(ev)?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.stop.load(Ordering::Relaxed) {
                        break;
                    }
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
                let valid_gens: HashSet<u64> = self.jobs.iter().map(|j| j.job_generation).collect();
                self.dedup.retain(|g, _| valid_gens.contains(g));
                self.prepared_cache.clear();
                self.current_job_generation = None;
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
                    j.connection_generation == job.connection_generation
                        && j.job_id == job.job_id
                        && j.ntime == job.ntime
                        && j.target == job.target
                        && j.prevhash == job.prevhash
                        && j.coinb1 == job.coinb1
                        && j.coinb2 == job.coinb2
                        && j.merkle == job.merkle
                        && j.version == job.version
                        && j.nbits == job.nbits
                        && j.job_generation == job.job_generation
                });
                if duplicate {
                    if job.clean {
                        self.jobs.retain(|j| j.job_generation == job.job_generation);
                        self.dedup.retain(|g, _| *g == job.job_generation);
                    }
                    return Ok(());
                }
                self.jobs.retain(|j| j.job_generation != job.job_generation);
                self.dedup.remove(&job.job_generation);
                self.prepared_cache
                    .retain(|key, _| key.job_gen != job.job_generation);
                let clean = job.clean;
                if clean {
                    self.jobs.retain(|j| {
                        !(j.connection_generation == job.connection_generation
                            && j.job_generation < job.job_generation)
                    });
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
                self.jobs.push(Arc::new(job.clone()));
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
                self.prepared_cache.clear();
                self.current_job_generation = None;
                self.cursor = 0;
                self.extranonce_exhausted = false;
                self.extranonce2.clear();
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

    fn update_status_telemetry(&self) {
        if let Some(status) = &self.status {
            let bt = self.backend.telemetry();
            let mut g = status.lock().unwrap();
            g.data["backend_telemetry"] = serde_json::json!({
                "submissions": bt.submissions,
                "completed_batches": bt.completed_batches,
                "fence_waits": bt.fence_waits,
                "poll_checks": bt.poll_checks,
                "submit_wall_ns": bt.submit_wall_ns,
                "wait_wall_ns": bt.wait_wall_ns,
                "gpu_ns": bt.gpu_ns,
                "overflow_rescans": bt.overflow_rescans,
            });
            g.data["backend_submissions"] = serde_json::json!(bt.submissions);
            g.data["backend_completed_batches"] = serde_json::json!(bt.completed_batches);
            g.data["backend_fence_waits"] = serde_json::json!(bt.fence_waits);
            g.data["backend_poll_checks"] = serde_json::json!(bt.poll_checks);
            g.data["backend_submit_wall_ns"] = serde_json::json!(bt.submit_wall_ns);
            g.data["backend_wait_wall_ns"] = serde_json::json!(bt.wait_wall_ns);
            if let Some(gpu) = bt.gpu_ns {
                g.data["backend_gpu_ns"] = serde_json::json!(gpu);
            } else {
                g.data["backend_gpu_ns"] = serde_json::json!(null);
            }
            g.data["backend_overflow_rescans"] = serde_json::json!(bt.overflow_rescans);
            g.data["prep_wall_ns"] = serde_json::json!(self.prep_wall_ns);
            g.data["validation_wall_ns"] = serde_json::json!(self.validation_wall_ns);
            g.data["status_wall_ns"] = serde_json::json!(self.status_wall_ns);
            g.data["prep_ns"] = serde_json::json!(self.prep_wall_ns);
            g.data["validation_ns"] = serde_json::json!(self.validation_wall_ns);
            g.data["status_ns"] = serde_json::json!(self.status_wall_ns);
        }
    }

    fn update_status_hashes(&self, hashes: u64) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["lifetime_hashes"] = serde_json::json!(self.lifetime_hashes);
            let _ = hashes;
        }
    }

    fn update_status_no_work(&self) {
        if let Some(status) = &self.status {
            let mut g = status.lock().unwrap();
            g.data["mh"] = serde_json::json!(0.0);
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
    share_tx: impl Into<ShareSender>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut c = Controller::new(backend, network_rx, share_tx, stop);
    c.run()
}

pub fn run_controller_with_status<B: ComputeBackend>(
    backend: B,
    network_rx: Receiver<Event>,
    share_tx: impl Into<ShareSender>,
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

#[cfg(test)]
mod pipeline_regressions {
    use super::*;
    use gbtc_core::backend::{BackendError, BatchResult, BatchTicket};
    use std::collections::VecDeque;
    use std::sync::mpsc::{self, Sender};

    struct Queue {
        work: VecDeque<WorkBatch>,
        seen: Vec<WorkBatch>,
        fail_next: bool,
        during_retire: Option<(Sender<Event>, Event)>,
    }
    impl ComputeBackend for Queue {
        fn run_batch(&mut self, _: &WorkBatch) -> Result<BatchResult, BackendError> {
            unreachable!()
        }
        fn queue_capacity(&self) -> usize {
            2
        }
        fn submit_batch(&mut self, work: &WorkBatch) -> Result<BatchTicket, BackendError> {
            if std::mem::take(&mut self.fail_next) {
                return Err(BackendError::unavailable("injected submit failure"));
            }
            assert!(self.work.len() < 2);
            self.seen.push(work.clone());
            self.work.push_back(work.clone());
            Ok(BatchTicket(self.seen.len() as u64))
        }
        fn retire_batch(&mut self, _: BatchTicket) -> Result<BatchResult, BackendError> {
            if let Some((sender, event)) = self.during_retire.take() {
                sender.send(event).unwrap();
            }
            let work = self.work.pop_front().unwrap();
            Ok(BatchResult::new(
                vec![work.nonce_base],
                work.nonce_count as u64,
                0,
            ))
        }
    }
    fn job(generation: u64) -> MiningJob {
        MiningJob {
            connection_generation: 1,
            job_generation: generation,
            target_generation: generation,
            job_id: format!("job-{generation}"),
            prevhash: [0; 32],
            coinb1: vec![1],
            coinb2: vec![2],
            merkle: vec![],
            version: 1,
            nbits: 0x1d00ffff,
            ntime: 1,
            target: [u32::MAX; 8],
            difficulty: 1.0,
            clean: true,
        }
    }
    fn setup() -> (Controller<Queue>, Sender<Event>, Receiver<Share>) {
        let (tx, rx) = mpsc::channel();
        let (share_tx, shares) = mpsc::sync_channel(32);
        let backend = Queue {
            work: VecDeque::new(),
            seen: vec![],
            fail_next: false,
            during_retire: None,
        };
        let mut c = Controller::with_options(
            backend,
            rx,
            share_tx,
            None,
            Arc::new(AtomicBool::new(false)),
            8,
        );
        c.handle_event(Event::Connected {
            connection_generation: 1,
            extranonce1: vec![],
            extranonce2_size: 1,
        })
        .unwrap();
        c.handle_event(Event::Job(job(1))).unwrap();
        c.handle_event(Event::Authorized).unwrap();
        (c, tx, shares)
    }
    #[test]
    fn failed_submit_preserves_nonce_and_extranonce() {
        let (mut c, _tx, _shares) = setup();
        c.cursor = 1u64 << 32;
        let before = c.extranonce2.clone();
        c.backend.fail_next = true;
        assert!(c.tick().is_err());
        assert_eq!(c.cursor, 1u64 << 32);
        assert_eq!(c.extranonce2, before);
        c.tick().unwrap();
        assert_eq!(c.backend.seen[0].nonce_base, 0);
        assert_eq!(c.extranonce2, vec![1]);
    }
    #[test]
    fn queued_last_nonce_uses_saved_extranonce() {
        let (mut c, _tx, shares) = setup();
        c.cursor = u32::MAX as u64;
        c.tick().unwrap();
        assert_eq!(c.backend.seen[0].nonce_base, u32::MAX);
        assert_eq!(c.backend.seen[0].nonce_count, 1);
        c.tick().unwrap();
        let share = shares.try_recv().unwrap();
        assert_eq!(share.nonce, u32::MAX);
        assert_eq!(share.extranonce2, "00");
        assert_eq!(c.backend.seen[1].nonce_base, 0);
        assert_eq!(c.extranonce2, vec![1]);
    }
    #[test]
    fn clean_event_during_retire_precedes_refill_and_validation() {
        let (mut c, tx, shares) = setup();
        c.tick().unwrap();
        let mut next = job(2);
        next.ntime = 1234;
        c.backend.during_retire = Some((tx.clone(), Event::Job(next.clone())));
        c.tick().unwrap();
        assert!(shares.try_recv().is_err());
        let expected = next
            .work(&[], &[0], 0, c.backend.seen[1].nonce_count)
            .unwrap();
        assert_eq!(c.backend.seen[1].tail3, expected.tail3);
        assert_eq!(c.discarded_count(), 1);
    }
    #[test]
    fn replacement_generation_cannot_reuse_prepared_header() {
        let (mut c, _tx, _shares) = setup();
        c.tick().unwrap();
        let old = c.backend.seen[0].clone();
        let mut replacement = job(1);
        replacement.coinb1 = vec![7, 8, 9];
        c.handle_event(Event::Job(replacement.clone())).unwrap();
        c.tick().unwrap();
        let work = c.backend.seen[1].clone();
        assert_ne!(work.midstate, old.midstate);
        assert_eq!(
            work.midstate,
            replacement
                .work(&[], &[0], 0, work.nonce_count)
                .unwrap()
                .midstate
        );
    }
}
