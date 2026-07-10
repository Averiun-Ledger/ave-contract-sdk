//! Host-side memory manager and linker wiring for the SDK's `env` imports.

use std::collections::HashMap;
use wasmtime::{Caller, Error as WasmError, Linker, StoreLimits, StoreLimitsBuilder};

use crate::runtime::{ContractError, WasmLimits};

/// Backing store for guest allocations and the resource limits enforced on a store.
#[derive(Debug)]
pub struct MemoryManager {
    memory: Vec<u8>,
    map: HashMap<usize, usize>,
    /// Wasmtime store limits derived from the configured `WasmLimits`.
    pub store_limits: StoreLimits,
    max_single_alloc: usize,
    max_total_memory: usize,
}

impl MemoryManager {
    /// Builds a memory manager whose store limits and allocation caps come from `limits`.
    pub fn from_limits(limits: &WasmLimits) -> Self {
        Self {
            memory: Vec::new(),
            map: HashMap::new(),
            store_limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_size)
                .table_elements(limits.max_table_elements)
                .instances(1)
                .tables(1)
                .memories(1)
                .trap_on_grow_failure(true)
                .build(),
            max_single_alloc: limits.max_single_alloc,
            max_total_memory: limits.max_total_memory,
        }
    }

    /// Allocates `len` zeroed bytes and returns the offset of the new region.
    ///
    /// Fails if `len` exceeds the single-allocation cap, if the running total would
    /// exceed the total-memory cap, or if the offset addition overflows.
    pub fn alloc(&mut self, len: usize) -> Result<usize, ContractError> {
        if len > self.max_single_alloc {
            return Err(ContractError::AllocationTooLarge {
                size: len,
                max: self.max_single_alloc,
            });
        }
        let current_len = self.memory.len();
        let new_len = current_len
            .checked_add(len)
            .ok_or(ContractError::AllocationOverflow)?;
        if new_len > self.max_total_memory {
            return Err(ContractError::TotalMemoryExceeded {
                total: new_len,
                max: self.max_total_memory,
            });
        }
        self.memory.resize(new_len, 0);
        self.map.insert(current_len, len);
        Ok(current_len)
    }

    /// Returns the bytes previously allocated at `ptr`.
    ///
    /// The slice length is the size recorded when `ptr` was allocated; fails if `ptr`
    /// is unknown or out of range.
    pub fn read_data(&self, ptr: usize) -> Result<&[u8], ContractError> {
        let len = self
            .map
            .get(&ptr)
            .copied()
            .ok_or(ContractError::InvalidPointer { pointer: ptr })?;
        if ptr + len > self.memory.len() {
            return Err(ContractError::InvalidPointer { pointer: ptr });
        }
        Ok(&self.memory[ptr..ptr + len])
    }

    /// Returns `len` bytes starting at `ptr` if the range is in bounds.
    pub fn read_bytes(&self, ptr: usize, len: usize) -> Result<&[u8], ContractError> {
        let end = ptr
            .checked_add(len)
            .ok_or(ContractError::InvalidPointer { pointer: ptr })?;
        if end > self.memory.len() {
            return Err(ContractError::InvalidPointer { pointer: ptr });
        }
        Ok(&self.memory[ptr..end])
    }

    /// Copies `data` into the allocation at `ptr`.
    ///
    /// Fails if `ptr` is unknown or if `data` is larger than the allocation.
    pub fn write_bytes(&mut self, ptr: usize, data: &[u8]) -> Result<(), ContractError> {
        let len = self
            .map
            .get(&ptr)
            .copied()
            .ok_or(ContractError::InvalidPointer { pointer: ptr })?;
        if data.len() > len {
            return Err(ContractError::WriteOutOfBounds {
                offset: data.len(),
                size: len,
            });
        }
        self.memory[ptr..ptr + data.len()].copy_from_slice(data);
        Ok(())
    }

    /// Allocates space for `bytes`, copies them in, and returns the new pointer.
    pub fn add_data_raw(&mut self, bytes: &[u8]) -> Result<usize, ContractError> {
        let ptr = self.alloc(bytes.len())?;
        self.memory[ptr..ptr + bytes.len()].copy_from_slice(bytes);
        Ok(ptr)
    }

    /// Returns the length of the allocation at `ptr`, or `-1` if `ptr` is unknown.
    pub fn get_pointer_len(&self, ptr: usize) -> isize {
        self.map.get(&ptr).map_or(-1, |len| *len as isize)
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::from_limits(&WasmLimits::default())
    }
}

/// Builds a `Linker` exposing the SDK `env` imports (`pointer_len`, `alloc`, `read_bytes`, `write_bytes`) backed by a `MemoryManager`.
pub fn generate_linker(engine: &wasmtime::Engine) -> Result<Linker<MemoryManager>, ContractError> {
    let mut linker: Linker<MemoryManager> = Linker::new(engine);

    linker
        .func_wrap(
            "env",
            "pointer_len",
            |caller: Caller<'_, MemoryManager>, pointer: i32| {
                caller.data().get_pointer_len(pointer as usize) as u32
            },
        )
        .map_err(|e| ContractError::LinkerError {
            function: "pointer_len",
            details: e.to_string(),
        })?;

    linker
        .func_wrap(
            "env",
            "alloc",
            |mut caller: Caller<'_, MemoryManager>, len: u32| -> Result<u32, WasmError> {
                caller
                    .data_mut()
                    .alloc(len as usize)
                    .map(|ptr| ptr as u32)
                    .map_err(WasmError::from)
            },
        )
        .map_err(|e| ContractError::LinkerError {
            function: "alloc",
            details: e.to_string(),
        })?;

    linker
        .func_wrap(
            "env",
            "read_bytes",
            |mut caller: Caller<'_, MemoryManager>,
             src_ptr: i32,
             dst_ptr: i32,
             len: i32|
             -> Result<(), WasmError> {
                let bytes = caller
                    .data()
                    .read_bytes(src_ptr as usize, len as usize)
                    .map_err(WasmError::from)?
                    .to_vec();
                let memory = caller
                    .get_export("memory")
                    .and_then(|e| e.into_memory())
                    .ok_or_else(|| ContractError::LinkerError {
                        function: "read_bytes",
                        details: "memory export not found".to_string(),
                    })?;
                memory
                    .write(&mut caller, dst_ptr as usize, &bytes)
                    .map_err(WasmError::from)?;
                Ok(())
            },
        )
        .map_err(|e| ContractError::LinkerError {
            function: "read_bytes",
            details: e.to_string(),
        })?;

    linker
        .func_wrap(
            "env",
            "write_bytes",
            |mut caller: Caller<'_, MemoryManager>,
             dst_ptr: i32,
             src_ptr: i32,
             len: i32|
             -> Result<(), WasmError> {
                let memory = caller
                    .get_export("memory")
                    .and_then(|e| e.into_memory())
                    .ok_or_else(|| ContractError::LinkerError {
                        function: "write_bytes",
                        details: "memory export not found".to_string(),
                    })?;
                let mut buf = vec![0u8; len as usize];
                memory
                    .read(&caller, src_ptr as usize, &mut buf)
                    .map_err(WasmError::from)?;
                caller
                    .data_mut()
                    .write_bytes(dst_ptr as usize, &buf)
                    .map_err(WasmError::from)?;
                Ok(())
            },
        )
        .map_err(|e| ContractError::LinkerError {
            function: "write_bytes",
            details: e.to_string(),
        })?;

    Ok(linker)
}
