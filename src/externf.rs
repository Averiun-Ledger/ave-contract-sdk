// Low-level functions exposed by the WASM host.
//
// They are intentionally kept private to the crate. `src/lib.rs` provides the
// safe wrappers used by contract authors.
//
// # Safety
//
// All functions below cross the WASM boundary and work with raw pointers.
unsafe extern "C" {
    // Reads one byte from host memory.
    pub(crate) fn read_byte(pointer: i32) -> u8;

    // Returns the byte length associated with a host pointer.
    pub(crate) fn pointer_len(pointer: i32) -> i32;

    // Allocates `len` bytes in WASM memory for returning data to the host.
    pub(crate) fn alloc(len: u32) -> i32;

    // Writes one byte into previously allocated WASM memory.
    pub(crate) fn write_byte(ptr: u32, offset: u32, data: u8);
}
