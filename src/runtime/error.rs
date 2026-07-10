//! Error types for contract execution, validation, and host memory operations.

use thiserror::Error;

/// Errors produced by the host-side memory manager and linker.
#[derive(Debug, Error, Clone)]
pub enum ContractError {
    /// An allocation request could not be satisfied.
    #[error("memory allocation failed: {details}")]
    MemoryAllocationFailed { details: String },

    /// A pointer does not reference a tracked allocation.
    #[error("invalid pointer: {pointer}")]
    InvalidPointer { pointer: usize },

    /// A write would exceed the bounds of the target allocation.
    #[error("write out of bounds: offset {offset} >= allocation size {size}")]
    WriteOutOfBounds { offset: usize, size: usize },

    /// A single allocation exceeds the per-allocation limit.
    #[error("allocation size {size} exceeds maximum of {max} bytes")]
    AllocationTooLarge { size: usize, max: usize },

    /// Total allocated memory exceeds the configured limit.
    #[error("total memory {total} exceeds maximum of {max} bytes")]
    TotalMemoryExceeded { total: usize, max: usize },

    /// An allocation size addition would overflow.
    #[error("memory allocation would overflow")]
    AllocationOverflow,

    /// A host function could not be registered with the linker.
    #[error("linker error [{function}]: {details}")]
    LinkerError {
        function: &'static str,
        details: String,
    },
}

/// Errors produced while creating the engine, compiling, validating, or executing a contract.
#[derive(Debug, Error, Clone)]
pub enum RuntimeError {
    /// The wasmtime engine could not be created.
    #[error("engine creation failed: {0}")]
    EngineCreation(String),

    /// Precompiling the WASM module failed.
    #[error("wasm precompile failed: {0}")]
    PrecompileFailed(String),

    /// Deserializing a precompiled module failed.
    #[error("wasm deserialization failed: {0}")]
    DeserializationFailed(String),

    /// The module's imports are not exactly the SDK set.
    #[error("invalid module: {0}")]
    InvalidModule(InvalidModuleKind),

    /// A required entry point is missing from the module.
    #[error("entry point not found: {function}")]
    EntryPointNotFound { function: &'static str },

    /// The contract trapped or returned an error during execution.
    #[error("contract execution failed: {0}")]
    ContractExecutionFailed(String),

    /// The store fuel could not be configured.
    #[error("fuel limit error: {0}")]
    FuelLimitError(String),

    /// The module could not be instantiated.
    #[error("instantiation failed: {0}")]
    InstantiationFailed(String),

    /// A host memory allocation failed.
    #[error("memory allocation failed: {0}")]
    MemoryAllocationFailed(String),

    /// (De)serialization of contract data failed.
    #[error("serialization error [{context}]: {details}")]
    SerializationError {
        context: &'static str,
        details: String,
    },
}

/// Reason a module failed SDK import validation.
#[derive(Debug, Clone)]
pub enum InvalidModuleKind {
    /// The module imports a function the SDK does not provide.
    UnknownImportFunction { name: String },
    /// The module imports something that is not a function.
    NonFunctionImport { import_type: String },
    /// The module is missing one or more required SDK imports.
    MissingImports { missing: Vec<String> },
}

impl std::fmt::Display for InvalidModuleKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownImportFunction { name } => write!(
                f,
                "module has function '{}' that is not contemplated in the SDK",
                name
            ),
            Self::NonFunctionImport { import_type } => write!(
                f,
                "module has a '{}' import that is not a function",
                import_type
            ),
            Self::MissingImports { missing } => {
                write!(f, "module is missing SDK imports: {}", missing.join(", "))
            }
        }
    }
}
