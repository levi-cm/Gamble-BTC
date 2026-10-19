use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    InvalidInput(String),
    Unavailable(String),
    Timeout(String),
    DeviceLost(String),
    DriverFailure(String),
}

impl BackendError {
    pub fn diagnostic(&self) -> &str {
        match self {
            BackendError::InvalidInput(s)
            | BackendError::Unavailable(s)
            | BackendError::Timeout(s)
            | BackendError::DeviceLost(s)
            | BackendError::DriverFailure(s) => s.as_str(),
        }
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        BackendError::InvalidInput(msg.into())
    }
    pub fn unavailable(msg: impl Into<String>) -> Self {
        BackendError::Unavailable(msg.into())
    }
    pub fn timeout(msg: impl Into<String>) -> Self {
        BackendError::Timeout(msg.into())
    }
    pub fn device_lost(msg: impl Into<String>) -> Self {
        BackendError::DeviceLost(msg.into())
    }
    pub fn driver_failure(msg: impl Into<String>) -> Self {
        BackendError::DriverFailure(msg.into())
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            BackendError::InvalidInput(_) => "invalid input",
            BackendError::Unavailable(_) => "unavailable",
            BackendError::Timeout(_) => "timeout",
            BackendError::DeviceLost(_) => "device lost",
            BackendError::DriverFailure(_) => "driver failure",
        };
        write!(f, "{}: {}", kind, self.diagnostic())
    }
}

impl std::error::Error for BackendError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendInfo {
    pub backend: String,
    pub api: String,
    pub device: String,
    pub driver: String,
}

impl BackendInfo {
    pub fn new(
        backend: impl Into<String>,
        api: impl Into<String>,
        device: impl Into<String>,
        driver: impl Into<String>,
    ) -> Self {
        Self {
            backend: backend.into(),
            api: api.into(),
            device: device.into(),
            driver: driver.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkBatch {
    pub midstate: [u32; 8],
    pub tail3: [u32; 3],
    pub target: [u32; 8],
    pub nonce_base: u32,
    pub nonce_count: u32,
}

impl WorkBatch {
    pub fn new(
        midstate: [u32; 8],
        tail3: [u32; 3],
        target: [u32; 8],
        nonce_base: u32,
        nonce_count: u32,
    ) -> Self {
        Self {
            midstate,
            tail3,
            target,
            nonce_base,
            nonce_count,
        }
    }

    pub fn validate(&self) -> Result<(), BackendError> {
        if self.nonce_count == 0 {
            return Err(BackendError::invalid_input(
                "nonce_count must be greater than zero",
            ));
        }
        let end = (self.nonce_base as u64) + (self.nonce_count as u64);
        if end > (1u64 << 32) {
            return Err(BackendError::invalid_input(
                "nonce range exceeds 2^32 without wrapping",
            ));
        }
        Ok(())
    }
}

pub fn validate_work_batch(work: &WorkBatch) -> Result<(), BackendError> {
    work.validate()
}

pub fn nonce_range_valid(base: u32, count: u32) -> bool {
    if count == 0 {
        return false;
    }
    (base as u64) + (count as u64) <= (1u64 << 32)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchResult {
    pub candidates: Vec<u32>,
    pub unique_hashes: u64,
    pub overflow_rescans: u32,
}

impl BatchResult {
    pub fn new(candidates: Vec<u32>, unique_hashes: u64, overflow_rescans: u32) -> Self {
        Self {
            candidates,
            unique_hashes,
            overflow_rescans,
        }
    }
}

/// Identifies one outstanding batch. Backends reject unknown or retired tickets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BatchTicket(pub u64);

/// Cumulative diagnostics. All host times are wall time, not process CPU time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackendTelemetry {
    pub submissions: u64,
    pub completed_batches: u64,
    pub fence_waits: u64,
    pub poll_checks: u64,
    pub submit_wall_ns: u64,
    pub wait_wall_ns: u64,
    pub gpu_ns: Option<u64>,
    pub overflow_rescans: u64,
}

pub trait ComputeBackend {
    fn run_batch(&mut self, work: &WorkBatch) -> Result<BatchResult, BackendError>;

    /// A capacity of one keeps legacy/synchronous implementations valid.
    fn queue_capacity(&self) -> usize {
        1
    }

    fn submit_batch(&mut self, _work: &WorkBatch) -> Result<BatchTicket, BackendError> {
        Err(BackendError::unavailable("queued submission unsupported"))
    }

    fn retire_batch(&mut self, _ticket: BatchTicket) -> Result<BatchResult, BackendError> {
        Err(BackendError::unavailable("queued retirement unsupported"))
    }

    fn telemetry(&self) -> BackendTelemetry {
        BackendTelemetry::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_count() {
        assert!(!nonce_range_valid(0, 0));
    }

    #[test]
    fn accepts_full_range_from_zero() {
        assert!(nonce_range_valid(0, u32::MAX));
    }

    #[test]
    fn rejects_overflowing_range() {
        assert!(!nonce_range_valid(2, u32::MAX));
    }

    #[test]
    fn accepts_boundary_range() {
        assert!(nonce_range_valid(1, u32::MAX));
    }

    #[test]
    fn work_batch_validate() {
        let wb = WorkBatch {
            midstate: [0; 8],
            tail3: [0; 3],
            target: [0; 8],
            nonce_base: 0,
            nonce_count: 1,
        };
        assert!(wb.validate().is_ok());
        let wb0 = WorkBatch {
            nonce_count: 0,
            ..wb.clone()
        };
        assert!(wb0.validate().is_err());
        let wbw = WorkBatch {
            nonce_base: 2,
            nonce_count: u32::MAX,
            ..wb.clone()
        };
        assert!(wbw.validate().is_err());
    }
}
