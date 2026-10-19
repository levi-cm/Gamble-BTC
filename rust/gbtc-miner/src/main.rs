use gbtc_backend_opencl::OpenClBackend;
use gbtc_backend_vulkan::VulkanContext;
use gbtc_core::backend::{
    BackendError, BackendTelemetry, BatchResult, BatchTicket, ComputeBackend, WorkBatch,
};
use gbtc_core::shutdown::Shutdown;
use gbtc_http::{serve_with_shutdown as serve_http, Status};
use gbtc_miner::bench::{run_synthetic_benchmark, BenchReport};
use gbtc_miner::config::Config;
use gbtc_miner::controller::Controller;
use gbtc_miner::watchdog;
use serde_json::json;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[allow(clippy::large_enum_variant)]
enum BackendRuntime {
    Vulkan(VulkanContext),
    OpenCl(OpenClBackend),
}

impl ComputeBackend for BackendRuntime {
    fn run_batch(
        &mut self,
        work: &gbtc_core::backend::WorkBatch,
    ) -> Result<gbtc_core::backend::BatchResult, gbtc_core::backend::BackendError> {
        match self {
            BackendRuntime::Vulkan(v) => v.run_batch(work),
            BackendRuntime::OpenCl(o) => o.run_batch(work),
        }
    }
    fn queue_capacity(&self) -> usize {
        match self {
            Self::Vulkan(v) => v.queue_capacity(),
            Self::OpenCl(o) => o.queue_capacity(),
        }
    }
    fn submit_batch(&mut self, work: &WorkBatch) -> Result<BatchTicket, BackendError> {
        match self {
            Self::Vulkan(v) => v.submit_batch(work),
            Self::OpenCl(o) => o.submit_batch(work),
        }
    }
    fn retire_batch(&mut self, ticket: BatchTicket) -> Result<BatchResult, BackendError> {
        match self {
            Self::Vulkan(v) => v.retire_batch(ticket),
            Self::OpenCl(o) => o.retire_batch(ticket),
        }
    }
    fn telemetry(&self) -> BackendTelemetry {
        match self {
            Self::Vulkan(v) => v.telemetry(),
            Self::OpenCl(o) => o.telemetry(),
        }
    }
}

impl BackendRuntime {
    fn backend_info(&self) -> gbtc_core::backend::BackendInfo {
        match self {
            BackendRuntime::Vulkan(v) => v.backend_info(),
            BackendRuntime::OpenCl(o) => o.backend_info(),
        }
    }

    fn local_size(&self) -> u32 {
        match self {
            BackendRuntime::Vulkan(v) => v.local_size(),
            BackendRuntime::OpenCl(o) => o.local_size(),
        }
    }

    fn kernel_name(&self) -> &str {
        match self {
            BackendRuntime::Vulkan(v) => v.kernel_name(),
            BackendRuntime::OpenCl(o) => o.kernel_name(),
        }
    }

    fn device_vendor(&self) -> String {
        match self {
            BackendRuntime::Vulkan(v) => format!("0x{:x}", v.info().vendor_id),
            BackendRuntime::OpenCl(_) => String::new(),
        }
    }

    fn device_name(&self) -> String {
        match self {
            BackendRuntime::Vulkan(v) => v.info().name.clone(),
            BackendRuntime::OpenCl(o) => o.backend_info().device.clone(),
        }
    }

    fn driver_name(&self) -> String {
        match self {
            BackendRuntime::Vulkan(v) => format!("0x{:x}", v.info().driver_version),
            BackendRuntime::OpenCl(o) => o.backend_info().driver.clone(),
        }
    }
}

fn sanitize_reason(s: String) -> String {
    s
}

fn create_backend(cfg: &Config, stop: Arc<AtomicBool>) -> Result<(BackendRuntime, String), String> {
    match cfg.backend {
        gbtc_miner::config::Backend::Auto => match VulkanContext::new(Some(cfg.device.as_str())) {
            Ok(mut b) => {
                b.set_stop(stop.clone());
                Ok((BackendRuntime::Vulkan(b), String::new()))
            }
            Err(e) => {
                let reason = sanitize_reason(e.to_string());
                eprintln!(
                    "vulkan backend unavailable ({}); falling back to opencl",
                    reason
                );
                let mut b = OpenClBackend::new(Some(cfg.device.as_str()))
                    .map_err(|e2| format!("vulkan failed: {}; opencl failed: {}", reason, e2))?;
                b.set_stop(stop);
                Ok((BackendRuntime::OpenCl(b), reason))
            }
        },
        gbtc_miner::config::Backend::Vulkan => {
            let mut b = VulkanContext::new(Some(cfg.device.as_str())).map_err(|e| e.to_string())?;
            b.set_stop(stop);
            Ok((BackendRuntime::Vulkan(b), String::new()))
        }
        gbtc_miner::config::Backend::OpenCl => {
            let mut b = OpenClBackend::new(Some(cfg.device.as_str())).map_err(|e| e.to_string())?;
            b.set_stop(stop);
            Ok((BackendRuntime::OpenCl(b), String::new()))
        }
    }
}

fn run() -> Result<(), String> {
    let cfg = Config::from_env().map_err(|e| e.to_string())?;

    let shutdown = Shutdown::new();
    let mut signals =
        Signals::new([SIGINT, SIGTERM]).map_err(|e| format!("signal register failed: {e}"))?;
    let signal_handle = signals.handle();
    let signal_shutdown = shutdown.clone();
    let signal_thread = std::thread::spawn(move || {
        if signals.forever().next().is_some() {
            signal_shutdown.request_stop();
        }
    });
    let finished = Arc::new(AtomicBool::new(false));
    let watchdog = watchdog::watchdog_with_shutdown(
        shutdown.clone(),
        finished.clone(),
        Duration::from_secs(5),
        || std::process::exit(1),
    );

    let result = run_runtime(&cfg, shutdown.clone());
    shutdown.request_stop();
    finished.store(true, Ordering::Relaxed);
    shutdown.notify();
    signal_handle.close();
    let _ = signal_thread.join();
    let _ = watchdog.join();
    if shutdown.is_stopped()
        && result
            .as_ref()
            .is_err_and(|e| e.contains("cancelled") || e.contains("stopped during"))
    {
        return Ok(());
    }
    result
}

struct Services {
    shutdown: Shutdown,
    handles: Vec<std::thread::JoinHandle<()>>,
}

impl Drop for Services {
    fn drop(&mut self) {
        self.shutdown.request_stop();
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

fn run_runtime(cfg: &Config, shutdown: Shutdown) -> Result<(), String> {
    let stop = shutdown.flag();
    let (mut backend, fallback_reason) = create_backend(cfg, stop.clone())?;

    if cfg.probe_only {
        let info = backend.backend_info();
        let probe = json!({
            "ok": true,
            "mode": "probe",
            "backend": info.backend,
            "backend_api": info.api,
            "device_path": cfg.device,
            "device_vendor": backend.device_vendor(),
            "device_name": backend.device_name(),
            "driver_name": backend.driver_name(),
            "local_size": backend.local_size(),
            "kernel": backend.kernel_name(),
            "fallback_reason": fallback_reason,
        });
        println!("{}", probe);
        return Ok(());
    }

    if cfg.bench_only {
        let info = backend.backend_info();
        let dv = backend.device_vendor();
        let dn = backend.device_name();
        let dr = backend.driver_name();
        let kn = backend.kernel_name().to_string();
        let ls = backend.local_size();
        let report = run_synthetic_benchmark(
            &mut backend,
            info.backend.as_str(),
            &info.api,
            cfg.device.as_str(),
            &dv,
            &dn,
            &dr,
            &kn,
            ls,
            cfg.batch_nonces,
            cfg.bench_warmup_seconds,
            cfg.bench_seconds,
            stop.clone(),
        );
        match report {
            Ok(mut r) => {
                r.fallback_reason = fallback_reason;
                println!("{}", r.to_json_line());
                return Ok(());
            }
            Err(e) => {
                let info = backend.backend_info();
                let dv = backend.device_vendor();
                let dn = backend.device_name();
                let dr = backend.driver_name();
                let kn = backend.kernel_name().to_string();
                let ls = backend.local_size();
                let r = BenchReport {
                    ok: false,
                    backend_telemetry_delta: BackendTelemetry::default(),
                    backend: info.backend,
                    backend_api: info.api,
                    device_path: cfg.device.clone(),
                    device_vendor: dv,
                    device_name: dn,
                    driver_name: dr,
                    kernel: kn,
                    local_size: ls,
                    batch_nonces: cfg.batch_nonces,
                    seconds: 0.0,
                    hashes: 0,
                    mh_s: 0.0,
                    compile_result: "failed".to_string(),
                    rescans: 0,
                    error: Some(e.clone()),
                    fallback_reason,
                };
                println!("{}", r.to_json_line());
                return Err(e);
            }
        }
    }

    if cfg.btc_address.trim().is_empty() {
        return Err("BTC_ADDRESS is required".to_string());
    }

    let status = Arc::new(Mutex::new(Status::default()));
    {
        let mut g = status.lock().unwrap();
        g.data["addr"] = json!(cfg.btc_address);
        g.data["pool"] = json!(cfg.pool_url);
        g.data["worker"] = json!(cfg.worker_name);
        let info = backend.backend_info();
        g.data["backend"] = json!(info.backend);
        g.data["backend_api"] = json!(info.api);
        g.data["device_path"] = json!(cfg.device);
        g.data["device_vendor"] = json!(backend.device_vendor());
        g.data["device_name"] = json!(backend.device_name());
        g.data["driver_name"] = json!(backend.driver_name());
        g.data["fallback_reason"] = json!(fallback_reason);
    }

    let http_addr = cfg.http_socket_addr();
    let mut services = Services {
        shutdown: shutdown.clone(),
        handles: Vec::new(),
    };
    services.handles.push(
        serve_http(http_addr, status.clone(), shutdown.clone())
            .map_err(|e| format!("http serve failed: {}", e))?,
    );

    let (net_rx, share_tx, net_handle) = gbtc_stratum::spawn_with_shutdown(
        cfg.pool_url.clone(),
        cfg.btc_address.clone(),
        cfg.worker_name.clone(),
        shutdown.clone(),
    )
    .map_err(|e| format!("stratum spawn failed: {}", e))?;

    services.handles.push(net_handle);
    let mut controller = Controller::with_options(
        backend,
        net_rx,
        share_tx,
        Some(status.clone()),
        stop.clone(),
        cfg.batch_nonces,
    )
    .with_queue_budget(Duration::from_millis(cfg.gpu_queue_ms));

    let ctrl_result = controller.run();

    shutdown.request_stop();

    drop(controller);
    drop(services);

    ctrl_result
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
