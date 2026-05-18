mod error;
mod externf;
use ave_common::{ContractData, ContractInitCheckData, ContractResultData};
use borsh::{BorshDeserialize, BorshSerialize};
use error::Error;
use serde::{Deserialize, Serialize};

/// Maximum size in bytes for data read from host memory.
/// Prevents excessive allocations from malformed or malicious host input.
const MAX_DATA_SIZE: i32 = 10_000_000; // 10MB

/// Contract execution context.
#[derive(Serialize, Deserialize, Debug)]
pub struct Context<Event> {
    /// Event being applied to the current state.
    pub event: Event,
    /// Whether the event sender is the owner.
    pub is_owner: bool,
}

/// Contract execution result.
#[derive(Serialize, Deserialize, Debug)]
pub struct ContractResult<State> {
    /// Final state after executing the event.
    pub state: State,
    /// Whether the runtime should apply the state change.
    pub success: bool,
    /// Rejection reason when `success` is `false`.
    pub error: String,
}

/// Contract initialization validation result.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ContractInitCheck {
    /// Whether the initial state is accepted.
    pub success: bool,
    /// Rejection reason when `success` is `false`.
    pub error: String,
}





impl<State> ContractResult<State> {
    /// Creates a new contract result with the given state.
    ///
    /// New results start as failed and must be marked successful by contract logic.
    pub fn new(state: State) -> Self {
        Self {
            state,
            success: false,
            error: String::default(),
        }
    }

    /// Marks the result as successful.
    pub fn accept(&mut self) {
        self.success = true;
    }

    /// Marks the result as failed with the given message.
    pub fn reject(&mut self, msg: impl Into<String>) {
        self.success = false;
        self.error = msg.into();
    }
}

impl ContractInitCheck {
    /// Marks the check as successful.
    pub fn accept(&mut self) {
        self.success = true;
    }

    /// Marks the check as failed with the given message.
    pub fn reject(&mut self, msg: impl Into<String>) {
        self.success = false;
        self.error = msg.into();
    }
}

/// Validates the initial state of a contract before subject creation.
///
/// The runtime passes the proposed state through `state_ptr`. The callback decides
/// whether that state is valid and writes the outcome into `ContractInitCheck`.
///
/// # Arguments
///
/// * `state_ptr` - Pointer to the proposed initial state in host memory.
/// * `callback` - Validation function with signature `fn(&State, &mut ContractInitCheck)`.
///
/// # Returns
///
/// Pointer to a serialized `ContractInitCheckData`.
///
/// # Example
///
/// ```ignore
/// #[unsafe(no_mangle)]
/// pub unsafe fn init_check_function(state_ptr: i32) -> u32 {
///     sdk::check_init_data(state_ptr, |state: &MyState, result| {
///         if state.value > 100 {
///             result.reject("Value too high");
///         } else {
///             result.accept();
///         }
///     })
/// }
/// ```
pub fn check_init_data<State, F>(state_ptr: i32, callback: F) -> u32
where
    State: for<'a> Deserialize<'a> + Serialize + Clone,
    F: Fn(&State, &mut ContractInitCheck),
{
    let error: String;
    'process: {
        let state = match read_and_parse::<State>(state_ptr, "State") {
            Ok(s) => s,
            Err(e) => {
                error = e;
                break 'process;
            }
        };
        let mut contract_result = ContractInitCheck::default();
        callback(&state, &mut contract_result);

        if !contract_result.success {
            error = format!(
                "Error running init contract data: {}",
                contract_result.error
            );
            break 'process;
        }

        let Ok(result_ptr) = store(&ContractInitCheckData::ok()) else {
            error = "Cannot return init contract result".to_owned();
            break 'process;
        };
        return result_ptr;
    }
    // Attempt to return error via store, but if that fails too, return 0 pointer.
    // The host should handle 0 pointer as a fatal error.
    store(&ContractInitCheckData::error(&error)).unwrap_or(0)
}

/// Executes a contract by processing an event and updating the subject's state.
///
/// This is the main entry point used by the WASM runtime. It reads the current
/// state and the incoming event, builds a `Context<Event>`, runs the callback,
/// and returns the serialized result.
///
/// # Arguments
///
/// * `state_ptr` - Pointer to the current state in host memory.
/// * `init_state_ptr` - Pointer to the initial state used as a fallback when the current state cannot be deserialized.
/// * `event_ptr` - Pointer to the incoming event in host memory.
/// * `is_owner` - Ownership flag sent by the runtime. `1` means owner, any other value means non-owner.
/// * `callback` - Contract logic with signature `fn(&Context<Event>, &mut ContractResult<State>)`.
///
/// # Returns
///
/// Pointer to a serialized `ContractResultData`.
///
/// # Fallback behaviour
///
/// If `state_ptr` cannot be deserialized (e.g. the subject state is empty on the
/// first event), the function silently falls back to `init_state_ptr`. This allows
/// contracts to bootstrap from the initial state defined at creation time.
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
///                 result.accept();
///             }
///             Event::Delete => {
///                 if context.is_owner {
///                     result.state.deleted = true;
///                     result.accept();
///                 } else {
///                     result.reject("Only owner can delete");
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
    let error: String;
    'process: {
        let state = match read_and_parse::<State>(state_ptr, "State") {
            Ok(state) => state,
            Err(_) => {
                match read_and_parse::<State>(init_state_ptr, "Init State") {
                    Ok(init_state) => init_state,
                    Err(e) => {
                        error = e;
                        break 'process;
                    }
                }
            }
        };
        let event = match read_and_parse::<Event>(event_ptr, "Event") {
            Ok(e) => e,
            Err(e) => {
                error = e;
                break 'process;
            }
        };
        let is_owner = is_owner == 1;
        let context = Context { event, is_owner };
        let mut contract_result = ContractResult::new(state);
        callback(&context, &mut contract_result);
        let state_bytes = match serde_json::to_vec(&contract_result.state) {
            Ok(bytes) => bytes,
            Err(_) => {
                error = "Cannot serialize contract final state into JSON bytes".to_owned();
                break 'process;
            }
        };
        let result = ContractResultData {
            final_state: ContractData(state_bytes),
            success: contract_result.success,
            error: if contract_result.success {
                String::new()
            } else {
                format!("Error running contract event: {}", contract_result.error)
            },
        };
        let Ok(result_ptr) = store(&result) else {
            error = "Cannot return contract result".to_owned();
            break 'process;
        };
        return result_ptr;
    }
    // Attempt to return error via store, but if that fails too, return 0 pointer.
    // The host should handle 0 pointer as a fatal error.
    store(&ContractResultData::error(&error)).unwrap_or(0)
}

/// Deserializes data from bytes using Borsh format.
fn deserialize(bytes: Vec<u8>) -> Result<ContractData, Error> {
    BorshDeserialize::try_from_slice(&bytes).map_err(|e| Error::Deserialization(e.to_string()))
}

/// Serializes data into bytes using Borsh format.
fn serialize<S: BorshSerialize>(data: S) -> Result<Vec<u8>, Error> {
    borsh::to_vec(&data).map_err(|e| Error::Serialization(e.to_string()))
}

/// Reads a typed value from host memory through the full deserialization pipeline.
///
/// 1. Reads raw bytes from the host at `ptr`.
/// 2. Deserializes the Borsh payload into a `ContractData` (raw JSON bytes).
/// 3. Parses the JSON bytes directly into the requested type `T` without an intermediate DOM.
///
/// `context_name` is used only to build informative error messages.
fn read_and_parse<T>(ptr: i32, context_name: &str) -> Result<T, String>
where
    T: for<'a> Deserialize<'a>,
{
    let bytes = get_from_context(ptr)
        .map_err(|e| format!("Cannot read {context_name} from host memory: {e}"))?;
    let data: ContractData = deserialize(bytes)
        .map_err(|e| format!("Cannot deserialize {context_name}: {e}"))?;
    serde_json::from_slice::<T>(&data.0)
        .map_err(|e| format!("Cannot parse {context_name} from JSON bytes: {e}"))
}

/// Reads data from WASM host memory at the given pointer.
///
/// The host provides a pointer and length through the external memory API.
fn get_from_context(pointer: i32) -> Result<Vec<u8>, Error> {
    unsafe {
        let len = externf::pointer_len(pointer);

        // Reject oversized host input before allocating.
        if len > MAX_DATA_SIZE {
            return Err(Error::MemoryLimitExceeded {
                requested: len as usize,
                max: MAX_DATA_SIZE as usize,
            });
        }

        // Negative lengths are invalid host input.
        if len < 0 {
            return Err(Error::Deserialization(
                "Invalid negative length from host".to_owned(),
            ));
        }

        let mut data = Vec::with_capacity(len as usize);
        data.set_len(len as usize);
        externf::read_bytes(pointer, data.as_mut_ptr() as i32, len);
        Ok(data)
    }
}

/// Stores data in WASM memory to be read by the host.
///
/// Serializes `data`, allocates host-visible memory, and writes the bytes there.
fn store<S>(data: &S) -> Result<u32, Error>
where
    S: BorshSerialize,
{
    let bytes = serialize(data).map_err(|e| Error::Serialization(e.to_string()))?;

    // The host allocator expects a `u32` byte length.
    let len = u32::try_from(bytes.len()).map_err(|_| {
        Error::IntegerOverflow(format!(
            "Serialized data too large: {} bytes exceeds u32::MAX",
            bytes.len()
        ))
    })?;

    unsafe {
        let raw_ptr = externf::alloc(len);
        if raw_ptr == 0 {
            return Err(Error::MemoryLimitExceeded {
                requested: bytes.len(),
                max: MAX_DATA_SIZE as usize,
            });
        }
        let ptr = raw_ptr as u32;
        externf::write_bytes(ptr as i32, bytes.as_ptr() as i32, len as i32);
        Ok(ptr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ave_common::ValueWrapper;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
    struct TestState {
        value: i32,
        name: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    enum TestEvent {
        Increment,
        Decrement,
        SetValue(i32),
        Rename(String),
    }

    #[test]
    fn test_context_creation() {
        let event = TestEvent::Increment;
        let context = Context {
            event,
            is_owner: true,
        };
        assert!(context.is_owner);
    }

    #[test]
    fn test_contract_result_new() {
        let state = TestState {
            value: 42,
            name: "test".to_string(),
        };
        let result = ContractResult::new(state.clone());
        assert_eq!(result.state.value, 42);
        assert!(!result.success);
        assert_eq!(result.error, "");
    }

    #[test]
    fn test_contract_result_success() {
        let state = TestState {
            value: 10,
            name: "Alice".to_string(),
        };
        let mut result = ContractResult::new(state);
        result.state.value = 20;
        result.success = true;

        assert_eq!(result.state.value, 20);
        assert!(result.success);
        assert_eq!(result.error, "");
    }

    #[test]
    fn test_contract_result_error() {
        let state = TestState {
            value: 5,
            name: "Bob".to_string(),
        };
        let mut result = ContractResult::new(state);
        result.success = false;
        result.error = "Invalid operation".to_string();

        assert!(!result.success);
        assert_eq!(result.error, "Invalid operation");
    }

    #[test]
    fn test_contract_init_check_default() {
        let check = ContractInitCheck::default();
        assert!(!check.success);
        assert_eq!(check.error, "");
    }

    #[test]
    fn test_contract_init_check_success() {
        let mut check = ContractInitCheck::default();
        check.success = true;
        assert!(check.success);
        assert_eq!(check.error, "");
    }

    #[test]
    fn test_contract_init_check_error() {
        let mut check = ContractInitCheck::default();
        check.success = false;
        check.error = "Invalid initial state".to_string();
        assert!(!check.success);
        assert_eq!(check.error, "Invalid initial state");
    }

    #[test]
    fn test_contract_result_data_error() {
        let result = ContractResultData::error("test error");
        assert!(!result.success);
        assert_eq!(result.error, "test error");
        let json_value: serde_json::Value = serde_json::from_slice(&result.final_state.0).unwrap();
        assert_eq!(json_value, serde_json::Value::Null);
    }

    #[test]
    fn test_contract_init_check_data_ok() {
        let result = ContractInitCheckData::ok();
        assert!(result.success);
        assert_eq!(result.error, "");
    }

    #[test]
    fn test_contract_init_check_data_error() {
        let result = ContractInitCheckData::error("validation failed");
        assert!(!result.success);
        assert_eq!(result.error, "validation failed");
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let state = TestState {
            value: 100,
            name: "test".to_string(),
        };
        let json_bytes = serde_json::to_vec(&state).unwrap();
        let data = ContractData(json_bytes);

        let serialized = serialize(&data).unwrap();
        let deserialized: ContractData = deserialize(serialized).unwrap();

        let recovered_state: TestState = serde_json::from_slice(&deserialized.0).unwrap();
        assert_eq!(recovered_state.value, 100);
        assert_eq!(recovered_state.name, "test");
    }

    #[test]
    fn test_serialize_contract_result_data() {
        let state = TestState {
            value: 42,
            name: "Alice".to_string(),
        };
        let state_bytes = serde_json::to_vec(&state).unwrap();
        let result = ContractResultData {
            final_state: ContractData(state_bytes),
            success: true,
            error: String::new(),
        };

        let serialized = serialize(&result);
        assert!(serialized.is_ok());
    }

    #[test]
    fn test_serialize_contract_init_check_data() {
        let check = ContractInitCheckData {
            success: true,
            error: String::new(),
        };

        let serialized = serialize(&check);
        assert!(serialized.is_ok());
    }

    #[test]
    fn test_deserialize_invalid_data() {
        let invalid_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let result = deserialize(invalid_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_context_is_owner_true() {
        let event = TestEvent::SetValue(100);
        let context = Context {
            event,
            is_owner: true,
        };
        assert!(context.is_owner);
    }

    #[test]
    fn test_context_is_owner_false() {
        let event = TestEvent::SetValue(100);
        let context = Context {
            event,
            is_owner: false,
        };
        assert!(!context.is_owner);
    }

    #[test]
    fn test_contract_result_state_modification() {
        let initial_state = TestState {
            value: 0,
            name: "Initial".to_string(),
        };
        let mut result = ContractResult::new(initial_state);

        result.state.value = 999;
        result.state.name = "Modified".to_string();
        result.success = true;

        assert_eq!(result.state.value, 999);
        assert_eq!(result.state.name, "Modified");
        assert!(result.success);
    }

    #[test]
    fn test_serialize_complex_nested_structure() {
        let mut inner_map = serde_json::Map::new();
        inner_map.insert("nested".to_string(), serde_json::json!({"deep": "value"}));

        let complex_value = serde_json::json!({
            "array": [1, 2, 3],
            "object": inner_map,
            "string": "test",
            "number": 42,
            "bool": true,
            "null": null
        });

        let json_bytes = serde_json::to_vec(&complex_value).unwrap();
        let data = ContractData(json_bytes);
        let serialized = serialize(&data).unwrap();
        let deserialized: ContractData = deserialize(serialized).unwrap();

        let original_json: serde_json::Value = serde_json::from_slice(&data.0).unwrap();
        let recovered_json: serde_json::Value = serde_json::from_slice(&deserialized.0).unwrap();
        assert_eq!(original_json, recovered_json);
    }

    #[test]
    fn test_max_data_size_constant() {
        assert_eq!(MAX_DATA_SIZE, 10_000_000);
    }

    #[test]
    fn test_contract_result_json_serialization() {
        let state = TestState {
            value: 123,
            name: "JsonTest".to_string(),
        };
        let result = ContractResult {
            state,
            success: true,
            error: String::new(),
        };

        let json = serde_json::to_string(&result);
        assert!(json.is_ok());

        let json_str = json.unwrap();
        assert!(json_str.contains("123"));
        assert!(json_str.contains("JsonTest"));
        assert!(json_str.contains("true"));
    }

    #[test]
    fn test_context_json_serialization() {
        let event = TestEvent::Increment;
        let context = Context {
            event,
            is_owner: true,
        };

        let json = serde_json::to_string(&context);
        assert!(json.is_ok());
    }

    #[test]
    fn test_contract_init_check_json_serialization() {
        let check = ContractInitCheck {
            success: true,
            error: String::new(),
        };

        let json = serde_json::to_string(&check);
        assert!(json.is_ok());

        let json_str = json.unwrap();
        assert!(json_str.contains("true"));
    }

    #[test]
    fn test_multiple_contract_results() {
        let states = vec![
            TestState {
                value: 1,
                name: "one".to_string(),
            },
            TestState {
                value: 2,
                name: "two".to_string(),
            },
            TestState {
                value: 3,
                name: "three".to_string(),
            },
        ];

        let results: Vec<ContractResult<TestState>> =
            states.into_iter().map(ContractResult::new).collect();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].state.value, 1);
        assert_eq!(results[1].state.value, 2);
        assert_eq!(results[2].state.value, 3);
    }

    #[test]
    fn test_value_wrapper_public_access() {
        let value = serde_json::json!({"test": "value"});
        let wrapper = ValueWrapper(value.clone());
        assert_eq!(wrapper.0, value);
    }

    #[test]
    fn test_empty_error_string() {
        let state = TestState {
            value: 0,
            name: String::new(),
        };
        let result = ContractResult::new(state);
        assert_eq!(result.error.len(), 0);
    }

    #[test]
    fn test_contract_result_accept() {
        let state = TestState {
            value: 1,
            name: "test".to_string(),
        };
        let mut result = ContractResult::new(state);
        assert!(!result.success);
        result.accept();
        assert!(result.success);
        assert_eq!(result.error, "");
    }

    #[test]
    fn test_contract_result_reject() {
        let state = TestState {
            value: 1,
            name: "test".to_string(),
        };
        let mut result = ContractResult::new(state);
        result.reject("something went wrong");
        assert!(!result.success);
        assert_eq!(result.error, "something went wrong");
    }

    #[test]
    fn test_contract_init_check_accept() {
        let mut check = ContractInitCheck::default();
        assert!(!check.success);
        check.accept();
        assert!(check.success);
    }

    #[test]
    fn test_contract_init_check_reject() {
        let mut check = ContractInitCheck::default();
        check.reject("invalid state");
        assert!(!check.success);
        assert_eq!(check.error, "invalid state");
    }
}
