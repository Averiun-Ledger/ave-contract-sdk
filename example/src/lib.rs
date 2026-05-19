use serde::{Serialize, Deserialize};
use ave_contract_sdk as sdk;

/// Contract state used by the example.
#[derive(Serialize, Deserialize, Clone)]
struct State {
  pub one: u32,
  pub two: u32,
  pub three: u32
}

#[derive(Serialize, Deserialize)]
enum StateEvent {
  ModOne { data: u32 },
  ModTwo { data: u32 },
  ModThree { data: u32 },
  ModAll { one: u32, two: u32, three: u32 }
}

#[unsafe(no_mangle)]
pub unsafe fn main_function(state_ptr: i32, init_state_ptr: i32, event_ptr: i32, is_owner: i32) -> u32 {
  sdk::execute_contract(state_ptr, init_state_ptr, event_ptr, is_owner, contract_logic)
}

#[unsafe(no_mangle)]
pub unsafe fn init_check_function(state_ptr: i32) -> u32 {
  sdk::check_init_data(state_ptr, init_logic)
}

fn init_logic(
  _state: &State,
  contract_result: &mut sdk::ContractInitCheck,
) {
  contract_result.success = true;
}

fn contract_logic(
  context: &sdk::Context<StateEvent>,
  contract_result: &mut sdk::ContractResult<State>,
) {
  let state = &mut contract_result.state;
  match context.event {
      StateEvent::ModOne { data } => {
        state.one = data;
      },
      StateEvent::ModTwo { data } => {
        state.two = data;
      },
      StateEvent::ModThree { data } => {
        if data == 50 {
          contract_result.error = "Can not change three value, 50 is a invalid value".to_owned();
          return
        }
        
        state.three = data;
      },
      StateEvent::ModAll { one, two, three } => {
        state.one = one;
        state.two = two;
        state.three = three;
      }
  }
  contract_result.success = true;
}

#[test]
fn contract_test() {
  let initial_state = State {
    one: 1,
    two: 2,
    three: 3
  };
  let context = sdk::Context {
    event: StateEvent::ModOne { data: 100 },
    is_owner: false
  };
  let mut result = sdk::ContractResult::new(initial_state);
  contract_logic(&context, &mut result);
  assert_eq!(result.state.one, 100);
  assert!(result.success);
}

#[test]
fn contract_test_mod_two() {
  let initial_state = State {
    one: 1,
    two: 2,
    three: 3
  };
  let context = sdk::Context {
    event: StateEvent::ModTwo { data: 42 },
    is_owner: false
  };
  let mut result = sdk::ContractResult::new(initial_state);
  contract_logic(&context, &mut result);
  assert_eq!(result.state.two, 42);
  assert!(result.success);
}

#[test]
fn contract_test_mod_three_success() {
  let initial_state = State {
    one: 1,
    two: 2,
    three: 3
  };
  let context = sdk::Context {
    event: StateEvent::ModThree { data: 49 },
    is_owner: false
  };
  let mut result = sdk::ContractResult::new(initial_state);
  contract_logic(&context, &mut result);
  assert_eq!(result.state.three, 49);
  assert!(result.success);
}

#[test]
fn contract_test_fail() {
  let initial_state = State {
    one: 1,
    two: 2,
    three: 3
  };
  let context = sdk::Context {
    event: StateEvent::ModThree { data: 50 },
    is_owner: false
  };
  let mut result = sdk::ContractResult::new(initial_state);
  contract_logic(&context, &mut result);
  assert_eq!(result.state.three, 3);
  assert_eq!(result.error, "Can not change three value, 50 is a invalid value");
  assert!(!result.success);
}

#[test]
fn contract_test_mod_all() {
  let initial_state = State {
    one: 1,
    two: 2,
    three: 3
  };
  let context = sdk::Context {
    event: StateEvent::ModAll { one: 10, two: 20, three: 30 },
    is_owner: false
  };
  let mut result = sdk::ContractResult::new(initial_state);
  contract_logic(&context, &mut result);
  assert_eq!(result.state.one, 10);
  assert_eq!(result.state.two, 20);
  assert_eq!(result.state.three, 30);
  assert!(result.success);
}

#[test]
fn init_test() {
  let state = State {
    one: 1,
    two: 2,
    three: 3
  };
  let mut check = sdk::ContractInitCheck::default();
  init_logic(&state, &mut check);
  assert!(check.success);
}

