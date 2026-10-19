use std::collections::BTreeMap;
use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Auto,
    Vulkan,
    OpenCl,
}

impl Backend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::Auto => "auto",
            Backend::Vulkan => "vulkan",
            Backend::OpenCl => "opencl",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "" | "auto" => Ok(Backend::Auto),
            "vulkan" => Ok(Backend::Vulkan),
            "opencl" => Ok(Backend::OpenCl),
            "gles" => Err(ConfigError::Migration(MigrationError::new(
                "GLES backend is retired; migrate to Vulkan or OpenCL: \
                 set GBTC_BACKEND=vulkan (or opencl), remove GBTC_GLES_KERNEL, \
                 GBTC_GLES_LOCAL_SIZE and GBTC_GLES_AUTOTUNE. \
                 See docs/RUST-COMPATIBILITY.md section 4.",
            ))),
            other => Err(ConfigError::InvalidValue {
                name: "GBTC_BACKEND".to_string(),
                message: format!(
                    "GBTC_BACKEND must be auto, vulkan, or opencl (got {:?}); \
                     gles is retired",
                    other
                ),
            }),
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<&str> for Backend {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for Backend {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<str> for Backend {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpBind(pub Ipv4Addr);

impl HttpBind {
    pub fn as_ipv4(&self) -> Ipv4Addr {
        self.0
    }
}

impl fmt::Display for HttpBind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Deref for HttpBind {
    type Target = Ipv4Addr;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Ipv4Addr> for HttpBind {
    fn from(v: Ipv4Addr) -> Self {
        HttpBind(v)
    }
}

impl PartialEq<Ipv4Addr> for HttpBind {
    fn eq(&self, other: &Ipv4Addr) -> bool {
        &self.0 == other
    }
}

impl PartialEq<&str> for HttpBind {
    fn eq(&self, other: &&str) -> bool {
        self.0.to_string() == *other
    }
}

impl PartialEq<String> for HttpBind {
    fn eq(&self, other: &String) -> bool {
        self.0.to_string() == *other
    }
}

impl PartialEq<str> for HttpBind {
    fn eq(&self, other: &str) -> bool {
        self.0.to_string() == other
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationError {
    pub message: String,
}

impl MigrationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MigrationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    Missing { name: String },
    InvalidValue { name: String, message: String },
    InvalidBackend(String),
    InvalidHttpBind(String),
    Migration(MigrationError),
}

impl ConfigError {
    pub fn missing(name: impl Into<String>) -> Self {
        ConfigError::Missing { name: name.into() }
    }

    pub fn invalid(name: impl Into<String>, message: impl Into<String>) -> Self {
        ConfigError::InvalidValue {
            name: name.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing { name } => write!(f, "{} is required", name),
            ConfigError::InvalidValue { name, message } => write!(f, "{}: {}", name, message),
            ConfigError::InvalidBackend(msg) => write!(f, "GBTC_BACKEND: {}", msg),
            ConfigError::InvalidHttpBind(msg) => write!(f, "GBTC_HTTP_BIND: {}", msg),
            ConfigError::Migration(m) => write!(f, "migration required: {}", m),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<MigrationError> for ConfigError {
    fn from(m: MigrationError) -> Self {
        ConfigError::Migration(m)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub btc_address: String,
    pub pool_url: String,
    pub worker_name: String,
    pub backend: Backend,
    pub probe_only: bool,
    pub bench_only: bool,
    pub device: String,
    pub batch_nonces: u32,
    pub bench_seconds: i64,
    pub bench_warmup_seconds: i64,
    pub http_bind: HttpBind,
    pub http_port: u16,

    pub opencl_local_size: Option<String>,
    pub opencl_poll_us: Option<String>,
    pub opencl_simd: Option<String>,
    pub opencl_kernel: Option<String>,
    pub opencl_debug: Option<String>,
    pub opencl_platform: Option<String>,

    pub vulkan_local_size: Option<String>,
    pub vulkan_poll_us: Option<String>,
    pub vulkan_validation: Option<String>,
    pub vulkan_wait_mode: Option<String>,
    pub vulkan_wait_us: Option<String>,

    pub gpu_queue_ms: u64,

    pub opencl_settings: BTreeMap<String, String>,
    pub vulkan_settings: BTreeMap<String, String>,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        // GLES retirement: any non-empty GLES setting is a hard migration error.
        for name in [
            "GBTC_GLES_KERNEL",
            "GBTC_GLES_LOCAL_SIZE",
            "GBTC_GLES_AUTOTUNE",
        ] {
            if let Ok(v) = std::env::var(name) {
                if !v.trim().is_empty() {
                    return Err(ConfigError::Migration(MigrationError::new(format!(
                        "GLES configuration is retired ({} is set). Migrate to Vulkan or OpenCL: \
                         set GBTC_BACKEND=vulkan (or opencl), remove GBTC_GLES_KERNEL, \
                         GBTC_GLES_LOCAL_SIZE and GBTC_GLES_AUTOTUNE, and use \
                         GBTC_VULKAN_* / GBTC_OPENCL_* settings instead. \
                         See docs/RUST-COMPATIBILITY.md section 4 and section 7.",
                        name
                    ))));
                }
            }
        }

        let probe_only = env_flag("GBTC_PROBE_ONLY");
        let bench_only = env_flag("GBTC_BENCH_ONLY");

        if probe_only && bench_only {
            return Err(ConfigError::InvalidValue {
                name: "GBTC_PROBE_ONLY".to_string(),
                message: "GBTC_PROBE_ONLY and GBTC_BENCH_ONLY are mutually exclusive".to_string(),
            });
        }

        // BTC_ADDRESS required only for actual mining.
        let btc_address = match std::env::var("BTC_ADDRESS") {
            Ok(v) if !v.trim().is_empty() => v,
            _ if probe_only || bench_only => String::new(),
            _ => {
                return Err(ConfigError::Missing {
                    name: "BTC_ADDRESS".to_string(),
                })
            }
        };

        let pool_url = match std::env::var("POOL_URL") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => "stratum+tcp://public-pool.io:3333".to_string(),
        };

        let worker_name = match std::env::var("WORKER_NAME") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => "x".to_string(),
        };

        let backend_raw = std::env::var("GBTC_BACKEND").unwrap_or_default();
        let backend = Backend::parse(if backend_raw.trim().is_empty() {
            "auto"
        } else {
            backend_raw.trim()
        })?;

        let device = match std::env::var("GBTC_DEVICE") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => "/dev/dri/renderD128".to_string(),
        };

        let batch_nonces = parse_batch_nonces(std::env::var("GBTC_BATCH_NONCES").ok())?;

        let bench_seconds = parse_seconds(
            std::env::var("GBTC_BENCH_SECONDS").ok(),
            60,
            "GBTC_BENCH_SECONDS",
        )?;

        let bench_warmup_seconds = parse_seconds(
            std::env::var("GBTC_BENCH_WARMUP_SECONDS").ok(),
            5,
            "GBTC_BENCH_WARMUP_SECONDS",
        )?;

        let http_bind_raw = std::env::var("GBTC_HTTP_BIND").unwrap_or_default();
        let http_bind = parse_http_bind(if http_bind_raw.trim().is_empty() {
            "127.0.0.1"
        } else {
            http_bind_raw.trim()
        })?;

        let http_port = parse_http_port(std::env::var("GBTC_HTTP_PORT").ok())?;

        // OpenCL settings stored as strings/Options.
        let opencl_local_size = non_empty_env("GBTC_OPENCL_LOCAL_SIZE");
        let opencl_poll_us = non_empty_env("GBTC_OPENCL_POLL_US");
        let opencl_simd = non_empty_env("GBTC_OPENCL_SIMD");
        let opencl_kernel = non_empty_env("GBTC_OPENCL_KERNEL");
        let opencl_debug = non_empty_env("GBTC_OPENCL_DEBUG");
        let opencl_platform = non_empty_env("GBTC_OPENCL_PLATFORM");

        let vulkan_local_size = match non_empty_env("GBTC_VULKAN_LOCAL_SIZE") {
            Some(v) => {
                let trimmed = v.trim();
                if !["8", "16", "32", "64", "128", "256"].contains(&trimmed) {
                    return Err(ConfigError::InvalidValue {
                        name: "GBTC_VULKAN_LOCAL_SIZE".to_string(),
                        message: "GBTC_VULKAN_LOCAL_SIZE must be one of 8, 16, 32, 64, 128, 256"
                            .to_string(),
                    });
                }
                Some(trimmed.to_string())
            }
            None => None,
        };
        let vulkan_poll_us = match non_empty_env("GBTC_VULKAN_POLL_US") {
            Some(v) => {
                let trimmed = v.trim();
                let parsed: u64 = trimmed.parse().map_err(|_| ConfigError::InvalidValue {
                    name: "GBTC_VULKAN_POLL_US".to_string(),
                    message:
                        "GBTC_VULKAN_POLL_US must be an integer from 100 to 50000 microseconds"
                            .to_string(),
                })?;
                if !(100..=50000).contains(&parsed) {
                    return Err(ConfigError::InvalidValue {
                        name: "GBTC_VULKAN_POLL_US".to_string(),
                        message:
                            "GBTC_VULKAN_POLL_US must be an integer from 100 to 50000 microseconds"
                                .to_string(),
                    });
                }
                Some(trimmed.to_string())
            }
            None => None,
        };
        let vulkan_validation = non_empty_env("GBTC_VULKAN_VALIDATION");

        let vulkan_wait_mode = match non_empty_env("GBTC_VULKAN_WAIT_MODE") {
            Some(v) => {
                let trimmed = v.trim();
                if trimmed != "fence" && trimmed != "poll" {
                    return Err(ConfigError::InvalidValue {
                        name: "GBTC_VULKAN_WAIT_MODE".to_string(),
                        message: "GBTC_VULKAN_WAIT_MODE must be fence or poll".to_string(),
                    });
                }
                Some(trimmed.to_string())
            }
            None => Some("fence".to_string()),
        };
        let vulkan_wait_us = match non_empty_env("GBTC_VULKAN_WAIT_US") {
            Some(v) => {
                let trimmed = v.trim();
                let parsed: u64 = trimmed.parse().map_err(|_| ConfigError::InvalidValue {
                    name: "GBTC_VULKAN_WAIT_US".to_string(),
                    message:
                        "GBTC_VULKAN_WAIT_US must be an integer from 1000 to 100000 microseconds"
                            .to_string(),
                })?;
                if !(1000..=100000).contains(&parsed) {
                    return Err(ConfigError::InvalidValue {
                        name: "GBTC_VULKAN_WAIT_US".to_string(),
                        message: "GBTC_VULKAN_WAIT_US must be an integer from 1000 to 100000 microseconds"
                            .to_string(),
                    });
                }
                Some(trimmed.to_string())
            }
            None => Some("10000".to_string()),
        };

        let gpu_queue_ms = match std::env::var("GBTC_GPU_QUEUE_MS") {
            Ok(v) if !v.trim().is_empty() => {
                let trimmed = v.trim();
                let parsed: u64 = trimmed.parse().map_err(|_| ConfigError::InvalidValue {
                    name: "GBTC_GPU_QUEUE_MS".to_string(),
                    message: "GBTC_GPU_QUEUE_MS must be an integer from 50 to 1000".to_string(),
                })?;
                if !(50..=1000).contains(&parsed) {
                    return Err(ConfigError::InvalidValue {
                        name: "GBTC_GPU_QUEUE_MS".to_string(),
                        message: "GBTC_GPU_QUEUE_MS must be an integer from 50 to 1000".to_string(),
                    });
                }
                parsed
            }
            _ => 200,
        };

        // Collect any additional GBTC_OPENCL_* / GBTC_VULKAN_* settings.
        let mut opencl_settings = BTreeMap::new();
        let mut vulkan_settings = BTreeMap::new();
        for (k, v) in std::env::vars() {
            if v.trim().is_empty() {
                continue;
            }
            if k.starts_with("GBTC_OPENCL_") {
                opencl_settings.insert(k, v);
            } else if k.starts_with("GBTC_VULKAN_") {
                vulkan_settings.insert(k, v);
            }
        }

        Ok(Config {
            btc_address,
            pool_url,
            worker_name,
            backend,
            probe_only,
            bench_only,
            device,
            batch_nonces,
            bench_seconds,
            bench_warmup_seconds,
            http_bind,
            http_port,
            opencl_local_size,
            opencl_poll_us,
            opencl_simd,
            opencl_kernel,
            opencl_debug,
            opencl_platform,
            vulkan_local_size,
            vulkan_poll_us,
            vulkan_validation,
            vulkan_wait_mode,
            vulkan_wait_us,
            opencl_settings,
            vulkan_settings,
            gpu_queue_ms,
        })
    }

    pub fn auto_backend_order() -> &'static [&'static str] {
        &["vulkan", "opencl"]
    }

    pub fn http_socket_addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::from((self.http_bind.as_ipv4(), self.http_port))
    }
}

fn env_flag(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => false,
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn parse_seconds(
    value: Option<String>,
    default_value: i64,
    name: &str,
) -> Result<i64, ConfigError> {
    match value {
        None => Ok(default_value),
        Some(v) if v.trim().is_empty() => Ok(default_value),
        Some(v) => {
            let trimmed = v.trim();
            // reject non-integer / overflow
            let parsed: i64 = trimmed.parse().map_err(|_| ConfigError::InvalidValue {
                name: name.to_string(),
                message: format!("{} must be an integer from 1 to 86400 seconds", name),
            })?;
            if !(1..=86400).contains(&parsed) {
                return Err(ConfigError::InvalidValue {
                    name: name.to_string(),
                    message: format!("{} must be an integer from 1 to 86400 seconds", name),
                });
            }
            Ok(parsed)
        }
    }
}

fn parse_batch_nonces(value: Option<String>) -> Result<u32, ConfigError> {
    let parsed = match value {
        None => 1u32 << 24,
        Some(v) if v.trim().is_empty() => 1u32 << 24,
        Some(v) => {
            let trimmed = v.trim();
            // reject signs/whitespace already trimmed; parse u64 then range
            let n: u64 = trimmed.parse().map_err(|_| ConfigError::InvalidValue {
                name: "GBTC_BATCH_NONCES".to_string(),
                message: "GBTC_BATCH_NONCES must be a power-of-two value between 1 and 1073741824"
                    .to_string(),
            })?;
            if n == 0 || n > (1u64 << 30) {
                return Err(ConfigError::InvalidValue {
                    name: "GBTC_BATCH_NONCES".to_string(),
                    message:
                        "GBTC_BATCH_NONCES must be a power-of-two value between 1 and 1073741824"
                            .to_string(),
                });
            }
            n as u32
        }
    };

    if parsed == 0 || (parsed & (parsed - 1)) != 0 {
        return Err(ConfigError::InvalidValue {
            name: "GBTC_BATCH_NONCES".to_string(),
            message: "GBTC_BATCH_NONCES must be a power of two".to_string(),
        });
    }
    Ok(parsed)
}

fn parse_http_port(value: Option<String>) -> Result<u16, ConfigError> {
    match value {
        None => Ok(41174),
        Some(v) if v.trim().is_empty() => Ok(41174),
        Some(v) => {
            let trimmed = v.trim();
            let parsed: u64 = trimmed.parse().map_err(|_| ConfigError::InvalidValue {
                name: "GBTC_HTTP_PORT".to_string(),
                message: "GBTC_HTTP_PORT must be an integer from 1 to 65535".to_string(),
            })?;
            if !(1..=65535).contains(&parsed) {
                return Err(ConfigError::InvalidValue {
                    name: "GBTC_HTTP_PORT".to_string(),
                    message: "GBTC_HTTP_PORT must be an integer from 1 to 65535".to_string(),
                });
            }
            Ok(parsed as u16)
        }
    }
}

fn parse_http_bind(value: &str) -> Result<HttpBind, ConfigError> {
    match Ipv4Addr::from_str(value) {
        Ok(addr) => Ok(HttpBind(addr)),
        Err(_) => Err(ConfigError::InvalidHttpBind(format!(
            "GBTC_HTTP_BIND must be a valid IPv4 address (got {:?})",
            value
        ))),
    }
}
