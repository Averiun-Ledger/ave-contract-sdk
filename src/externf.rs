// Low-level functions exposed by the WASM host.
//
// They are intentionally kept private to the crate. `src/lib.rs` provides the
// safe wrappers used by contract authors.
//
// # Safety
//
// All functions below cross the WASM boundary and work with raw pointers.

#[cfg(not(test))]
unsafe extern "C" {
    // Reads a block of bytes from host memory into WASM linear memory.
    // `src_ptr` is the host pointer, `dst_ptr` is the destination in WASM memory.
    pub(crate) fn read_bytes(src_ptr: i32, dst_ptr: i32, len: i32);

    // Returns the byte length associated with a host pointer.
    pub(crate) fn pointer_len(pointer: i32) -> i32;

    // Allocates `len` bytes in WASM memory for returning data to the host.
    pub(crate) fn alloc(len: u32) -> i32;

    // Writes a block of bytes from WASM linear memory into host memory.
    // `dst_ptr` is the host pointer, `src_ptr` is the source in WASM memory.
    pub(crate) fn write_bytes(dst_ptr: i32, src_ptr: i32, len: i32);
}

#[cfg(test)]
pub(crate) use test_impl::{
    alloc, get_data, pointer_len, read_bytes_into_vec, reset, set_force_alloc_zero,
    set_force_pointer_len, store_data, write_bytes_from_slice,
};

#[cfg(test)]
mod test_impl {
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        static HOST_MEMORY: RefCell<HashMap<i32, Vec<u8>>> = RefCell::new(HashMap::new());
        static NEXT_ID: RefCell<i32> = const { RefCell::new(1000) };
        static FORCE_ALLOC_ZERO: RefCell<bool> = const { RefCell::new(false) };
        static FORCE_POINTER_LEN: RefCell<Option<i32>> = const { RefCell::new(None) };
    }

    /// Clears the mock host memory and resets pointer allocation.
    pub fn reset() {
        HOST_MEMORY.with(|m| m.borrow_mut().clear());
        NEXT_ID.with(|n| *n.borrow_mut() = 1000);
        FORCE_ALLOC_ZERO.with(|f| *f.borrow_mut() = false);
        FORCE_POINTER_LEN.with(|f| *f.borrow_mut() = None);
    }

    pub fn set_force_alloc_zero(value: bool) {
        FORCE_ALLOC_ZERO.with(|f| *f.borrow_mut() = value);
    }

    pub fn set_force_pointer_len(value: i32) {
        FORCE_POINTER_LEN.with(|f| *f.borrow_mut() = Some(value));
    }

    /// Stores data in mock host memory and returns the simulated host pointer.
    pub fn store_data(data: Vec<u8>) -> i32 {
        let ptr = NEXT_ID.with(|n| {
            let mut id = n.borrow_mut();
            let current = *id;
            *id += 1;
            current
        });
        HOST_MEMORY.with(|m| {
            m.borrow_mut().insert(ptr, data);
        });
        ptr
    }

    /// Retrieves a copy of data stored at the given mock host pointer.
    pub fn get_data(ptr: i32) -> Option<Vec<u8>> {
        HOST_MEMORY.with(|m| m.borrow().get(&ptr).cloned())
    }

    // Mock implementation of `read_bytes`.
    // Note: the low-level `read_bytes` / `write_bytes` mocks that work with raw
    // pointers are omitted here because in the native test target pointer
    // truncation (`*mut u8` -> `i32`) causes SIGSEGV.  Tests use the safe
    // alternatives `read_bytes_into_vec` and `write_bytes_from_slice` instead.

    /// Mock implementation of `pointer_len`.
    pub fn pointer_len(pointer: i32) -> i32 {
        if let Some(forced) = FORCE_POINTER_LEN.with(|f| *f.borrow()) {
            return forced;
        }
        HOST_MEMORY.with(|m| m.borrow().get(&pointer).map_or(0, |d| d.len() as i32))
    }

    /// Mock implementation of `alloc`.
    pub fn alloc(len: u32) -> i32 {
        if FORCE_ALLOC_ZERO.with(|f| *f.borrow()) {
            return 0;
        }
        let ptr = NEXT_ID.with(|n| {
            let mut id = n.borrow_mut();
            let current = *id;
            *id += 1;
            current
        });
        HOST_MEMORY.with(|m| {
            m.borrow_mut().insert(ptr, vec![0; len as usize]);
        });
        ptr
    }

    /// Mock implementation of `write_bytes`.
    /// Safe test helper: copies the full content stored at `src_ptr` into `dst`.
    pub fn read_bytes_into_vec(src_ptr: i32, dst: &mut Vec<u8>) {
        let data = HOST_MEMORY
            .with(|m| m.borrow().get(&src_ptr).cloned())
            .unwrap_or_default();
        dst.extend_from_slice(&data);
    }

    /// Safe test helper: stores `src` into mock host memory at `dst_ptr`.
    pub fn write_bytes_from_slice(dst_ptr: i32, src: &[u8]) {
        HOST_MEMORY.with(|m| {
            m.borrow_mut().insert(dst_ptr, src.to_vec());
        });
    }
}
