# ave-contract-sdk

Rust SDK for writing Ave Ledger contracts compiled to WASM.

## What it does

`ave-contract-sdk` handles the repetitive part of a contract:

- Reads state and event data from WASM host memory.
- Converts those values into Rust types with `serde`.
- Executes the contract logic through a callback.
- Returns the serialized result so the runtime can apply or reject the state change.

The contract only needs to define:

- The state type.
- The event type.
- The initial state validation.
- The logic that updates the state.

## What it receives and what it returns

The public API is intentionally small.

### `check_init_data`

Validates the initial state before the contract is created.

Receives:

- `state_ptr: i32`: pointer to the initial state in host memory.
- `callback`: function with signature `fn(&State, &mut ContractInitCheck)`.

Does:

- Reads and deserializes the initial state.
- Runs the validation logic defined by the contract.

Returns:

- `u32`: pointer to the serialized result with `success` and `error`.

### `execute_contract`

Executes an event against the current state.

Receives:

- `state_ptr: i32`: pointer to the current state.
- `init_state_ptr: i32`: pointer to the initial state; used as a fallback if the current state does not exist yet or cannot be recovered.
- `event_ptr: i32`: pointer to the incoming event.
- `is_owner: i32`: `1` if the event sender is the owner, `0` otherwise.
- `callback`: function with signature `fn(&Context<Event>, &mut ContractResult<State>)`.

Does:

- Deserializes state and event.
- Builds `Context<Event>`.
- Runs the contract logic.
- Serializes the final state and execution result.

Returns:

- `u32`: pointer to the serialized result with `final_state`, `success`, and `error`.

## Types you will use

### `Context<Event>`

Describes the execution context:

- `event`: event that triggers the logic.
- `is_owner`: whether the operation is performed by the owner.

### `ContractResult<State>`

Execution result:

- `state`: final state.
- `success`: whether the change should be applied.
- `error`: rejection reason when `success == false`.

### `ContractInitCheck`

Initial validation result:

- `success`: whether the initial state is valid.
- `error`: rejection reason.

## Minimal example

```rust
use ave_contract_sdk as sdk;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
struct State {
    value: String,
}

#[derive(Serialize, Deserialize)]
enum Event {
    SetValue { value: String },
}

#[unsafe(no_mangle)]
pub unsafe fn main_function(
    state_ptr: i32,
    init_state_ptr: i32,
    event_ptr: i32,
    is_owner: i32,
) -> u32 {
    sdk::execute_contract(state_ptr, init_state_ptr, event_ptr, is_owner, contract_logic)
}

#[unsafe(no_mangle)]
pub unsafe fn init_check_function(state_ptr: i32) -> u32 {
    sdk::check_init_data(state_ptr, init_logic)
}

fn init_logic(_state: &State, result: &mut sdk::ContractInitCheck) {
    result.success = true;
}

fn contract_logic(
    context: &sdk::Context<Event>,
    result: &mut sdk::ContractResult<State>,
) {
    match &context.event {
        Event::SetValue { value } => {
            result.state.value = value.clone();
            result.success = true;
        }
    }
}
```

You can find complete examples in:

- [`example/src/lib.rs`](/home/ale/dev/ave-contract-sdk/example/src/lib.rs)
- [`example2/src/lib.rs`](/home/ale/dev/ave-contract-sdk/example2/src/lib.rs)

## Contract flow

1. The runtime calls `init_check_function` to validate the initial state.
2. The runtime calls `main_function` to apply an event.
3. The contract updates `result.state`.
4. The contract sets `result.success = true` or provides an explicit error.

## Repository structure

- [`src/lib.rs`](/home/ale/dev/ave-contract-sdk/src/lib.rs): public SDK API.
- [`src/externf.rs`](/home/ale/dev/ave-contract-sdk/src/externf.rs): external functions used to interact with host memory.
- [`src/error.rs`](/home/ale/dev/ave-contract-sdk/src/error.rs): internal SDK errors.
- [`tests/integration_tests.rs`](/home/ale/dev/ave-contract-sdk/tests/integration_tests.rs): integration tests.
- [`example/src/lib.rs`](/home/ale/dev/ave-contract-sdk/example/src/lib.rs): example with multiple events.
- [`example2/src/lib.rs`](/home/ale/dev/ave-contract-sdk/example2/src/lib.rs): minimal example.

## Development

Run the test suite with:

```bash
cargo test
```

The SDK uses `serde` to map state and events, and `borsh` for binary exchange with the WASM runtime.

## License

This project is a fork of [kore-contract-sdk](https://github.com/kore-ledger/kore-contract-sdk), originally developed by Kore Ledger, SL, modified in 2025 by Averiun Ledger, SL, and distributed under the same AGPL-3.0-only license.