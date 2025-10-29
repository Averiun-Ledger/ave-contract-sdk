

use thiserror::Error;

/// Internal error types for the Kore Contract SDK.
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
}
