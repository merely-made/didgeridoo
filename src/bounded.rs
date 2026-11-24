//! A bounded run: step an executor until it finishes, spends its fuel or grows
//! the heap past a ceiling.

use std::fmt;

use crate::{Fuel, Lua, StashedExecutor};

/// Fuel handed to one `Executor::step`. Between steps the arena is left, which
/// lets the collector run during a long evaluation.
const FUEL_PER_STEP: i32 = 4096;

/// What bounds one [`drive`]: fuel and heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriveLimits {
    /// Fuel the run may spend; `None` has no fuel limit.
    pub max_ops: Option<u64>,
    /// The most memory, in bytes, the Lua state may hold. Checked even when
    /// `max_ops` is `None`.
    pub max_heap_bytes: usize,
}

/// Why [`drive`] stopped without a finished run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveError {
    /// The fuel ran out; carries the budget.
    OutOfFuel { max_ops: u64 },
    /// The state grew past the ceiling; carries the ceiling.
    OverHeap { max_heap_bytes: usize },
    /// The VM failed to step, in its words. A script's own error is not this:
    /// it finishes the run and is read from the executor.
    Runtime(String),
}

impl fmt::Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfFuel { max_ops } => write!(
                f,
                "didgeridoo refused: the script ran past its budget of {max_ops} operations"
            ),
            Self::OverHeap { max_heap_bytes } => write!(
                f,
                "didgeridoo refused: the script used more than {max_heap_bytes} bytes of memory"
            ),
            Self::Runtime(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DriveError {}

/// Step `executor` until it finishes, spends `limits.max_ops` fuel or grows the
/// heap past `limits.max_heap_bytes`. Returns the fuel spent.
///
/// # Errors
///
/// [`DriveError`]: out of fuel, over the heap ceiling, or a script error.
pub fn drive(
    lua: &mut Lua,
    executor: &StashedExecutor,
    limits: DriveLimits,
) -> Result<u64, DriveError> {
    let mut left = limits.max_ops;
    if left == Some(0) {
        return Err(DriveError::OutOfFuel { max_ops: 0 });
    }
    let mut used: u64 = 0;
    loop {
        let slice = left
            .and_then(|left| i32::try_from(left).ok())
            .unwrap_or(FUEL_PER_STEP)
            .min(FUEL_PER_STEP);
        let mut fuel = Fuel::with(slice);
        let finished = lua
            .enter(|ctx| ctx.fetch(executor).step(ctx, &mut fuel))
            .map_err(|e| DriveError::Runtime(e.to_string()))?;
        // A callback may overdraw the slice, so remaining fuel can be negative.
        let spent = u64::try_from(i64::from(slice) - i64::from(fuel.remaining())).unwrap_or(0);
        used = used.saturating_add(spent);
        if finished {
            return Ok(used);
        }
        if lua.total_memory() > limits.max_heap_bytes {
            return Err(DriveError::OverHeap {
                max_heap_bytes: limits.max_heap_bytes,
            });
        }
        if let (Some(remaining), Some(max_ops)) = (left, limits.max_ops) {
            // At least one unit a step, so an interrupt cannot spin forever.
            let remaining = remaining.saturating_sub(spent.max(1));
            if remaining == 0 {
                return Err(DriveError::OutOfFuel { max_ops });
            }
            left = Some(remaining);
        }
    }
}
