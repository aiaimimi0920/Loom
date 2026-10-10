//! Per-job resource limits and cooperative cancellation for untrusted images.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const MAX_DIMENSION: u32 = 8192;
pub const MAX_PIXELS: u64 = 8 * 1024 * 1024;
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RADIUS: u32 = 128;

#[derive(Clone, Copy)]
pub(crate) struct Budget<'a> {
    deadline: Instant,
    cancellation: Option<&'a AtomicBool>,
}
impl<'a> Budget<'a> {
    pub(crate) fn new(deadline: Option<Instant>, cancellation: Option<&'a AtomicBool>) -> Self {
        let maximum = Instant::now() + Duration::from_secs(15);
        Self {
            deadline: deadline.map_or(maximum, |d| d.min(maximum)),
            cancellation,
        }
    }

    pub(crate) fn check(self) -> Result<(), String> {
        if self
            .cancellation
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            return Err("native image cancelled".to_owned());
        }
        if Instant::now() >= self.deadline {
            return Err("native image deadline exceeded".to_owned());
        }
        Ok(())
    }
}

pub(crate) fn validate_surface(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("native image surface limit exceeded".to_owned());
    }
    Ok(())
}

pub(crate) fn integer_parameter(
    value: Option<&serde_json::Value>,
    default: u32,
    maximum: u32,
    name: &str,
) -> Result<u32, String> {
    let Some(value) = value else {
        return Ok(default);
    };
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| n <= maximum)
        .ok_or_else(|| format!("invalid {name}: expected integer in 0..={maximum}"))
}
