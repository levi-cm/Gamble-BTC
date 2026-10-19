use gbtc_miner::config::{Config, ConfigError, MigrationError};
use std::env;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn set_base() {
    env::set_var("BTC_ADDRESS", "bc1qexample");
    env::remove_var("POOL_URL");
    env::remove_var("WORKER_NAME");
    env::remove_var("GBTC_BACKEND");
    env::remove_var("GBTC_PROBE_ONLY");
    env::remove_var("GBTC_BENCH_ONLY");
    env::remove_var("GBTC_DEVICE");
    env::remove_var("GBTC_BATCH_NONCES");
    env::remove_var("GBTC_BENCH_SECONDS");
    env::remove_var("GBTC_BENCH_WARMUP_SECONDS");
    env::remove_var("GBTC_HTTP_BIND");
    env::remove_var("GBTC_HTTP_PORT");
    for v in [
        "GBTC_GLES_KERNEL",
        "GBTC_GLES_LOCAL_SIZE",
        "GBTC_GLES_AUTOTUNE",
        "GBTC_OPENCL_LOCAL_SIZE",
        "GBTC_OPENCL_POLL_US",
        "GBTC_OPENCL_SIMD",
        "GBTC_OPENCL_KERNEL",
        "GBTC_OPENCL_DEBUG",
        "GBTC_OPENCL_PLATFORM",
        "GBTC_VULKAN_LOCAL_SIZE",
        "GBTC_VULKAN_POLL_US",
        "GBTC_VULKAN_VALIDATION",
    ] {
        env::remove_var(v);
    }
}

#[test]
fn defaults_parse() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    let cfg = Config::from_env().expect("defaults should parse");
    assert_eq!(cfg.btc_address, "bc1qexample");
    assert_eq!(cfg.pool_url, "stratum+tcp://public-pool.io:3333");
    assert_eq!(cfg.worker_name, "x");
    assert_eq!(cfg.backend, "auto");
    assert!(!cfg.probe_only);
    assert!(!cfg.bench_only);
    assert_eq!(cfg.device, "/dev/dri/renderD128");
    assert_eq!(cfg.batch_nonces, 1u32 << 24);
    assert_eq!(cfg.bench_seconds, 60);
    assert_eq!(cfg.bench_warmup_seconds, 5);
    assert_eq!(cfg.http_bind.to_string(), "127.0.0.1");
}

#[test]
fn invalid_http_bind_rejected() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_HTTP_BIND", "localhost");
    assert!(Config::from_env().is_err());
    env::set_var("GBTC_HTTP_BIND", "1.2.3.999");
    assert!(Config::from_env().is_err());
}

#[test]
fn invalid_backend_rejected() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_BACKEND", "bogus");
    assert!(Config::from_env().is_err());
    env::set_var("GBTC_BACKEND", "cpu");
    assert!(Config::from_env().is_err());
}

#[test]
fn gles_kernel_returns_migration_error() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_GLES_KERNEL", "altbool");
    let err = Config::from_env().unwrap_err();
    assert!(matches!(err, ConfigError::Migration(_)));
    let _ = MigrationError::new("x");
}

#[test]
fn invalid_batch_nonces_rejected() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_BATCH_NONCES", "96");
    assert!(Config::from_env().is_err());
    env::set_var("GBTC_BATCH_NONCES", "bogus");
    assert!(Config::from_env().is_err());
    env::set_var("GBTC_BATCH_NONCES", "0");
    assert!(Config::from_env().is_err());
}

#[test]
fn invalid_bench_seconds_rejected() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_BENCH_SECONDS", "0");
    assert!(Config::from_env().is_err());
    env::set_var("GBTC_BENCH_SECONDS", "abc");
    assert!(Config::from_env().is_err());
    env::set_var("GBTC_BENCH_SECONDS", "86401");
    assert!(Config::from_env().is_err());
}

#[test]
fn explicit_gles_backend_returns_migration_error() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_BACKEND", "gles");
    let err = Config::from_env().unwrap_err();
    assert!(matches!(err, ConfigError::Migration(_)));
}

#[test]
fn missing_btc_address_rejected() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::remove_var("BTC_ADDRESS");
    assert!(Config::from_env().is_err());
}

#[test]
fn probe_mode_does_not_require_btc_address() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::remove_var("BTC_ADDRESS");
    env::set_var("GBTC_PROBE_ONLY", "1");
    let cfg = Config::from_env().expect("probe mode should parse without wallet");
    assert!(cfg.probe_only);
    assert!(!cfg.bench_only);
    assert_eq!(cfg.btc_address, "");
}

#[test]
fn bench_mode_does_not_require_btc_address() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::remove_var("BTC_ADDRESS");
    env::set_var("GBTC_BENCH_ONLY", "1");
    let cfg = Config::from_env().expect("bench mode should parse without wallet");
    assert!(cfg.bench_only);
    assert!(!cfg.probe_only);
    assert_eq!(cfg.btc_address, "");
}

#[test]
fn simultaneous_modes_rejected() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_PROBE_ONLY", "1");
    env::set_var("GBTC_BENCH_ONLY", "1");
    assert!(Config::from_env().is_err());
}

#[test]
fn invalid_vulkan_poll_rejected() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_VULKAN_POLL_US", "50");
    assert!(Config::from_env().is_err());
    env::set_var("GBTC_VULKAN_POLL_US", "50001");
    assert!(Config::from_env().is_err());
    env::set_var("GBTC_VULKAN_POLL_US", "abc");
    assert!(Config::from_env().is_err());
}

#[test]
fn invalid_vulkan_local_size_rejected() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_VULKAN_LOCAL_SIZE", "7");
    assert!(Config::from_env().is_err());
    env::set_var("GBTC_VULKAN_LOCAL_SIZE", "512");
    assert!(Config::from_env().is_err());
}

#[test]
fn valid_vulkan_settings_parse() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_VULKAN_POLL_US", "1000");
    env::set_var("GBTC_VULKAN_LOCAL_SIZE", "128");
    let cfg = Config::from_env().expect("valid vulkan settings");
    assert_eq!(cfg.vulkan_poll_us, Some("1000".to_string()));
    assert_eq!(cfg.vulkan_local_size, Some("128".to_string()));
}

#[test]
fn custom_http_port_is_used_in_socket_address() {
    let _g = ENV_LOCK.lock().unwrap();
    set_base();
    env::set_var("GBTC_HTTP_PORT", "41175");
    let cfg = Config::from_env().unwrap();
    assert_eq!(cfg.http_socket_addr().port(), 41175);
    env::remove_var("GBTC_HTTP_PORT");
}
