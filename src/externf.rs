// Low-level functions exposed by the WASM host.
//
// They are intentionally kept private to the crate. `src/lib.rs` provides the
// safe wrappers used by contract authors.
//
// # Safety
//
// All functions below cross the WASM boundary and work with raw pointers.
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
