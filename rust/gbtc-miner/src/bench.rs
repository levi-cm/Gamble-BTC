use gbtc_core::backend::{BackendTelemetry, ComputeBackend, WorkBatch};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct BenchReport {
    pub ok: bool,
    pub backend: String,
    pub backend_api: String,
    pub device_path: String,
    pub device_vendor: String,
    pub device_name: String,
    pub driver_name: String,
    pub kernel: String,
    pub local_size: u32,
    pub batch_nonces: u32,
    pub seconds: f64,
    pub hashes: u64,
    pub mh_s: f64,
    pub compile_result: String,
    pub rescans: u32,
    pub error: Option<String>,
    pub fallback_reason: String,
    pub backend_telemetry_delta: BackendTelemetry,
}

impl BenchReport {
    pub fn to_json_line(&self) -> String {
        let mut obj = json!({
            "ok": self.ok,
            "backend": self.backend,
            "backend_api": self.backend_api,
            "device_path": self.device_path,
            "device_vendor": self.device_vendor,
            "device_name": self.device_name,
            "driver_name": self.driver_name,
            "kernel": self.kernel,
            "local_size": self.local_size,
            "batch_nonces": self.batch_nonces,
            "seconds": self.seconds,
            "hashes": self.hashes,
            "mh_s": self.mh_s,
            "compile_result": self.compile_result,
            "rescans": self.rescans,
            "fallback_reason": self.fallback_reason,
            "backend_telemetry_delta": {
                "submissions": self.backend_telemetry_delta.submissions,
                "completed_batches": self.backend_telemetry_delta.completed_batches,
                "fence_waits": self.backend_telemetry_delta.fence_waits,
                "poll_checks": self.backend_telemetry_delta.poll_checks,
                "submit_wall_ns": self.backend_telemetry_delta.submit_wall_ns,
                "wait_wall_ns": self.backend_telemetry_delta.wait_wall_ns,
                "gpu_ns": self.backend_telemetry_delta.gpu_ns,
                "overflow_rescans": self.backend_telemetry_delta.overflow_rescans,
            },
            "backend_telemetry": {
                "submissions": self.backend_telemetry_delta.submissions,
                "completed_batches": self.backend_telemetry_delta.completed_batches,
                "fence_waits": self.backend_telemetry_delta.fence_waits,
                "poll_checks": self.backend_telemetry_delta.poll_checks,
                "submit_wall_ns": self.backend_telemetry_delta.submit_wall_ns,
                "wait_wall_ns": self.backend_telemetry_delta.wait_wall_ns,
                "gpu_ns": self.backend_telemetry_delta.gpu_ns,
                "overflow_rescans": self.backend_telemetry_delta.overflow_rescans,
            },
        });
        if let Some(e) = &self.error {
            obj["error"] = json!(e);
        }
        obj.to_string()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_synthetic_benchmark<B: ComputeBackend>(
    backend: &mut B,
    backend_name: &str,
    backend_api: &str,
    device_path: &str,
    device_vendor: &str,
    device_name: &str,
    driver_name: &str,
    kernel: &str,
    local_size: u32,
    batch_nonces: u32,
    warmup_seconds: i64,
    measure_seconds: i64,
    stop: Arc<AtomicBool>,
) -> Result<BenchReport, String> {
    if batch_nonces == 0 {
        return Err("batch_nonces must be greater than zero".to_string());
    }
    let mut work = WorkBatch {
        midstate: [0; 8],
        tail3: [0; 3],
        target: [0; 8],
        nonce_base: 0,
        nonce_count: batch_nonces,
    };
    gbtc_core::make_synthetic_work(&mut work, batch_nonces);

    let capacity = backend.queue_capacity().max(1);

    // Warmup (not counted, not timed).
    let warmup_deadline = Instant::now() + Duration::from_secs(warmup_seconds.max(0) as u64);
    if capacity <= 1 {
        while Instant::now() < warmup_deadline {
            if stop.load(Ordering::Relaxed) {
                return Err("stopped during warmup".to_string());
            }
            let _ = backend.run_batch(&work).map_err(|e| e.to_string())?;
        }
    } else {
        let mut outstanding: Vec<gbtc_core::backend::BatchTicket> = Vec::new();
        while Instant::now() < warmup_deadline {
            if stop.load(Ordering::Relaxed) {
                return Err("stopped during warmup".to_string());
            }
            while outstanding.len() < capacity {
                match backend.submit_batch(&work) {
                    Ok(t) => outstanding.push(t),
                    Err(e) => return Err(e.to_string()),
                }
            }
            if !outstanding.is_empty() {
                let t = outstanding.remove(0);
                let _ = backend.retire_batch(t).map_err(|e| e.to_string())?;
            }
        }
        // drain
        while !outstanding.is_empty() {
            let t = outstanding.remove(0);
            let _ = backend.retire_batch(t).map_err(|e| e.to_string())?;
        }
    }

    let telemetry_before = backend.telemetry();

    // Measured loop; tail batch fully included.
    let start = Instant::now();
    let measure_deadline = start + Duration::from_secs(measure_seconds.max(1) as u64);
    let mut hashes: u64 = 0;
    let mut rescans: u32 = 0;
    if capacity <= 1 {
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err("stopped during benchmark".to_string());
            }
            let result = backend.run_batch(&work).map_err(|e| e.to_string())?;
            if result.unique_hashes != work.nonce_count as u64 {
                return Err("backend benchmark hash count contract violation".into());
            }
            hashes = hashes.saturating_add(result.unique_hashes);
            rescans = rescans.saturating_add(result.overflow_rescans);
            if Instant::now() >= measure_deadline {
                break;
            }
        }
    } else {
        let mut outstanding: Vec<gbtc_core::backend::BatchTicket> = Vec::new();
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err("stopped during benchmark".to_string());
            }
            while outstanding.len() < capacity {
                match backend.submit_batch(&work) {
                    Ok(t) => outstanding.push(t),
                    Err(e) => return Err(e.to_string()),
                }
            }
            if !outstanding.is_empty() {
                let t = outstanding.remove(0);
                let result = backend.retire_batch(t).map_err(|e| e.to_string())?;
                if result.unique_hashes != work.nonce_count as u64 {
                    return Err("backend benchmark hash count contract violation".into());
                }
                hashes = hashes.saturating_add(result.unique_hashes);
                rescans = rescans.saturating_add(result.overflow_rescans);
            }
            if Instant::now() >= measure_deadline {
                break;
            }
        }
        // drain remaining
        while !outstanding.is_empty() {
            let t = outstanding.remove(0);
            let result = backend.retire_batch(t).map_err(|e| e.to_string())?;
            if result.unique_hashes != work.nonce_count as u64 {
                return Err("backend benchmark hash count contract violation".into());
            }
            hashes = hashes.saturating_add(result.unique_hashes);
            rescans = rescans.saturating_add(result.overflow_rescans);
        }
    }
    let telemetry_after = backend.telemetry();
    let telemetry_delta = BackendTelemetry {
        submissions: telemetry_after
            .submissions
            .saturating_sub(telemetry_before.submissions),
        completed_batches: telemetry_after
            .completed_batches
            .saturating_sub(telemetry_before.completed_batches),
        fence_waits: telemetry_after
            .fence_waits
            .saturating_sub(telemetry_before.fence_waits),
        poll_checks: telemetry_after
            .poll_checks
            .saturating_sub(telemetry_before.poll_checks),
        submit_wall_ns: telemetry_after
            .submit_wall_ns
            .saturating_sub(telemetry_before.submit_wall_ns),
        wait_wall_ns: telemetry_after
            .wait_wall_ns
            .saturating_sub(telemetry_before.wait_wall_ns),
        gpu_ns: match (telemetry_before.gpu_ns, telemetry_after.gpu_ns) {
            (Some(a), Some(b)) => Some(b.saturating_sub(a)),
            _ => telemetry_after.gpu_ns,
        },
        overflow_rescans: telemetry_after
            .overflow_rescans
            .saturating_sub(telemetry_before.overflow_rescans),
    };

    let seconds = start.elapsed().as_secs_f64();
    let mh_s = if seconds > 0.0 {
        hashes as f64 / 1_000_000.0 / seconds
    } else {
        0.0
    };

    Ok(BenchReport {
        ok: true,
        backend: backend_name.to_string(),
        backend_api: backend_api.to_string(),
        device_path: device_path.to_string(),
        device_vendor: device_vendor.to_string(),
        device_name: device_name.to_string(),
        driver_name: driver_name.to_string(),
        kernel: kernel.to_string(),
        local_size,
        batch_nonces,
        seconds,
        hashes,
        mh_s,
        compile_result: "ok".to_string(),
        rescans,
        error: None,
        fallback_reason: String::new(),
        backend_telemetry_delta: telemetry_delta,
    })
}
