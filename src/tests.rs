use super::*;
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
    let bytes = externf::get_data(ptr as i32).expect("Result data not found in mock host memory");
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
fn test_deserialize_invalid_data() {
    let invalid_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
    let result = deserialize(&invalid_bytes);
    assert!(result.is_err());
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
#[test]
fn test_context_json_roundtrip() {
    let event = TestEvent::SetValue(42);
    let context = Context {
        event,
        is_owner: true,
    };

    let json = serde_json::to_string(&context).unwrap();
    let recovered: Context<TestEvent> = serde_json::from_str(&json).unwrap();
    match recovered.event {
        TestEvent::SetValue(v) => assert_eq!(v, 42),
        _ => panic!("Wrong event type"),
    }
    assert!(recovered.is_owner);
}

#[test]
fn test_contract_init_check_json_roundtrip() {
    let mut check = ContractInitCheck::default();
    check.reject("bad state");

    let json = serde_json::to_string(&check).unwrap();
    let recovered: ContractInitCheck = serde_json::from_str(&json).unwrap();
    assert!(!recovered.success);
    assert_eq!(recovered.error, "bad state");
}

#[test]
fn test_is_owner_boundary_values() {
    externf::reset();
    let state = TestState {
        value: 0,
        name: "test".to_string(),
    };
    let state_ptr = setup_host_data(&state);
    let event = TestEvent::Increment;
    let event_ptr = setup_host_data(&event);

    // is_owner = 1 means owner
    let ptr1 = execute_contract::<_, TestState, TestEvent>(
        state_ptr,
        state_ptr,
        event_ptr,
        1,
        |_ctx, res| {
            if _ctx.is_owner {
                res.state.value = 100;
                res.accept();
            } else {
                res.reject("not owner");
            }
        },
    );
    let r1: ContractResultData = read_host_result(ptr1);
    assert!(r1.success);
    let s1: TestState = serde_json::from_slice(&r1.final_state.0).unwrap();
    assert_eq!(s1.value, 100);

    // Any value other than 1 should be treated as non-owner
    for raw in [0, 2, -1, 99] {
        let ptr = execute_contract::<_, TestState, TestEvent>(
            state_ptr,
            state_ptr,
            event_ptr,
            raw,
            |_ctx, res| {
                if _ctx.is_owner {
                    res.state.value = 100;
                    res.accept();
                } else {
                    res.reject("not owner");
                }
            },
        );
        let r: ContractResultData = read_host_result(ptr);
        assert!(!r.success, "expected failure for is_owner={}", raw);
        assert!(r.error.contains("not owner"));
    }
}

#[test]
fn test_check_init_data_callback_leaves_default() {
    externf::reset();
    let state = TestState {
        value: 10,
        name: "test".to_string(),
    };
    let ptr = setup_host_data(&state);

    // Callback does nothing: check remains with success = false
    let result_ptr = check_init_data::<TestState, _>(ptr, |_state, _check| {
        // intentionally empty
    });

    assert!(result_ptr != 0);
    let result: ContractInitCheckData = read_host_result(result_ptr);
    assert!(!result.success);
    assert!(result.error.contains("Error running init contract data"));
}

#[test]
fn test_contract_result_accept_then_reject() {
    let state = TestState {
        value: 1,
        name: "test".to_string(),
    };
    let mut result = ContractResult::new(state);
    result.accept();
    assert!(result.success);
    result.reject("changed mind");
    assert!(!result.success);
    assert_eq!(result.error, "changed mind");
}

#[test]
fn test_contract_init_check_accept_then_reject() {
    let mut check = ContractInitCheck::default();
    check.accept();
    assert!(check.success);
    check.reject("revised");
    assert!(!check.success);
    assert_eq!(check.error, "revised");
}

#[test]
fn test_contract_result_data_roundtrip() {
    let state = TestState {
        value: 77,
        name: "Roundtrip".to_string(),
    };
    let bytes = serde_json::to_vec(&state).unwrap();
    let original = ContractResultData {
        final_state: ContractData(bytes),
        success: true,
        error: String::new(),
    };

    let serialized = serialize(&original).unwrap();
    let recovered: ContractResultData = BorshDeserialize::try_from_slice(&serialized).unwrap();
    assert!(recovered.success);
    assert_eq!(recovered.error, "");
    let recovered_state: TestState = serde_json::from_slice(&recovered.final_state.0).unwrap();
    assert_eq!(recovered_state.value, 77);
    assert_eq!(recovered_state.name, "Roundtrip");
}

#[test]
fn test_contract_init_check_data_roundtrip() {
    let original = ContractInitCheckData {
        success: false,
        error: "validation error".to_string(),
    };

    let serialized = serialize(&original).unwrap();
    let recovered: ContractInitCheckData = BorshDeserialize::try_from_slice(&serialized).unwrap();
    assert!(!recovered.success);
    assert_eq!(recovered.error, "validation error");
}

#[test]
fn test_execute_contract_event_rename() {
    externf::reset();
    let state = TestState {
        value: 5,
        name: "old_name".to_string(),
    };
    let state_ptr = setup_host_data(&state);

    let event = TestEvent::Rename("new_name".to_string());
    let event_ptr = setup_host_data(&event);

    let result_ptr = execute_contract::<_, TestState, TestEvent>(
        state_ptr,
        state_ptr,
        event_ptr,
        0,
        |context, result| {
            if let TestEvent::Rename(name) = &context.event {
                result.state.name = name.clone();
                result.accept();
            }
        },
    );

    assert!(result_ptr != 0);
    let result: ContractResultData = read_host_result(result_ptr);
    assert!(result.success);
    let final_state: TestState = serde_json::from_slice(&result.final_state.0).unwrap();
    assert_eq!(final_state.name, "new_name");
    assert_eq!(final_state.value, 5); // unchanged
}

#[test]
fn test_execute_contract_event_decrement() {
    externf::reset();
    let state = TestState {
        value: 10,
        name: "counter".to_string(),
    };
    let state_ptr = setup_host_data(&state);

    let event = TestEvent::Decrement;
    let event_ptr = setup_host_data(&event);

    let result_ptr = execute_contract::<_, TestState, TestEvent>(
        state_ptr,
        state_ptr,
        event_ptr,
        1,
        |context, result| {
            if let TestEvent::Decrement = &context.event {
                result.state.value -= 1;
                result.accept();
            }
        },
    );

    assert!(result_ptr != 0);
    let result: ContractResultData = read_host_result(result_ptr);
    assert!(result.success);
    let final_state: TestState = serde_json::from_slice(&result.final_state.0).unwrap();
    assert_eq!(final_state.value, 9);
}

#[test]
fn test_check_init_data_with_validation() {
    externf::reset();
    let valid_state = TestState {
        value: 50,
        name: "valid".to_string(),
    };
    let valid_ptr = setup_host_data(&valid_state);

    let result_ptr = check_init_data::<TestState, _>(valid_ptr, |state, check| {
        if state.value > 100 {
            check.reject("value too high");
        } else {
            check.accept();
        }
    });
    let result: ContractInitCheckData = read_host_result(result_ptr);
    assert!(result.success);

    let invalid_state = TestState {
        value: 150,
        name: "invalid".to_string(),
    };
    let invalid_ptr = setup_host_data(&invalid_state);

    let result_ptr = check_init_data::<TestState, _>(invalid_ptr, |state, check| {
        if state.value > 100 {
            check.reject("value too high");
        } else {
            check.accept();
        }
    });
    let result: ContractInitCheckData = read_host_result(result_ptr);
    assert!(!result.success);
    assert!(result.error.contains("value too high"));
}
