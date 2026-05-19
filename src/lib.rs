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
    pub const fn accept(&mut self) {
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
    pub const fn accept(&mut self) {
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
    match try_check_init_data(state_ptr, callback) {
        Ok(ptr) => ptr,
        Err(error) => store(&ContractInitCheckData::error(&error)).unwrap_or(0),
    }
}

fn try_check_init_data<State, F>(state_ptr: i32, callback: F) -> Result<u32, String>
where
    State: for<'a> Deserialize<'a> + Serialize + Clone,
    F: Fn(&State, &mut ContractInitCheck),
{
    let state = read_and_parse::<State>(state_ptr, "State")?;
    let mut contract_result = ContractInitCheck::default();
    callback(&state, &mut contract_result);

    if !contract_result.success {
        return Err(format!(
            "Error running init contract data: {}",
            contract_result.error
        ));
    }

    store(&ContractInitCheckData::ok()).map_err(|_| "Cannot return init contract result".to_owned())
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
    match try_execute_contract(state_ptr, init_state_ptr, event_ptr, is_owner, callback) {
        Ok(ptr) => ptr,
        Err(error) => store(&ContractResultData::error(&error)).unwrap_or(0),
    }
}

fn try_execute_contract<F, State, Event>(
    state_ptr: i32,
    init_state_ptr: i32,
    event_ptr: i32,
    is_owner: i32,
    callback: F,
) -> Result<u32, String>
where
    State: for<'a> Deserialize<'a> + Serialize + Clone,
    Event: for<'a> Deserialize<'a> + Serialize,
    F: Fn(&Context<Event>, &mut ContractResult<State>),
{
    let state = match read_and_parse::<State>(state_ptr, "State") {
        Ok(state) => state,
        Err(_) => read_and_parse::<State>(init_state_ptr, "Init State")?,
    };
    let event = read_and_parse::<Event>(event_ptr, "Event")?;
    let is_owner = is_owner == 1;
    let context = Context { event, is_owner };
    let mut contract_result = ContractResult::new(state);
    callback(&context, &mut contract_result);
    let state_bytes = serde_json::to_vec(&contract_result.state)
        .map_err(|_| "Cannot serialize contract final state into JSON bytes".to_owned())?;
    let result = ContractResultData {
        final_state: ContractData(state_bytes),
        success: contract_result.success,
        error: if contract_result.success {
            String::new()
        } else {
            format!("Error running contract event: {}", contract_result.error)
        },
    };
    store(&result).map_err(|_| "Cannot return contract result".to_owned())
}

/// Deserializes data from bytes using Borsh format.
fn deserialize(bytes: &[u8]) -> Result<ContractData, Error> {
    BorshDeserialize::try_from_slice(bytes).map_err(|e| Error::Deserialization(e.to_string()))
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
    let data: ContractData =
        deserialize(&bytes).map_err(|e| format!("Cannot deserialize {context_name}: {e}"))?;
    serde_json::from_slice::<T>(&data.0)
        .map_err(|e| format!("Cannot parse {context_name} from JSON bytes: {e}"))
}

/// Validates a length returned by the host before allocating a buffer.
fn validate_host_len(len: i32) -> Result<usize, Error> {
    if len > MAX_DATA_SIZE {
        return Err(Error::MemoryLimitExceeded {
            requested: len as usize,
            max: MAX_DATA_SIZE as usize,
        });
    }
    if len < 0 {
        return Err(Error::Deserialization(
            "Invalid negative length from host".to_owned(),
        ));
    }
    Ok(len as usize)
}

#[cfg(not(test))]
fn read_host_bytes(pointer: i32, data: &mut Vec<u8>, len: usize) {
    unsafe {
        externf::read_bytes(pointer, data.as_mut_ptr() as i32, len as i32);
        data.set_len(len);
    }
}

#[cfg(test)]
fn read_host_bytes(pointer: i32, data: &mut Vec<u8>, _len: usize) {
    externf::read_bytes_into_vec(pointer, data);
}

/// Reads data from WASM host memory at the given pointer.
///
/// The host provides a pointer and length through the external memory API.
#[allow(unused_unsafe)]
fn get_from_context(pointer: i32) -> Result<Vec<u8>, Error> {
    let len = validate_host_len(unsafe { externf::pointer_len(pointer) })?;
    let mut data = Vec::with_capacity(len);
    read_host_bytes(pointer, &mut data, len);
    Ok(data)
}

/// Validates that serialized bytes fit into a `u32` length expected by the host allocator.
fn validate_serialized_len(bytes: &[u8]) -> Result<u32, Error> {
    let max_len = i32::MAX as usize;
    if bytes.len() > max_len {
        return Err(Error::IntegerOverflow(format!(
            "Serialized data too large: {} bytes exceeds i32::MAX",
            bytes.len()
        )));
    }
    Ok(bytes.len() as u32)
}

#[cfg(not(test))]
fn write_host_bytes(ptr: i32, bytes: &[u8]) {
    unsafe {
        externf::write_bytes(ptr, bytes.as_ptr() as i32, bytes.len() as i32);
    }
}

#[cfg(test)]
fn write_host_bytes(ptr: i32, bytes: &[u8]) {
    externf::write_bytes_from_slice(ptr, bytes);
}

/// Stores data in WASM memory to be read by the host.
///
/// Serializes `data`, allocates host-visible memory, and writes the bytes there.
#[allow(unused_unsafe)]
fn store<S>(data: &S) -> Result<u32, Error>
where
    S: BorshSerialize,
{
    let bytes = serialize(data).map_err(|e| Error::Serialization(e.to_string()))?;
    let len = validate_serialized_len(&bytes)?;

    let raw_ptr = unsafe { externf::alloc(len) };
    if raw_ptr == 0 {
        return Err(Error::MemoryLimitExceeded {
            requested: bytes.len(),
            max: MAX_DATA_SIZE as usize,
        });
    }
    let ptr = raw_ptr as u32;
    write_host_bytes(ptr as i32, &bytes);
    Ok(ptr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ave_common::ValueWrapper;
    use borsh::BorshDeserialize;
    use serde::{Deserialize, Serialize};

    /// Helper: serializes `data` as JSON inside a `ContractData`, then Borsh-serializes
    /// that wrapper and stores it in mock host memory. Returns the simulated host pointer.
    ///
    /// **Note:** does **not** reset the mock host memory; callers must call
    /// `externf::reset()` themselves when needed.
    fn setup_host_data<T: Serialize>(data: &T) -> i32 {
        let json_bytes = serde_json::to_vec(data).unwrap();
        let contract_data = ContractData(json_bytes);
        let borsh_bytes = borsh::to_vec(&contract_data).unwrap();
        externf::store_data(borsh_bytes)
    }

    /// Helper: reads Borsh-serialized data of type `T` from the mock host memory at `ptr`.
    fn read_host_result<T: BorshDeserialize>(ptr: u32) -> T {
        let bytes =
            externf::get_data(ptr as i32).expect("Result data not found in mock host memory");
        T::try_from_slice(&bytes).expect("Failed to deserialize result from host memory")
    }

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
        let deserialized: ContractData = deserialize(&serialized).unwrap();

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
        let result = deserialize(&invalid_bytes);
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
        let deserialized: ContractData = deserialize(&serialized).unwrap();

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

    // ------------------------------------------------------------------
    // get_from_context
    // ------------------------------------------------------------------

    #[test]
    fn test_get_from_context_success() {
        externf::reset();
        let payload = vec![1, 2, 3, 4, 5];
        let ptr = externf::store_data(payload.clone());

        let result = get_from_context(ptr);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), payload);
    }

    #[test]
    fn test_get_from_context_oversized() {
        externf::reset();
        let huge = vec![0u8; (MAX_DATA_SIZE as usize) + 1];
        let ptr = externf::store_data(huge);

        let result = get_from_context(ptr);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::MemoryLimitExceeded { .. } => {}
            other => panic!("Expected MemoryLimitExceeded, got {:?}", other),
        }
    }

    #[test]
    fn test_get_from_context_negative_len() {
        externf::reset();
        externf::set_force_pointer_len(-1);

        let result = get_from_context(1234);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Deserialization(msg) => {
                assert!(msg.contains("Invalid negative length"));
            }
            other => panic!("Expected Deserialization error, got {:?}", other),
        }
    }

    // ------------------------------------------------------------------
    // store
    // ------------------------------------------------------------------

    #[test]
    fn test_store_success() {
        externf::reset();
        let data = ContractInitCheckData::ok();

        let ptr = store(&data);
        assert!(ptr.is_ok());
        let ptr = ptr.unwrap();
        assert!(ptr != 0);

        // Verify the stored bytes can be deserialized back
        let bytes = externf::get_data(ptr as i32).unwrap();
        let recovered: ContractInitCheckData = BorshDeserialize::try_from_slice(&bytes).unwrap();
        assert!(recovered.success);
    }

    #[test]
    fn test_store_alloc_fails() {
        externf::reset();
        externf::set_force_alloc_zero(true);
        let data = ContractInitCheckData::ok();

        let result = store(&data);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::MemoryLimitExceeded { .. } => {}
            other => panic!("Expected MemoryLimitExceeded, got {:?}", other),
        }
    }

    // ------------------------------------------------------------------
    // read_and_parse
    // ------------------------------------------------------------------

    #[test]
    fn test_read_and_parse_success() {
        let state = TestState {
            value: 42,
            name: "Alice".to_string(),
        };
        let ptr = setup_host_data(&state);

        let result: Result<TestState, String> = read_and_parse(ptr, "State");
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.value, 42);
        assert_eq!(parsed.name, "Alice");
    }

    #[test]
    fn test_read_and_parse_deserialize_error() {
        externf::reset();
        let bad_borsh = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let ptr = externf::store_data(bad_borsh);

        let result: Result<TestState, String> = read_and_parse(ptr, "State");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot deserialize State"));
    }

    #[test]
    fn test_read_and_parse_json_error() {
        externf::reset();
        // Valid Borsh of ContractData, but inner bytes are not valid JSON
        let contract_data = ContractData(vec![0xFF, 0xFF]);
        let borsh_bytes = borsh::to_vec(&contract_data).unwrap();
        let ptr = externf::store_data(borsh_bytes);

        let result: Result<TestState, String> = read_and_parse(ptr, "State");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("Cannot parse State from JSON bytes")
        );
    }

    // ------------------------------------------------------------------
    // check_init_data
    // ------------------------------------------------------------------

    #[test]
    fn test_check_init_data_success() {
        let state = TestState {
            value: 10,
            name: "test".to_string(),
        };
        let ptr = setup_host_data(&state);

        let result_ptr = check_init_data::<TestState, _>(ptr, |_state, check| {
            check.accept();
        });

        assert!(result_ptr != 0);
        let result: ContractInitCheckData = read_host_result(result_ptr);
        assert!(result.success);
    }

    #[test]
    fn test_check_init_data_rejected() {
        let state = TestState {
            value: 10,
            name: "test".to_string(),
        };
        let ptr = setup_host_data(&state);

        let result_ptr = check_init_data::<TestState, _>(ptr, |_state, check| {
            check.reject("not allowed");
        });

        assert!(result_ptr != 0);
        let result: ContractInitCheckData = read_host_result(result_ptr);
        assert!(!result.success);
        assert!(result.error.contains("not allowed"));
    }

    #[test]
    fn test_check_init_data_invalid_state() {
        externf::reset();
        let bad_borsh = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let ptr = externf::store_data(bad_borsh);

        let result_ptr = check_init_data::<TestState, _>(ptr, |_state, check| {
            check.accept();
        });

        assert!(result_ptr != 0);
        let result: ContractInitCheckData = read_host_result(result_ptr);
        assert!(!result.success);
        assert!(result.error.contains("Cannot deserialize State"));
    }

    // ------------------------------------------------------------------
    // execute_contract
    // ------------------------------------------------------------------

    #[test]
    fn test_execute_contract_success() {
        externf::reset();
        let state = TestState {
            value: 5,
            name: "contract".to_string(),
        };
        let state_ptr = setup_host_data(&state);

        let event = TestEvent::SetValue(99);
        let event_ptr = setup_host_data(&event);

        let result_ptr = execute_contract::<_, TestState, TestEvent>(
            state_ptr,
            state_ptr, // same as init_state for simplicity
            event_ptr,
            1,
            |context, result| {
                if let TestEvent::SetValue(v) = &context.event {
                    result.state.value = *v;
                    result.accept();
                }
            },
        );

        assert!(result_ptr != 0);
        let result: ContractResultData = read_host_result(result_ptr);
        assert!(result.success);
        let final_state: TestState = serde_json::from_slice(&result.final_state.0).unwrap();
        assert_eq!(final_state.value, 99);
    }

    #[test]
    fn test_execute_contract_fallback_to_init_state() {
        externf::reset();
        // Invalid current state pointer (bad borsh)
        let bad_state_ptr = externf::store_data(vec![0xFF]);

        let init_state = TestState {
            value: 10,
            name: "init".to_string(),
        };
        let init_state_ptr = setup_host_data(&init_state);

        let event = TestEvent::Increment;
        let event_ptr = setup_host_data(&event);

        let result_ptr = execute_contract::<_, TestState, TestEvent>(
            bad_state_ptr,
            init_state_ptr,
            event_ptr,
            0,
            |context, result| {
                if let TestEvent::Increment = &context.event {
                    result.state.value += 1;
                    result.accept();
                }
            },
        );

        assert!(result_ptr != 0);
        let result: ContractResultData = read_host_result(result_ptr);
        assert!(result.success);
        let final_state: TestState = serde_json::from_slice(&result.final_state.0).unwrap();
        // Started from init_state.value = 10, then incremented
        assert_eq!(final_state.value, 11);
    }

    #[test]
    fn test_execute_contract_invalid_event() {
        externf::reset();
        let state = TestState {
            value: 5,
            name: "test".to_string(),
        };
        let state_ptr = setup_host_data(&state);

        let bad_event_ptr = externf::store_data(vec![0xFF]);

        let result_ptr = execute_contract::<_, TestState, TestEvent>(
            state_ptr,
            state_ptr,
            bad_event_ptr,
            1,
            |_context, _result| {},
        );

        assert!(result_ptr != 0);
        let result: ContractResultData = read_host_result(result_ptr);
        assert!(!result.success);
        assert!(result.error.contains("Cannot deserialize Event"));
    }

    #[test]
    fn test_execute_contract_invalid_both_states() {
        externf::reset();
        let bad_state = externf::store_data(vec![0xFF]);
        let bad_event = externf::store_data(vec![0xFF]);

        let result_ptr = execute_contract::<_, TestState, TestEvent>(
            bad_state,
            bad_state,
            bad_event,
            1,
            |_context, _result| {},
        );

        assert!(result_ptr != 0);
        let result: ContractResultData = read_host_result(result_ptr);
        assert!(!result.success);
        assert!(result.error.contains("Cannot deserialize Init State"));
    }

    #[test]
    fn test_execute_contract_rejected_by_callback() {
        externf::reset();
        let state = TestState {
            value: 5,
            name: "test".to_string(),
        };
        let state_ptr = setup_host_data(&state);

        let event = TestEvent::Decrement;
        let event_ptr = setup_host_data(&event);

        let result_ptr = execute_contract::<_, TestState, TestEvent>(
            state_ptr,
            state_ptr,
            event_ptr,
            0,
            |_context, result| {
                result.reject("decrement not allowed");
            },
        );

        assert!(result_ptr != 0);
        let result: ContractResultData = read_host_result(result_ptr);
        assert!(!result.success);
        assert!(result.error.contains("decrement not allowed"));
    }

    #[test]
    fn test_check_init_data_store_fails() {
        let state = TestState {
            value: 10,
            name: "test".to_string(),
        };
        let ptr = setup_host_data(&state);
        externf::set_force_alloc_zero(true);

        let result_ptr = check_init_data::<TestState, _>(ptr, |_state, check| {
            check.accept();
        });

        assert_eq!(result_ptr, 0);
    }

    #[test]
    fn test_execute_contract_store_fails() {
        externf::reset();
        let state = TestState {
            value: 5,
            name: "test".to_string(),
        };
        let state_ptr = setup_host_data(&state);

        let event = TestEvent::Increment;
        let event_ptr = setup_host_data(&event);
        externf::set_force_alloc_zero(true);

        let result_ptr = execute_contract::<_, TestState, TestEvent>(
            state_ptr,
            state_ptr,
            event_ptr,
            1,
            |_context, result| {
                result.accept();
            },
        );

        assert_eq!(result_ptr, 0);
    }

    /// A type that always fails JSON serialization.
    #[derive(Deserialize, Clone, Debug)]
    struct FailingState;

    impl Serialize for FailingState {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("serialization always fails"))
        }
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct DummyEvent;

    #[test]
    fn test_execute_contract_state_serialize_fails() {
        externf::reset();
        // FailingState is a unit struct and deserializes from JSON `null`.
        // We cannot use `setup_host_data` because FailingState does not implement
        // Serialize, so we build the host payload manually.
        let json_bytes = serde_json::to_vec(&serde_json::Value::Null).unwrap();
        let contract_data = ContractData(json_bytes);
        let borsh_bytes = borsh::to_vec(&contract_data).unwrap();
        let state_ptr = externf::store_data(borsh_bytes);

        let event = DummyEvent;
        let event_ptr = setup_host_data(&event);

        let result_ptr = execute_contract::<_, FailingState, DummyEvent>(
            state_ptr,
            state_ptr,
            event_ptr,
            1,
            |_context, result| {
                result.accept();
            },
        );

        assert!(result_ptr != 0);
        let result: ContractResultData = read_host_result(result_ptr);
        assert!(!result.success);
        assert!(
            result
                .error
                .contains("Cannot serialize contract final state into JSON bytes")
        );
    }
}
