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
    /// Retained for compatibility: the Cranelift optimization level is
    /// fixed (`OptLevel::Speed`) for every machine — codegen must not
    /// depend on local hardware, or equal contracts diverge across
    /// nodes. This flag no longer selects anything.
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
    /// 512 MiB steps and are clamped. Only LIMITS scale with the machine:
    /// codegen never does (see `create_secure_wasmtime_config`).
    pub fn build(ram_mb: u64, cpu_cores: usize) -> Self {
        // Math in u64, single saturating conversion at the end: the
        // `as usize` cast it replaces truncates on 32-bit targets.
        let memory_size = usize::try_from(
            (ram_mb / 512)
                .saturating_mul(4 * 1024 * 1024)
                .clamp(4 * 1024 * 1024, MAX_MEMORY_BYTES as u64),
        )
        .unwrap_or(usize::MAX);

        let max_total_memory = usize::try_from(
            (ram_mb / 512)
                .saturating_mul(3 * 1024 * 1024)
                .clamp(3 * 1024 * 1024, 24 * 1024 * 1024),
        )
        .unwrap_or(usize::MAX);

        let max_single_alloc = (max_total_memory / 3).clamp(1024 * 1024, 8 * 1024 * 1024);

        let max_table_elements = (256 * cpu_cores.max(2)).min(2_048);

        Self {
            max_wasm_stack: 1024 * 1024,
            memory_size,
            max_single_alloc,
            max_total_memory,
            max_table_elements,
            aggressive_compilation: false,
        }
    }
}

/// Absolute ceiling for a module's declared minimum linear memory:
/// above it no compliant node can ever instantiate the module (every
/// `WasmLimits::build` clamps to at most this), so the failure is
/// deterministic fleet-wide and must be a verdict, not `Unavailable`.
/// Kept as the single source of truth: `WasmLimits::build` clamps to
/// it and `declared_min_memory_bytes` gates against it.
pub const MAX_MEMORY_BYTES: usize = 32 * 1024 * 1024;

/// WASM memory page size in bytes.
pub const WASM_PAGE_BYTES: u64 = 65_536;

/// Reads the largest declared minimum linear memory (in bytes) from
/// the module's memory section. Returns `None` when the bytes do not
/// parse: malformed modules are rejected downstream (precompile), so
/// the gate only fires on well-formed, provably oversized modules.
pub fn declared_min_memory_bytes(wasm: &[u8]) -> Option<u64> {
    const MAGIC: &[u8; 4] = b"\0asm";
    let cursor = wasm.get(..8)?;
    if cursor.get(..4)? != MAGIC || cursor.get(4..)? != [1, 0, 0, 0] {
        return None;
    }
    let mut pos = 8usize;
    // Small LEB128-u32 reader; `None` on truncation or overflow.
    fn read_uleb(bytes: &[u8], pos: &mut usize) -> Option<u32> {
        let mut result: u32 = 0;
        for shift in (0..35).step_by(7) {
            let byte = *bytes.get(*pos)?;
            *pos += 1;
            result |= ((byte & 0x7F) as u32) << shift;
            if byte & 0x80 == 0 {
                return Some(result);
            }
        }
        None
    }
    let mut max_pages: u64 = 0;
    while pos < wasm.len() {
        let section_id = *wasm.get(pos)?;
        pos += 1;
        let section_len = read_uleb(wasm, &mut pos)? as usize;
        let body = wasm.get(pos..pos.checked_add(section_len)?)?;
        // Memory section: entries of {flags, min[, max]} in pages.
        if section_id == 5 {
            let mut inner = 0usize;
            let count = read_uleb(body, &mut inner)? as usize;
            for _ in 0..count {
                let flags = read_uleb(body, &mut inner)?;
                let min = read_uleb(body, &mut inner)? as u64;
                if flags & 1 != 0 {
                    read_uleb(body, &mut inner)?;
                }
                max_pages = max_pages.max(min);
            }
        }
        pos += section_len;
    }
    Some(max_pages.saturating_mul(WASM_PAGE_BYTES))
}

/// Fuel budget for a single contract execution.
pub const MAX_FUEL: u64 = 10_000_000;
/// Fuel budget used while validating a module.
pub const MAX_FUEL_COMPILATION: u64 = 50_000_000;

/// Creates a wasmtime `Config` with fuel metering and the given limits applied.
///
/// The optimization level is fixed (`OptLevel::Speed`) on every machine:
/// equal wasm must compile to equal native code on all nodes, or float
/// results (NaN bits) diverge and quorum never converges. NaN
/// canonicalization is forced for the same reason. Only LIMITS (memory,
/// tables) scale with the machine — a smaller node fails to instantiate
/// instead of voting a different result.
pub fn create_secure_wasmtime_config(limits: &WasmLimits) -> Config {
    let mut config = Config::default();
    config.consume_fuel(true);
    config.max_wasm_stack(limits.max_wasm_stack);
    config.cranelift_opt_level(OptLevel::Speed);
    config.cranelift_nan_canonicalization(true);
    config
}
