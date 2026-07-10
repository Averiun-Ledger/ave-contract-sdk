//! Outcome types produced by executing a contract.

use ave_common::ValueWrapper;

/// Result of running a contract event.
pub struct ExecutionResult {
    /// Final contract state as a JSON value.
    pub final_state: ValueWrapper,
    /// Whether the contract accepted the event.
    pub success: bool,
    /// Rejection reason when `success` is `false`.
    pub error: String,
}

/// Resource-usage statistics captured during an execution.
pub struct ExecutionStats {
    /// Fuel consumed by the call.
    pub fuel_consumed: u64,
    /// Peak linear memory in bytes.
    pub memory_bytes: usize,
    /// Whether the call ran out of fuel.
    pub fuel_exhausted: bool,
}
