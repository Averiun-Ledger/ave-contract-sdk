use ave_common::ValueWrapper;
use ave_contract_sdk::{Context, ContractInitCheck, ContractResult};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct CounterState {
    count: i32,
    owner: String,
}

#[derive(Serialize, Deserialize, Debug)]
enum CounterEvent {
    Increment,
    Decrement,
    Reset,
    SetCount(i32),
}

#[test]
fn test_contract_workflow_increment() {
    let initial_state = CounterState {
        count: 0,
        owner: "alice".to_string(),
    };

    let event = CounterEvent::Increment;
    let context = Context {
        event,
        is_owner: true,
    };

    let mut result = ContractResult::new(initial_state);

    // Simulate contract logic
    match &context.event {
        CounterEvent::Increment => {
            result.state.count += 1;
            result.success = true;
        }
        _ => {}
    }

    assert_eq!(result.state.count, 1);
    assert!(result.success);
    assert_eq!(result.error, "");
}

#[test]
fn test_contract_workflow_decrement() {
    let initial_state = CounterState {
        count: 10,
        owner: "bob".to_string(),
    };

    let event = CounterEvent::Decrement;
    let context = Context {
        event,
        is_owner: false,
    };

    let mut result = ContractResult::new(initial_state);

    match &context.event {
        CounterEvent::Decrement => {
            result.state.count -= 1;
            result.success = true;
        }
        _ => {}
    }

    assert_eq!(result.state.count, 9);
    assert!(result.success);
}

#[test]
fn test_contract_workflow_owner_only_operation() {
    let initial_state = CounterState {
        count: 5,
        owner: "charlie".to_string(),
    };

    let event = CounterEvent::Reset;
    let context = Context {
        event,
        is_owner: false,
    };

    let mut result = ContractResult::new(initial_state);

    match &context.event {
        CounterEvent::Reset => {
            if context.is_owner {
                result.state.count = 0;
                result.success = true;
            } else {
                result.success = false;
                result.error = "Only owner can reset".to_string();
            }
        }
        _ => {}
    }

    assert_eq!(result.state.count, 5); // State unchanged
    assert!(!result.success);
    assert_eq!(result.error, "Only owner can reset");
}

#[test]
fn test_contract_workflow_owner_reset() {
    let initial_state = CounterState {
        count: 100,
        owner: "dave".to_string(),
    };

    let event = CounterEvent::Reset;
    let context = Context {
        event,
        is_owner: true,
    };

    let mut result = ContractResult::new(initial_state);

    match &context.event {
        CounterEvent::Reset => {
            if context.is_owner {
                result.state.count = 0;
                result.success = true;
            } else {
                result.success = false;
                result.error = "Only owner can reset".to_string();
            }
        }
        _ => {}
    }

    assert_eq!(result.state.count, 0);
    assert!(result.success);
}

#[test]
fn test_init_check_valid_state() {
    let state = CounterState {
        count: 50,
        owner: "eve".to_string(),
    };

    let mut check = ContractInitCheck::default();

    // Simulate validation logic
    if state.count >= 0 && state.count <= 100 && !state.owner.is_empty() {
        check.success = true;
    } else {
        check.success = false;
        check.error = "Invalid initial state".to_string();
    }

    assert!(check.success);
    assert_eq!(check.error, "");
}

#[test]
fn test_init_check_invalid_count() {
    let state = CounterState {
        count: -10,
        owner: "frank".to_string(),
    };

    let mut check = ContractInitCheck::default();

    if state.count >= 0 && state.count <= 100 && !state.owner.is_empty() {
        check.success = true;
    } else {
        check.success = false;
        check.error = "Count must be between 0 and 100".to_string();
    }

    assert!(!check.success);
    assert_eq!(check.error, "Count must be between 0 and 100");
}

#[test]
fn test_init_check_empty_owner() {
    let state = CounterState {
        count: 50,
        owner: String::new(),
    };

    let mut check = ContractInitCheck::default();

    if state.count >= 0 && state.count <= 100 && !state.owner.is_empty() {
        check.success = true;
    } else {
        check.success = false;
        check.error = "Owner cannot be empty".to_string();
    }

    assert!(!check.success);
    assert_eq!(check.error, "Owner cannot be empty");
}

#[test]
fn test_state_serialization_roundtrip() {
    let state = CounterState {
        count: 42,
        owner: "grace".to_string(),
    };

    // Serialize to JSON
    let json_value = serde_json::to_value(&state).unwrap();
    let wrapper = ValueWrapper(json_value);

    // Serialize to Borsh
    let borsh_bytes = borsh::to_vec(&wrapper).unwrap();

    // Deserialize from Borsh
    let wrapper_back: ValueWrapper = borsh::BorshDeserialize::try_from_slice(&borsh_bytes).unwrap();

    // Deserialize from JSON
    let state_back: CounterState = serde_json::from_value(wrapper_back.0).unwrap();

    assert_eq!(state_back.count, 42);
    assert_eq!(state_back.owner, "grace");
}

#[test]
fn test_event_serialization_roundtrip() {
    let event = CounterEvent::SetCount(999);

    let json_value = serde_json::to_value(&event).unwrap();
    let wrapper = ValueWrapper(json_value);

    let borsh_bytes = borsh::to_vec(&wrapper).unwrap();
    let wrapper_back: ValueWrapper = borsh::BorshDeserialize::try_from_slice(&borsh_bytes).unwrap();

    let event_back: CounterEvent = serde_json::from_value(wrapper_back.0).unwrap();

    match event_back {
        CounterEvent::SetCount(val) => assert_eq!(val, 999),
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn test_contract_event_sequence_with_result_context() {
    let initial_state = CounterState {
        count: 0,
        owner: "henry".to_string(),
    };

    // Event 1: Increment (owner)
    let context1 = Context {
        event: CounterEvent::Increment,
        is_owner: true,
    };
    let mut result1 = ContractResult::new(initial_state);
    match &context1.event {
        CounterEvent::Increment => {
            result1.state.count += 1;
            result1.accept();
        }
        _ => {}
    }
    assert_eq!(result1.state.count, 1);
    assert!(result1.success);

    // Event 2: Increment again (non-owner)
    let context2 = Context {
        event: CounterEvent::Increment,
        is_owner: false,
    };
    let mut result2 = ContractResult::new(result1.state);
    match &context2.event {
        CounterEvent::Increment => {
            result2.state.count += 1;
            result2.accept();
        }
        _ => {}
    }
    assert_eq!(result2.state.count, 2);
    assert!(result2.success);

    // Event 3: SetCount (owner)
    let context3 = Context {
        event: CounterEvent::SetCount(50),
        is_owner: true,
    };
    let mut result3 = ContractResult::new(result2.state);
    match &context3.event {
        CounterEvent::SetCount(val) => {
            result3.state.count = *val;
            result3.accept();
        }
        _ => {}
    }
    assert_eq!(result3.state.count, 50);
    assert!(result3.success);

    // Event 4: Decrement (non-owner)
    let context4 = Context {
        event: CounterEvent::Decrement,
        is_owner: false,
    };
    let mut result4 = ContractResult::new(result3.state);
    match &context4.event {
        CounterEvent::Decrement => {
            result4.state.count -= 1;
            result4.accept();
        }
        _ => {}
    }
    assert_eq!(result4.state.count, 49);
    assert!(result4.success);
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ComplexState {
    nested: NestedData,
    array: Vec<i32>,
    optional: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct NestedData {
    value: i32,
    label: String,
}

#[test]
fn test_complex_state_serialization() {
    let state = ComplexState {
        nested: NestedData {
            value: 123,
            label: "test".to_string(),
        },
        array: vec![1, 2, 3, 4, 5],
        optional: Some("present".to_string()),
    };

    let json_value = serde_json::to_value(&state).unwrap();
    let wrapper = ValueWrapper(json_value);

    let borsh_bytes = borsh::to_vec(&wrapper).unwrap();
    let wrapper_back: ValueWrapper = borsh::BorshDeserialize::try_from_slice(&borsh_bytes).unwrap();

    let state_back: ComplexState = serde_json::from_value(wrapper_back.0).unwrap();

    assert_eq!(state_back.nested.value, 123);
    assert_eq!(state_back.nested.label, "test");
    assert_eq!(state_back.array, vec![1, 2, 3, 4, 5]);
    assert_eq!(state_back.optional, Some("present".to_string()));
}

#[test]
fn test_contract_result_full_workflow() {
    let initial_state = CounterState {
        count: 10,
        owner: "iris".to_string(),
    };

    let mut result = ContractResult::new(initial_state);

    // Verify initial state
    assert_eq!(result.state.count, 10);
    assert!(!result.success);

    // Modify state
    result.state.count = 20;
    result.success = true;

    // Serialize the result
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("20"));
    assert!(json.contains("iris"));
    assert!(json.contains("true"));

    // Deserialize back
    let result_back: ContractResult<CounterState> = serde_json::from_str(&json).unwrap();
    assert_eq!(result_back.state.count, 20);
    assert!(result_back.success);
}
