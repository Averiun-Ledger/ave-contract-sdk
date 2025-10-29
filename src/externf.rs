
/// External functions provided by the WASM host for memory operations and debugging.
///
/// These functions form the interface between the WASM contract module and the
/// Kore Ledger runtime. They enable bidirectional data transfer across the WASM
/// boundary using raw memory pointers.
///
/// # Safety
///
/// All these functions are unsafe because they involve raw memory operations and
/// cross the WASM-host boundary. The SDK provides safe wrappers around these
/// functions in the main library module.
unsafe extern "C" {
    /// Reads a single byte from host memory at the specified pointer location.
    ///
    /// This function is used to read data that the host has provided to the WASM module,
    /// such as state data or event data. It's typically called in a loop to read
    /// complete data structures byte by byte.
    ///
    /// # Arguments
    ///
    /// * `pointer` - Memory pointer indicating which byte to read from host memory.
    ///
    /// # Returns
    ///
    /// * `u8` - The byte value at the specified memory location.
    ///
    /// # Safety
    ///
    /// The caller must ensure the pointer is valid and points to readable memory
    /// in the host's address space.
    pub(crate) fn read_byte(pointer: i32) -> u8;

    /// Gets the length in bytes of a data structure in host memory.
    ///
    /// Given a pointer to the start of a data structure in host memory, this function
    /// returns how many bytes can be read from that location. This is essential for
    /// knowing how much data to read when deserializing state or events.
    ///
    /// # Arguments
    ///
    /// * `pointer` - Memory pointer to the start of the data structure.
    ///
    /// # Returns
    ///
    /// * `i32` - The length in bytes of the data structure.
    ///
    /// # Safety
    ///
    /// The caller must ensure the pointer is valid and points to a properly
    /// initialized data structure in the host's address space.
    pub(crate) fn pointer_len(pointer: i32) -> i32;

    /// Allocates memory in the WASM module's linear memory for writing results.
    ///
    /// This function requests the host to allocate a block of memory that the WASM
    /// module can write to. The allocated memory is used to return results (like
    /// contract execution results) back to the host.
    ///
    /// # Arguments
    ///
    /// * `len` - The number of bytes to allocate.
    ///
    /// # Returns
    ///
    /// * `i32` - A pointer to the allocated memory block.
    ///
    /// # Safety
    ///
    /// The returned pointer must be used carefully. The caller is responsible for
    /// writing exactly `len` bytes and not exceeding the allocated space.
    pub(crate) fn alloc(len: u32) -> i32;

    /// Writes a single byte to a specific position in allocated memory.
    ///
    /// This function writes data to memory that was previously allocated via `alloc()`.
    /// It's typically called in a loop to write complete serialized data structures
    /// byte by byte so the host can read the contract's results.
    ///
    /// # Arguments
    ///
    /// * `ptr` - Pointer to the allocated memory block (from `alloc()`).
    /// * `offset` - Byte offset within the allocated block where to write.
    /// * `data` - The byte value to write.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// - `ptr` points to validly allocated memory
    /// - `offset` is within the bounds of the allocated block
    /// - No concurrent writes occur to the same memory location
    pub(crate) fn write_byte(ptr: u32, offset: u32, data: u8);

    /// Outputs a debug message to the host's console or logging system.
    ///
    /// This function is used for debugging purposes during contract development.
    /// It allows contracts to print messages that can be viewed in the host
    /// runtime's logs.
    ///
    /// # Arguments
    ///
    /// * `ptr` - Pointer to a UTF-8 encoded string in memory.
    ///
    /// # Safety
    ///
    /// The pointer must point to a valid, properly formatted UTF-8 string.
    ///
    /// # Note
    ///
    /// This function should only be used for debugging and development. It may not
    /// be available or may be disabled in production environments.
    #[allow(dead_code)]
    pub(crate) fn cout(ptr: u32);
}
