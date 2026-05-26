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
mod tests;
