

use thiserror::Error;

/// Internal error types for the Ave Contract SDK.
///
/// These errors are used internally by the SDK to handle failures during
/// serialization and deserialization operations. They are not exposed to
/// contract authors as the SDK handles them and converts them into appropriate
/// result messages.
#[derive(Error, Debug)]
pub(crate) enum Error {
    /// Error that occurs when serializing data to Borsh binary format.
    ///
    /// This typically happens when trying to convert contract results or
    /// initialization check results into binary format for transmission
    /// to the WASM host. The inner String contains the detailed error message
    /// from the Borsh serialization library.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Error that occurs when deserializing data from Borsh binary format.
    ///
    /// This typically happens when trying to convert binary data from the
    /// WASM host (such as state or event data) into Rust types. The inner
    /// String contains the detailed error message from the Borsh
    /// deserialization library.
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Error that occurs when attempting to allocate memory exceeding the maximum allowed size.
    ///
    /// This error prevents denial-of-service attacks where malicious contracts or hosts
    /// attempt to exhaust memory by requesting extremely large allocations.
    #[error("Memory limit exceeded: requested {requested} bytes, maximum allowed is {max} bytes")]
    MemoryLimitExceeded { requested: usize, max: usize },

    /// Error that occurs when an integer conversion would overflow.
    ///
    /// This error prevents memory corruption that could occur from implicit integer
    /// truncation in type casts.
    #[error("Integer overflow: {0}")]
    IntegerOverflow(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_error_display() {
        let error = Error::Serialization("test error message".to_string());
        assert_eq!(error.to_string(), "Serialization error: test error message");
    }

    #[test]
    fn test_deserialization_error_display() {
        let error = Error::Deserialization("invalid format".to_string());
        assert_eq!(error.to_string(), "Deserialization error: invalid format");
    }

    #[test]
    fn test_memory_limit_exceeded_error_display() {
        let error = Error::MemoryLimitExceeded {
            requested: 20_000_000,
            max: 10_000_000,
        };
        assert_eq!(
            error.to_string(),
            "Memory limit exceeded: requested 20000000 bytes, maximum allowed is 10000000 bytes"
        );
    }

    #[test]
    fn test_integer_overflow_error_display() {
        let error = Error::IntegerOverflow("value too large for u32".to_string());
        assert_eq!(error.to_string(), "Integer overflow: value too large for u32");
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Error>();
    }

    #[test]
    fn test_error_debug_format() {
        let error = Error::Serialization("test".to_string());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("Serialization"));
        assert!(debug_str.contains("test"));
    }
}
