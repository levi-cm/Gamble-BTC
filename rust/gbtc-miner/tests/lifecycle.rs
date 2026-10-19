use std::process::Command;

fn base_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gbtc-miner"));
    // isolate from ambient GBTC_ variables
    for (k, _) in std::env::vars() {
        if k.starts_with("GBTC_") {
            cmd.env_remove(&k);
        }
    }
    cmd
}

#[test]
fn probe_mode_no_wallet_exits_ok() {
    // Only run GPU-dependent probe when explicitly requested.
    if std::env::var("GBTC_REQUIRE_GPU_TESTS").ok().as_deref() != Some("1") {
        let out = base_cmd()
            .env("GBTC_PROBE_ONLY", "1")
            .env("GBTC_BACKEND", "vulkan")
            .env("GBTC_DEVICE", "/dev/null")
            .env_remove("BTC_ADDRESS")
            .output()
            .expect("run");
        assert!(!out.status.success(), "invalid device should fail");
        return;
    }
    let out = base_cmd()
        .env("GBTC_PROBE_ONLY", "1")
        .env_remove("BTC_ADDRESS")
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"ok\""), "stdout: {}", stdout);
}

#[test]
fn unknown_backend_exits_nonzero() {
    let out = base_cmd()
        .env("GBTC_BACKEND", "definitely-not-a-backend")
        .env("BTC_ADDRESS", "bc1qexample")
        .output()
        .expect("run");
    assert!(!out.status.success());
}

#[test]
fn mode_contradiction_exits_nonzero() {
    let out = base_cmd()
        .env("GBTC_PROBE_ONLY", "1")
        .env("GBTC_BENCH_ONLY", "1")
        .output()
        .expect("run");
    assert!(!out.status.success());
}

#[test]
fn invalid_device_vulkan_exits_nonzero() {
    let out = base_cmd()
        .env("GBTC_PROBE_ONLY", "1")
        .env("GBTC_BACKEND", "vulkan")
        .env("GBTC_DEVICE", "/dev/null")
        .env_remove("BTC_ADDRESS")
        .output()
        .expect("run");
    assert!(!out.status.success());
}

#[test]
fn invalid_http_port_fails_before_gpu_initialization() {
    let out = base_cmd()
        .env("GBTC_PROBE_ONLY", "1")
        .env("GBTC_HTTP_PORT", "0")
        .env_remove("BTC_ADDRESS")
        .output()
        .expect("run");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("GBTC_HTTP_PORT"));
}

#[test]
fn explicit_vulkan_no_fallback_on_bad_device() {
    let out = base_cmd()
        .env("GBTC_PROBE_ONLY", "1")
        .env("GBTC_BACKEND", "vulkan")
        .env("GBTC_DEVICE", "/dev/null")
        .output()
        .expect("run");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("falling back"));
    assert!(out.stdout.is_empty());
}
