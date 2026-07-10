//! wasmtime-backed runtime that compiles, validates, and executes contract modules.

use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
    time::Instant,
};

use ave_common::{
    ContractData, ContractInitCheckData, ContractResultData, ValueWrapper,
    identity::{DigestIdentifier, HashAlgorithm, hash_borsh},
};
use borsh::{BorshDeserialize, to_vec};
use wasmtime::{ExternType, Linker, Module, Store, Trap};

use crate::runtime::{
    InvalidModuleKind, ResolvedMachineSpec, RuntimeError, WasmLimits,
    config::{MAX_FUEL, MAX_FUEL_COMPILATION, create_secure_wasmtime_config},
    executor::{ExecutionResult, ExecutionStats},
    host::{MemoryManager, generate_linker},
    metrics::ContractMetrics,
};

const SDK_FUNCTIONS: &[&str] = &["pointer_len", "alloc", "read_bytes", "write_bytes"];

/// A precompiled contract module ready for validation and execution.
pub struct CompiledModule {
    module: Arc<Module>,
    precompiled_bytes: Vec<u8>,
}

impl CompiledModule {
    pub(crate) fn inner(&self) -> &Module {
        &self.module
    }

    /// Returns the engine-serialized (precompiled) bytes of this module.
    pub fn precompiled_bytes(&self) -> &[u8] {
        &self.precompiled_bytes
    }
}

/// A configured contract runtime with its engine, limits, and optional metrics.
pub struct ContractRuntime {
    engine: wasmtime::Engine,
    limits: WasmLimits,
    linker: Linker<MemoryManager>,
    metrics: Option<Arc<ContractMetrics>>,
}

impl ContractRuntime {
    /// Creates a runtime, deriving limits from `spec` or the defaults when `spec` is `None`.
    pub fn new(spec: Option<ResolvedMachineSpec>) -> Result<Self, RuntimeError> {
        let limits = spec.map_or_else(WasmLimits::default, |s| {
            WasmLimits::build(s.ram_mb, s.cpu_cores)
        });
        let engine = wasmtime::Engine::new(&create_secure_wasmtime_config(&limits))
            .map_err(|e| RuntimeError::EngineCreation(e.to_string()))?;
        let linker = generate_linker(&engine).map_err(|e| {
            RuntimeError::EngineCreation(format!("linker setup failed: {e}"))
        })?;
        Ok(Self {
            engine,
            limits,
            linker,
            metrics: None,
        })
    }

    /// Creates a runtime like `new` and attaches the given Prometheus `metrics`.
    pub fn with_metrics(
        spec: Option<ResolvedMachineSpec>,
        metrics: Option<Arc<ContractMetrics>>,
    ) -> Result<Self, RuntimeError> {
        let mut runtime = Self::new(spec)?;
        runtime.metrics = metrics;
        Ok(runtime)
    }

    /// Returns the underlying wasmtime engine.
    pub fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }

    /// Returns the limits this runtime was built with.
    pub fn limits(&self) -> &WasmLimits {
        &self.limits
    }

    /// Precompiles `wasm_bytes` and deserializes them into a `CompiledModule`.
    ///
    /// # Safety
    ///
    /// The precompiled bytes are produced by this engine's own `precompile_module` and
    /// immediately deserialized with the same engine, so they are guaranteed compatible
    /// with `self.engine`.
    pub fn compile(&self, wasm_bytes: &[u8]) -> Result<CompiledModule, RuntimeError> {
        let precompiled_bytes = self
            .engine
            .precompile_module(wasm_bytes)
            .map_err(|e| RuntimeError::PrecompileFailed(e.to_string()))?;

        let module = unsafe {
            Module::deserialize(&self.engine, &precompiled_bytes)
                .map_err(|e| RuntimeError::DeserializationFailed(e.to_string()))?
        };

        Ok(CompiledModule {
            module: Arc::new(module),
            precompiled_bytes,
        })
    }

    /// Loads a module from caller-supplied precompiled bytes.
    ///
    /// # Safety
    ///
    /// `precompiled_bytes` must have been produced by `Engine::precompile_module` on an
    /// engine compatible with this one (same wasmtime version and configuration).
    /// Deserializing bytes from any other source is undefined behavior in wasmtime.
    pub fn load_precompiled(
        &self,
        precompiled_bytes: &[u8],
    ) -> Result<CompiledModule, RuntimeError> {
        let module = unsafe {
            Module::deserialize(&self.engine, precompiled_bytes).map_err(
                |e| RuntimeError::DeserializationFailed(e.to_string()),
            )?
        };

        Ok(CompiledModule {
            module: Arc::new(module),
            precompiled_bytes: precompiled_bytes.to_vec(),
        })
    }

    /// Validates that `module` imports exactly the SDK functions, exposes `main_function`
    /// and `init_check_function`, and accepts `initial_state`.
    pub fn validate(
        &self,
        module: &CompiledModule,
        initial_state: &ValueWrapper,
    ) -> Result<(), RuntimeError> {
        self.validate_inner(module, initial_state)
    }

    fn validate_inner(
        &self,
        module: &CompiledModule,
        initial_state: &ValueWrapper,
    ) -> Result<(), RuntimeError> {
        let imports = module.inner().imports();
        let mut pending_sdk: HashSet<&str> = SDK_FUNCTIONS.iter().copied().collect();

        for import in imports {
            match import.ty() {
                ExternType::Func(_) => {
                    if !pending_sdk.remove(import.name()) {
                        return Err(RuntimeError::InvalidModule(
                            InvalidModuleKind::UnknownImportFunction {
                                name: import.name().to_string(),
                            },
                        ));
                    }
                }
                extern_type => {
                    return Err(RuntimeError::InvalidModule(
                        InvalidModuleKind::NonFunctionImport {
                            import_type: format!("{:?}", extern_type),
                        },
                    ));
                }
            }
        }

        if !pending_sdk.is_empty() {
            return Err(RuntimeError::InvalidModule(
                InvalidModuleKind::MissingImports {
                    missing: pending_sdk.into_iter().map(|s| s.to_string()).collect(),
                },
            ));
        }

        let (context, state_ptr) = generate_context(initial_state, &self.limits)?;
        let mut store = Store::new(&self.engine, context);
        store.limiter(|data| &mut data.store_limits);
        store
            .set_fuel(MAX_FUEL_COMPILATION)
            .map_err(|e| RuntimeError::FuelLimitError(e.to_string()))?;

        let instance = self
            .linker
            .instantiate(&mut store, module.inner())
            .map_err(|e| RuntimeError::InstantiationFailed(e.to_string()))?;

        let _ = instance
            .get_typed_func::<(u32, u32, u32, u32), u32>(&mut store, "main_function")
            .map_err(|_| RuntimeError::EntryPointNotFound {
                function: "main_function",
            })?;

        let init_contract_entrypoint = instance
            .get_typed_func::<u32, u32>(&mut store, "init_check_function")
            .map_err(|_| RuntimeError::EntryPointNotFound {
                function: "init_check_function",
            })?;

        let result_ptr = init_contract_entrypoint
            .call(&mut store, state_ptr)
            .map_err(|e| RuntimeError::ContractExecutionFailed(e.to_string()))?;

        check_init_result(&store, result_ptr)?;
        Ok(())
    }

    /// Executes `module`'s `main_function` with the given state, initial state, event, and
    /// ownership flag under fuel and memory limits.
    ///
    /// Returns the execution result together with resource-usage statistics, and records
    /// execution metrics when enabled.
    pub fn execute(
        &self,
        module: &CompiledModule,
        state: &ValueWrapper,
        init_state: &ValueWrapper,
        event: &ValueWrapper,
        is_owner: bool,
    ) -> Result<(ExecutionResult, ExecutionStats), RuntimeError> {
        let started_at = Instant::now();
        let result = self.execute_inner(module, state, init_state, event, is_owner);

        if let Some(metrics) = &self.metrics {
            let result_label = if result.is_ok() { "success" } else { "error" };
            metrics.observe_contract_execution(result_label, started_at.elapsed());
        }

        result
    }

    fn execute_inner(
        &self,
        module: &CompiledModule,
        state: &ValueWrapper,
        init_state: &ValueWrapper,
        event: &ValueWrapper,
        is_owner: bool,
    ) -> Result<(ExecutionResult, ExecutionStats), RuntimeError> {
        let (context, state_ptr, init_state_ptr, event_ptr) =
            generate_execution_context(state, init_state, event, &self.limits)?;

        let mut store = Store::new(&self.engine, context);
        store.limiter(|data| &mut data.store_limits);
        store
            .set_fuel(MAX_FUEL)
            .map_err(|e| RuntimeError::FuelLimitError(e.to_string()))?;

        let instance = self
            .linker
            .instantiate(&mut store, module.inner())
            .map_err(|e| RuntimeError::InstantiationFailed(e.to_string()))?;

        let contract_entrypoint = instance
            .get_typed_func::<(u32, u32, u32, u32), u32>(&mut store, "main_function")
            .map_err(|_| RuntimeError::EntryPointNotFound {
                function: "main_function",
            })?;

        let call_result = contract_entrypoint.call(
            &mut store,
            (
                state_ptr,
                init_state_ptr,
                event_ptr,
                if is_owner { 1 } else { 0 },
            ),
        );

        let remaining = store.get_fuel().unwrap_or(0);
        let fuel_consumed = MAX_FUEL.saturating_sub(remaining);
        let memory_bytes = instance
            .get_memory(&mut store, "memory")
            .map(|m| m.size(&store) * 64 * 1024)
            .unwrap_or(0);

        let fuel_exhausted = matches!(
            call_result
                .as_ref()
                .err()
                .and_then(|e| e.downcast_ref::<Trap>()),
            Some(&Trap::OutOfFuel)
        );

        if let Some(metrics) = &self.metrics {
            let result_label = if call_result.is_ok() {
                "success"
            } else {
                "error"
            };
            metrics.observe_contract_fuel_consumed(result_label, fuel_consumed);
            metrics.observe_contract_memory_peak(result_label, memory_bytes);
            if fuel_exhausted {
                metrics.observe_contract_fuel_exhausted();
            }
        }

        match call_result {
            Ok(result_ptr) => {
                let result = get_execution_result(&store, result_ptr)?;
                Ok((
                    result,
                    ExecutionStats {
                        fuel_consumed,
                        memory_bytes: memory_bytes as usize,
                        fuel_exhausted: false,
                    },
                ))
            }
            Err(e) => Err(RuntimeError::ContractExecutionFailed(e.to_string())),
        }
    }

    /// Hashes the engine's precompile-compatibility value into a `DigestIdentifier` using `hash`.
    pub fn engine_fingerprint(
        &self,
        hash: HashAlgorithm,
    ) -> Result<DigestIdentifier, RuntimeError> {
        let mut hasher = DefaultHasher::new();
        self.engine
            .precompile_compatibility_hash()
            .hash(&mut hasher);
        hash_borsh(&*hash.hasher(), &hasher.finish()).map_err(|e| {
            RuntimeError::SerializationError {
                context: "engine fingerprint",
                details: e.to_string(),
            }
        })
    }
}

fn value_to_contract_data(value: &ValueWrapper) -> Result<ContractData, RuntimeError> {
    serde_json::to_vec(&value.0)
        .map(ContractData)
        .map_err(|e| RuntimeError::SerializationError {
            context: "value to contract data",
            details: e.to_string(),
        })
}

fn generate_context(
    state: &ValueWrapper,
    limits: &WasmLimits,
) -> Result<(MemoryManager, u32), RuntimeError> {
    let mut context = MemoryManager::from_limits(limits);
    let data = value_to_contract_data(state)?;
    let bytes = to_vec(&data).map_err(|e| RuntimeError::SerializationError {
        context: "context borsh",
        details: e.to_string(),
    })?;
    let ptr = context
        .alloc(bytes.len())
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;
    context
        .write_bytes(ptr, &bytes)
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;
    Ok((context, ptr as u32))
}

fn generate_execution_context(
    state: &ValueWrapper,
    init_state: &ValueWrapper,
    event: &ValueWrapper,
    limits: &WasmLimits,
) -> Result<(MemoryManager, u32, u32, u32), RuntimeError> {
    let mut context = MemoryManager::from_limits(limits);

    let state_data = value_to_contract_data(state)?;
    let state_bytes = to_vec(&state_data).map_err(|e| RuntimeError::SerializationError {
        context: "state borsh",
        details: e.to_string(),
    })?;
    let state_ptr = context
        .alloc(state_bytes.len())
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;
    context
        .write_bytes(state_ptr, &state_bytes)
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;

    let init_data = value_to_contract_data(init_state)?;
    let init_bytes = to_vec(&init_data).map_err(|e| RuntimeError::SerializationError {
        context: "init state borsh",
        details: e.to_string(),
    })?;
    let init_ptr = context
        .alloc(init_bytes.len())
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;
    context
        .write_bytes(init_ptr, &init_bytes)
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;

    let event_data = value_to_contract_data(event)?;
    let event_bytes = to_vec(&event_data).map_err(|e| RuntimeError::SerializationError {
        context: "event borsh",
        details: e.to_string(),
    })?;
    let event_ptr = context
        .alloc(event_bytes.len())
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;
    context
        .write_bytes(event_ptr, &event_bytes)
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;

    Ok((context, state_ptr as u32, init_ptr as u32, event_ptr as u32))
}

fn check_init_result(store: &Store<MemoryManager>, result_ptr: u32) -> Result<(), RuntimeError> {
    let memory = store
        .data()
        .read_data(result_ptr as usize)
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;
    let check = ContractInitCheckData::try_from_slice(memory).map_err(|e| {
        RuntimeError::SerializationError {
            context: "init check result",
            details: e.to_string(),
        }
    })?;
    if check.success {
        Ok(())
    } else {
        Err(RuntimeError::ContractExecutionFailed(check.error))
    }
}

fn get_execution_result(
    store: &Store<MemoryManager>,
    result_ptr: u32,
) -> Result<ExecutionResult, RuntimeError> {
    let memory = store
        .data()
        .read_data(result_ptr as usize)
        .map_err(|e| RuntimeError::MemoryAllocationFailed(e.to_string()))?;
    let result = ContractResultData::try_from_slice(memory).map_err(|e| {
        RuntimeError::SerializationError {
            context: "execution result",
            details: e.to_string(),
        }
    })?;
    let final_state = serde_json::from_slice(&result.final_state.0)
        .map(ValueWrapper)
        .map_err(|e| RuntimeError::SerializationError {
            context: "final state json",
            details: e.to_string(),
        })?;
    Ok(ExecutionResult {
        final_state,
        success: result.success,
        error: result.error,
    })
}
