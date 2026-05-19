use thiserror::Error;

/// Internal error types for the Ave Contract SDK.
///
/// These errors are converted into host-facing failure results by the SDK.
#[derive(Error, Debug)]
pub enum Error {
    /// Failure while encoding data to Borsh.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Failure while decoding Borsh data received from the host.
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Host input exceeded the configured memory limit.
    #[error("Memory limit exceeded: requested {requested} bytes, maximum allowed is {max} bytes")]
    MemoryLimitExceeded { requested: usize, max: usize },

    /// Integer conversion or pointer arithmetic overflow.
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
        assert_eq!(
            error.to_string(),
            "Integer overflow: value too large for u32"
        );
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Error>();
    }

    #[test]
    fn test_error_debug_format() {
        let error = Error::Serialization("test".to_string());
        let debug_str = format!("{error:?}");
        assert!(debug_str.contains("Serialization"));
        assert!(debug_str.contains("test"));
    }
}
