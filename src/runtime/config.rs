//! Resource limits and engine configuration for the wasmtime runtime.
//!
//! Derives memory, stack, table, and fuel limits from a machine specification and
//! builds the corresponding wasmtime `Config`.

use wasmtime::{Config, OptLevel};

/// Detected machine resources used to size the WASM limits.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedMachineSpec {
    /// Available RAM in megabytes.
    pub ram_mb: u64,
    /// Available CPU cores.
    pub cpu_cores: usize,
}

/// Resource limits applied to a contract execution environment.
#[derive(Debug, Clone)]
pub struct WasmLimits {
    /// Maximum WASM stack size in bytes.
    pub max_wasm_stack: usize,
    /// Maximum linear memory size in bytes.
    pub memory_size: usize,
    /// Maximum size of a single allocation in bytes.
    pub max_single_alloc: usize,
    /// Maximum total allocated memory in bytes.
    pub max_total_memory: usize,
    /// Maximum number of table elements.
    pub max_table_elements: usize,
    /// Whether to use the speed-and-size Cranelift optimization level.
    pub aggressive_compilation: bool,
}

impl Default for WasmLimits {
    fn default() -> Self {
        Self::build(4_096, 2)
    }
}

impl WasmLimits {
    /// Builds limits from RAM and CPU counts, clamping each value to a fixed range.
    ///
    /// Stack size is fixed at 1 MiB; memory and allocation limits scale with RAM in
    /// 512 MiB steps and are clamped. `aggressive_compilation` is enabled when
    /// `cpu_cores >= 4`.
    pub fn build(ram_mb: u64, cpu_cores: usize) -> Self {
        let memory_size = ((ram_mb / 512) as usize)
            .saturating_mul(4 * 1024 * 1024)
            .clamp(4 * 1024 * 1024, 32 * 1024 * 1024);

        let max_total_memory = ((ram_mb / 512) as usize)
            .saturating_mul(3 * 1024 * 1024)
            .clamp(3 * 1024 * 1024, 24 * 1024 * 1024);

        let max_single_alloc = (max_total_memory / 3).clamp(1024 * 1024, 8 * 1024 * 1024);

        let max_table_elements = (256 * cpu_cores.max(2)).min(2_048);

        Self {
            max_wasm_stack: 1024 * 1024,
            memory_size,
            max_single_alloc,
            max_total_memory,
            max_table_elements,
            aggressive_compilation: cpu_cores >= 4,
        }
    }
}

/// Fuel budget for a single contract execution.
pub const MAX_FUEL: u64 = 10_000_000;
/// Fuel budget used while validating a module.
pub const MAX_FUEL_COMPILATION: u64 = 50_000_000;

/// Creates a wasmtime `Config` with fuel metering and the given limits applied.
///
/// Selects `OptLevel::SpeedAndSize` when `limits.aggressive_compilation` is set,
/// otherwise `OptLevel::Speed`.
pub fn create_secure_wasmtime_config(limits: &WasmLimits) -> Config {
    let mut config = Config::default();
    config.consume_fuel(true);
    config.max_wasm_stack(limits.max_wasm_stack);
    config.cranelift_opt_level(if limits.aggressive_compilation {
        OptLevel::SpeedAndSize
    } else {
        OptLevel::Speed
    });
    config
}
