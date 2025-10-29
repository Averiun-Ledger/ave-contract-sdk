

mod error;
mod externf;
mod value_wrapper;
use borsh::{BorshDeserialize, BorshSerialize};
use error::Error;
use serde::{Deserialize, Serialize};

pub use self::value_wrapper::ValueWrapper;

/// Contract execution context.
///
/// This structure contains all the information about the event that triggered
/// the contract execution and the authorization context.
#[derive(Serialize, Deserialize, Debug)]
pub struct Context<Event> {
    /// Event that triggered the contract execution.
    /// Contains the data describing the action to be performed on the subject's state.
    pub event: Event,
    /// Indicates whether the sender of the event is the owner of the subject.
    /// This can be used to implement owner-only operations in the contract logic.
    pub is_owner: bool,
}

/// Contract execution result.
///
/// This structure is returned after processing an event and contains the modified state,
/// a success flag, and optional error information.
#[derive(Serialize, Deserialize, Debug)]
pub struct ContractResult<State> {
    /// Final state of the contract after processing the event.
    /// This state will be persisted if the execution is successful.
    pub state: State,
    /// Indicates whether the contract execution was successful.
    /// If true, the state changes will be applied; if false, they will be rejected.
    pub success: bool,
    /// Error message describing why the execution failed.
    /// Should be empty when success is true.
    pub error: String
}

/// Contract initialization validation result.
///
/// This structure is returned after validating the initial state of a contract
/// before subject creation.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ContractInitCheck {
    /// Indicates whether the initial state validation was successful.
    /// If true, the subject can be created with this initial state;
    /// if false, creation will be rejected.
    pub success: bool,
    /// Error message describing why the initial state validation failed.
    /// Should be empty when success is true.
    pub error: String,
}

/// Internal contract execution result used for Borsh serialization.
///
/// This structure is used internally to serialize the contract result into
/// a binary format (Borsh) for efficient data transfer across the WASM boundary.
#[derive(BorshSerialize)]
struct ContractResultBorsh {
    /// Final state of the contract wrapped for Borsh serialization.
    pub final_state: ValueWrapper,
    /// Indicates whether the contract execution was successful.
    pub success: bool,
    /// Error message if the execution failed.
    pub error: String
}

/// Implementation for creating error results.
impl ContractResultBorsh {
    /// Creates a new error result with a null state.
    ///
    /// # Arguments
    ///
    /// * `error` - The error message describing the failure.
    ///
    /// # Returns
    ///
    /// A `ContractResultBorsh` instance representing a failed execution.
    pub fn error(error: &str) -> Self {
        Self {
            final_state: ValueWrapper(serde_json::Value::Null),
            success: false,
            error: error.to_owned()
        }
    }
}

/// Internal contract initialization check result used for Borsh serialization.
///
/// This structure is used internally to serialize the initialization check result
/// into a binary format (Borsh) for efficient data transfer across the WASM boundary.
#[derive(BorshSerialize)]
struct ContractInitCheckBorsh {
    /// Indicates whether the initialization check was successful.
    pub success: bool,
    /// Error message if the initialization check failed.
    pub error: String
}

/// Implementation for creating initialization check results.
impl ContractInitCheckBorsh {
    /// Creates a new error result indicating validation failure.
    ///
    /// # Arguments
    ///
    /// * `error` - The error message describing why the initial state is invalid.
    ///
    /// # Returns
    ///
    /// A `ContractInitCheckBorsh` instance representing a failed validation.
    pub fn error(error: &str) -> Self {
        Self {
            success: false,
            error: error.to_owned()
        }
    }

    /// Creates a new success result indicating validation passed.
    ///
    /// # Returns
    ///
    /// A `ContractInitCheckBorsh` instance representing a successful validation.
    pub fn ok() -> Self {
        Self {
            success: true,
            error: String::default()
        }
    }
}

/// Implementation for creating new contract results.
impl<State> ContractResult<State> {
    /// Creates a new contract result with the given state.
    ///
    /// The result is initialized with success set to false, which should be
    /// changed to true by the contract logic if the event is processed successfully.
    ///
    /// # Arguments
    ///
    /// * `state` - The current state of the contract before processing the event.
    ///
    /// # Returns
    ///
    /// A new `ContractResult` instance with the provided state.
    pub fn new(state: State) -> Self {
        Self {
            state,
            success: false,
            error: String::default()
        }
    }
}

/// Validates the initial state of a contract before subject creation.
///
/// This function is called by the WASM runtime to check whether a proposed
/// initial state is valid according to the contract's business rules. It deserializes
/// the state from the provided memory pointer, executes the validation callback,
/// and returns a serialized result indicating success or failure.
///
/// # Type Parameters
///
/// * `State` - The type representing the contract's state. Must implement
///   `Serialize`, `Deserialize`, and `Clone`.
/// * `F` - The callback function type that contains the validation logic.
///
/// # Arguments
///
/// * `state_ptr` - Memory pointer to the proposed initial state data (provided by the WASM host).
/// * `callback` - User-defined function that validates the state and sets the result.
///
/// # Returns
///
/// * `u32` - Memory pointer to the serialized `ContractInitCheckBorsh` result,
///   which the host runtime can read to determine if initialization should proceed.
///
/// # Callback Signature
///
/// The callback function should have the signature:
/// ```ignore
/// fn callback(state: &State, result: &mut ContractInitCheck)
/// ```
///
/// The callback should set `result.success = true` if the state is valid,
/// or `result.success = false` with an error message if it's invalid.
///
/// # Example
///
/// ```ignore
/// #[unsafe(no_mangle)]
/// pub unsafe fn init_check_function(state_ptr: i32) -> u32 {
///     sdk::check_init_data(state_ptr, |state: &MyState, result| {
///         if state.value > 100 {
///             result.success = false;
///             result.error = "Value too high".to_string();
///         } else {
///             result.success = true;
///         }
///     })
/// }
/// ```
pub fn check_init_data<State, F>(
    state_ptr: i32,
    callback: F,
) -> u32
where
    State: for<'a> Deserialize<'a> + Serialize + Clone,
    F: Fn(&State, &mut ContractInitCheck),
{
    {
        let error: String;
        'process: {
            let Ok(state_value) = deserialize(get_from_context(state_ptr)) else {
                error = "Can not deserialize State".to_owned();
                break 'process;
            };
            let Ok(state) = serde_json::from_value::<State>(state_value.0) else {
                error = "Can not convert State from value".to_owned();
                break 'process;
            };
            let mut contract_result = ContractInitCheck::default();
            callback(&state, &mut contract_result);

            if !contract_result.success {
                error = format!("Error running init contract data: {}", contract_result.error);
                break 'process;
            }

            let Ok(result_ptr) = store(&ContractInitCheckBorsh::ok()) else {
                error = "Can not return init contract result".to_owned();
                break 'process;
            };
            return result_ptr;
        }
        store(&ContractInitCheckBorsh::error(&error)).expect("Contract store process failed")
    }
}

/// Executes a contract by processing an event and updating the subject's state.
///
/// This is the main entry point for contract execution. It is called by the WASM runtime
/// whenever an event needs to be processed. The function:
/// 1. Deserializes the current state (or falls back to init state if needed)
/// 2. Deserializes the event that triggered the execution
/// 3. Creates a context with the event and ownership information
/// 4. Executes the user-defined contract logic callback
/// 5. Serializes and returns the result
///
/// # Type Parameters
///
/// * `State` - The type representing the contract's state. Must implement
///   `Serialize`, `Deserialize`, and `Clone`.
/// * `Event` - The type representing events that can modify the state. Must implement
///   `Serialize` and `Deserialize`.
/// * `F` - The callback function type that contains the contract's business logic.
///
/// # Arguments
///
/// * `state_ptr` - Memory pointer to the current state of the contract (provided by the WASM host).
/// * `init_state_ptr` - Memory pointer to the initial state, used as a fallback if the current
///   state cannot be deserialized (e.g., for newly created subjects).
/// * `event_ptr` - Memory pointer to the event data that triggered this execution.
/// * `is_owner` - Integer flag (0 or 1) indicating whether the event issuer is the subject owner.
/// * `callback` - User-defined function that implements the contract's logic for processing events.
///
/// # Returns
///
/// * `u32` - Memory pointer to the serialized `ContractResultBorsh`, which contains the
///   final state, success flag, and any error message. The host runtime reads this to
///   determine whether to apply the state changes.
///
/// # State Fallback Mechanism
///
/// If the current state at `state_ptr` cannot be deserialized (which happens for new subjects),
/// the function automatically falls back to using `init_state_ptr`. This allows contracts to
/// handle initialization gracefully without special-casing the first event.
///
/// # Callback Signature
///
/// The callback function should have the signature:
/// ```ignore
/// fn callback(context: &Context<Event>, result: &mut ContractResult<State>)
/// ```
///
/// The callback should:
/// - Modify `result.state` as needed based on `context.event`
/// - Set `result.success = true` if the event should be applied
/// - Set `result.success = false` and provide `result.error` if the event should be rejected
///
/// # Example
///
/// ```ignore
/// #[unsafe(no_mangle)]
/// pub unsafe fn main_function(
///     state_ptr: i32,
///     init_state_ptr: i32,
///     event_ptr: i32,
///     is_owner: i32,
/// ) -> u32 {
///     sdk::execute_contract(state_ptr, init_state_ptr, event_ptr, is_owner, |context, result| {
///         match &context.event {
///             Event::Update { value } => {
///                 result.state.value = *value;
///                 result.success = true;
///             }
///             Event::Delete => {
///                 if context.is_owner {
///                     result.state.deleted = true;
///                     result.success = true;
///                 } else {
///                     result.success = false;
///                     result.error = "Only owner can delete".to_string();
///                 }
///             }
///         }
///     })
/// }
/// ```
pub fn execute_contract<F, State, Event>(
    state_ptr: i32,
    init_state_ptr: i32,
    event_ptr: i32,
    is_owner: i32,
    callback: F,
) -> u32
where
    State: for<'a> Deserialize<'a> + Serialize + Clone,
    Event: for<'a> Deserialize<'a> + Serialize,
    F: Fn(&Context<Event>, &mut ContractResult<State>),
{
    {
        let error: String;
        'process: {
            let Ok(state_value) = deserialize(get_from_context(state_ptr)) else {
                error = "Can not deserialize State".to_owned();
                break 'process;
            };
            let state = match serde_json::from_value::<State>(state_value.0) {
                Ok(state) => state,
                Err(_) => {
                    let Ok(init_state) = deserialize(get_from_context(init_state_ptr)) else {
                        error = "Can not deserialize Init State".to_owned();
                        break 'process;
                    };

                    let Ok(init_state) = serde_json::from_value::<State>(init_state.0) else {
                        error = "Can not convert State from value".to_owned();
                        break 'process;
                    };

                    init_state
                }
            };
            let Ok(event_value) = deserialize(get_from_context(event_ptr)) else {
                error = "Can not deserialize Event".to_owned();
                break 'process;
            };
            let Ok(event) = serde_json::from_value::<Event>(event_value.0) else {
                error = "Can not convert Event from value".to_owned();
                break 'process;
            };
            let is_owner = if is_owner == 1 { true } else { false };
            let context = Context {
                event,
                is_owner
            };
            let mut contract_result = ContractResult::new(state);
            callback(&context, &mut contract_result);
            let Ok(state_value) = serde_json::to_value(&contract_result.state) else {
                error = "Can not convert contract final state into Value".to_owned();
                break 'process;
            };
            let result = ContractResultBorsh {
                final_state: ValueWrapper(state_value),
                success: contract_result.success,
                error: format!("Error running contract event: {}", contract_result.error)
            };
            // After the state has been modified, we must save the new state.
            // It would be interesting to avoid saving state if the event is non-modifying.
            let Ok(result_ptr) = store(&result) else {
                error = "Can not return contract result".to_owned();
                break 'process;
            };
            return result_ptr;
        };
        store(&ContractResultBorsh::error(&error)).expect("Contract store process failed")
    }
}

/// Deserializes data from bytes using Borsh format.
///
/// Converts a byte vector into a `ValueWrapper` by deserializing from Borsh binary format.
///
/// # Arguments
///
/// * `bytes` - The byte vector containing Borsh-serialized data.
///
/// # Returns
///
/// * `Result<ValueWrapper, Error>` - The deserialized value wrapper or an error.
fn deserialize(bytes: Vec<u8>) -> Result<ValueWrapper, Error> {
    BorshDeserialize::try_from_slice(&bytes).map_err(|e| Error::Deserialization(e.to_string()))
}

/// Serializes data into bytes using Borsh format.
///
/// Converts any data implementing `BorshSerialize` into a byte vector for
/// efficient transfer across the WASM boundary.
///
/// # Type Parameters
///
/// * `S` - The type to serialize, must implement `BorshSerialize`.
///
/// # Arguments
///
/// * `data` - The data to serialize.
///
/// # Returns
///
/// * `Result<Vec<u8>, Error>` - The serialized bytes or an error.
fn serialize<S: BorshSerialize>(data: S) -> Result<Vec<u8>, Error> {
    borsh::to_vec(&data).map_err(|e| Error::Serialization(e.to_string()))
}

/// Reads data from WASM host memory at the given pointer.
///
/// This function uses the external `read_byte` and `pointer_len` functions
/// to read a complete byte sequence from the host's memory space into the
/// WASM module's memory.
///
/// # Arguments
///
/// * `pointer` - Memory pointer provided by the host indicating where to read from.
///
/// # Returns
///
/// * `Vec<u8>` - The bytes read from host memory.
fn get_from_context(pointer: i32) -> Vec<u8> {
    let data = unsafe {
        let len = externf::pointer_len(pointer);
        let mut data = vec![];
        for i in 0..len {
            data.push(externf::read_byte(pointer + i));
        }
        data
    };
    data
}

/// Stores data in WASM memory to be read by the host.
///
/// This function serializes the data to Borsh format, allocates memory in the
/// WASM module's linear memory space, writes the serialized bytes to that memory,
/// and returns a pointer that the host can use to read the result.
///
/// # Type Parameters
///
/// * `S` - The type to store, must implement `BorshSerialize`.
///
/// # Arguments
///
/// * `data` - The data to serialize and store in memory.
///
/// # Returns
///
/// * `Result<u32, Error>` - A memory pointer to the stored data, or an error.
fn store<S>(data: &S) -> Result<u32, Error>
where
    S: BorshSerialize
{
    let bytes = serialize(&data).map_err(|e| Error::Serialization(e.to_string()))?;
    unsafe {
        let ptr = externf::alloc(bytes.len() as u32) as u32;
        for (index, byte) in bytes.into_iter().enumerate() {
            externf::write_byte(ptr, index as u32, byte);
        }
        Ok(ptr)
    }
}

#[cfg(test)]
mod tests {
}
