//! WebAssembly contract runtime backed by wasmtime.
//!
//! Declares and re-exports the runtime submodules, gated behind the `runtime` feature.

#[cfg(feature = "runtime")]
pub mod config;
#[cfg(feature = "runtime")]
pub use config::{ResolvedMachineSpec, WasmLimits};
#[cfg(feature = "runtime")]
pub mod contract_runtime;
#[cfg(feature = "runtime")]
pub use contract_runtime::{CompiledModule, ContractRuntime};
#[cfg(feature = "runtime")]
pub mod error;
#[cfg(feature = "runtime")]
pub use error::{ContractError, InvalidModuleKind, RuntimeError};
#[cfg(feature = "runtime")]
pub mod executor;
#[cfg(feature = "runtime")]
pub use executor::{ExecutionResult, ExecutionStats};
#[cfg(feature = "runtime")]
pub mod host;
#[cfg(all(feature = "runtime", feature = "prometheus"))]
pub mod metrics;
#[cfg(all(feature = "runtime", feature = "prometheus"))]
pub use metrics::ContractMetrics;
#[cfg(feature = "runtime")]
/// `Cargo.toml` template for generated contract crates, embedded at compile time.
pub const CONTRACT_CARGO_TOML: &str = include_str!("contract_Cargo.toml");
#[cfg(feature = "runtime")]
/// Cargo config template for generated contract crates, embedded at compile time.
pub const CONTRACT_CARGO_CONFIG: &str = include_str!("contract_cargo_config.toml");

#[cfg(all(test, feature = "runtime"))]
mod tests;
