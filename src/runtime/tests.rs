#[cfg(test)]
mod tests {
    use crate::runtime::host::MemoryManager;
    use crate::runtime::{
        ContractError, ContractRuntime, InvalidModuleKind, ResolvedMachineSpec, RuntimeError,
        WasmLimits,
    };

    #[test]
    fn wasm_limits_build_clamps_values() {
        let limits = WasmLimits::build(4096, 4);
        assert_eq!(limits.memory_size, 32 * 1024 * 1024);
        assert_eq!(limits.max_table_elements, 1024);
    }

    #[test]
    fn contract_runtime_creates_engine() {
        let runtime = ContractRuntime::new(Some(ResolvedMachineSpec {
            ram_mb: 4096,
            cpu_cores: 2,
        }));
        assert!(runtime.is_ok());
    }

    #[test]
    fn wasm_limits_build_uses_lower_clamp() {
        let limits = WasmLimits::build(256, 1);
        assert_eq!(limits.memory_size, 4 * 1024 * 1024);
        assert_eq!(limits.max_total_memory, 3 * 1024 * 1024);
        assert_eq!(limits.max_table_elements, 512);
    }

    #[test]
    fn wasm_limits_default_matches_expected() {
        let default = WasmLimits::default();
        let built = WasmLimits::build(4096, 2);
        assert_eq!(default.memory_size, built.memory_size);
        assert_eq!(default.max_table_elements, 512);
    }

    #[test]
    fn memory_manager_alloc_and_read_write_round_trip() {
        let mut manager = MemoryManager::default();
        let ptr = manager.alloc(4).unwrap();
        manager.write_bytes(ptr, &[1, 2, 3, 4]).unwrap();
        assert_eq!(manager.read_data(ptr).unwrap(), &[1, 2, 3, 4]);
        assert_eq!(manager.read_bytes(ptr, 4).unwrap(), &[1, 2, 3, 4]);
    }

    #[test]
    fn memory_manager_alloc_too_large_fails() {
        let limits = WasmLimits::build(512, 2);
        let mut manager = MemoryManager::from_limits(&limits);
        let err = manager.alloc(2 * 1024 * 1024).unwrap_err();
        assert!(matches!(err, ContractError::AllocationTooLarge { .. }));
    }

    #[test]
    fn memory_manager_write_out_of_bounds_fails() {
        let mut manager = MemoryManager::default();
        let ptr = manager.alloc(2).unwrap();
        let err = manager.write_bytes(ptr, &[1, 2, 3, 4]).unwrap_err();
        assert!(matches!(
            err,
            ContractError::WriteOutOfBounds { offset: 4, size: 2 }
        ));
    }

    #[test]
    fn memory_manager_invalid_pointer_fails() {
        let manager = MemoryManager::default();
        let err = manager.read_data(9999).unwrap_err();
        assert!(matches!(
            err,
            ContractError::InvalidPointer { pointer: 9999 }
        ));
    }

    #[test]
    fn contract_runtime_with_metrics_and_accessors() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let runtime = ContractRuntime::with_metrics(None, None).unwrap();
        assert_eq!(
            runtime.limits().memory_size,
            WasmLimits::default().memory_size
        );
        assert_eq!(runtime.limits().max_table_elements, 512);

        let mut runtime_hash = DefaultHasher::new();
        runtime
            .engine()
            .precompile_compatibility_hash()
            .hash(&mut runtime_hash);
        let mut default_hash = DefaultHasher::new();
        wasmtime::Engine::default()
            .precompile_compatibility_hash()
            .hash(&mut default_hash);
        assert_ne!(runtime_hash.finish(), default_hash.finish());
    }

    #[test]
    fn runtime_error_display_and_invalid_module_kind() {
        let err = RuntimeError::InvalidModule(InvalidModuleKind::MissingImports {
            missing: vec!["alloc".to_string()],
        });
        assert!(
            err.to_string()
                .contains("module is missing SDK imports: alloc")
        );
    }
}
