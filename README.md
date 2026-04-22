# ave-contract-sdk

[![Crates.io](https://img.shields.io/crates/v/ave-contract-sdk.svg)](https://crates.io/crates/ave-contract-sdk)
[![Docs.rs](https://docs.rs/ave-contract-sdk/badge.svg)](https://docs.rs/ave-contract-sdk)
[![License: AGPL-3.0-only](https://img.shields.io/badge/license-AGPL--3.0--only-blue.svg)](LICENSE)

Rust SDK for writing Ave Ledger smart contracts that are compiled to WebAssembly and executed by the Ave runtime.

The SDK keeps the contract-facing API small: contract authors define their state, event types, initialization checks, and update logic while the SDK handles host memory reads, serialization, deserialization, execution context creation, and result serialization.

## Installation

Add the SDK to your contract crate:

```toml
[dependencies]
ave-contract-sdk = "0.7.1"
serde = { version = "1", features = ["derive"] }
```

Contracts that are compiled for the Ave runtime should expose a `cdylib` artifact:

```toml
[lib]
crate-type = ["cdylib"]
```

This crate currently requires Rust `1.91.0` or newer and uses the Rust 2024 edition.

## Contract Interface

An Ave contract exposes two C ABI functions to the runtime:

- `init_check_function(state_ptr: i32) -> u32`
- `main_function(state_ptr: i32, init_state_ptr: i32, event_ptr: i32, is_owner: i32) -> u32`

Those exported functions delegate to the SDK:

- `check_init_data` validates the initial state before contract creation.
- `execute_contract` applies an event to the current state and returns the final state plus the execution status.

The runtime passes state and event data through WASM host memory. The SDK decodes that data using Borsh-wrapped JSON values, converts it into the contract's Rust types with `serde`, runs the callback provided by the contract, and stores the serialized result back in WASM memory for the host.

## Minimal Contract

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
pub unsafe fn init_check_function(state_ptr: i32) -> u32 {
    sdk::check_init_data(state_ptr, init_logic)
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

fn init_logic(state: &State, result: &mut sdk::ContractInitCheck) {
    if state.value.is_empty() {
        result.success = false;
        result.error = "initial value cannot be empty".to_owned();
        return;
    }

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

Build the contract for WebAssembly:

```bash
cargo build --release --target wasm32-unknown-unknown
```

## Public API

### `check_init_data`

```rust
pub fn check_init_data<State, F>(state_ptr: i32, callback: F) -> u32
where
    State: for<'a> serde::Deserialize<'a> + serde::Serialize + Clone,
    F: Fn(&State, &mut ContractInitCheck),
```

Validates the proposed initial state. The callback receives the deserialized state and a mutable `ContractInitCheck`. Set `success = true` to accept the state, or set `success = false` and provide `error` to reject it.

### `execute_contract`

```rust
pub fn execute_contract<F, State, Event>(
    state_ptr: i32,
    init_state_ptr: i32,
    event_ptr: i32,
    is_owner: i32,
    callback: F,
) -> u32
where
    State: for<'a> serde::Deserialize<'a> + serde::Serialize + Clone,
    Event: for<'a> serde::Deserialize<'a> + serde::Serialize,
    F: Fn(&Context<Event>, &mut ContractResult<State>),
```

Executes one event against the current state. If the current state cannot be converted into the contract state type, the SDK attempts to recover from `init_state_ptr`. The callback receives a `Context<Event>` and a mutable `ContractResult<State>`.

Set `result.success = true` when the event should be accepted. Leave it as `false`, or set it explicitly to `false`, and fill `result.error` when the event should be rejected.

## Core Types

### `Context<Event>`

Execution context passed to contract logic:

- `event`: the deserialized event being applied.
- `is_owner`: `true` when the runtime reports that the event sender is the owner.

### `ContractResult<State>`

Mutable result passed to event logic:

- `state`: final state candidate.
- `success`: whether the runtime should apply the state change.
- `error`: rejection reason when `success == false`.

New values created with `ContractResult::new(state)` start with `success = false`, so contract logic must explicitly accept successful changes.

### `ContractInitCheck`

Mutable result passed to initialization checks:

- `success`: whether the initial state is valid.
- `error`: rejection reason when `success == false`.

## Examples

The repository includes two non-published example crates:

- [`example`](https://github.com/Averiun-Ledger/ave-contract-sdk/tree/main/example): contract with several event variants and a rejected update path.
- [`example2`](https://github.com/Averiun-Ledger/ave-contract-sdk/tree/main/example2): minimal string update contract.

Run their tests from the repository root:

```bash
cargo test --manifest-path example/Cargo.toml
cargo test --manifest-path example2/Cargo.toml
```

## Development

Run the SDK test suite:

```bash
cargo test
```

Check what would be included in the crates.io package:

```bash
cargo package --list
```

The crate is configured as both `rlib` and `cdylib` so it can be used by Rust tests and by contracts compiled for WASM.

## Publishing Notes

Before publishing a new release, verify:

- `Cargo.toml` has the intended `version`, `repository`, `homepage`, `license`, `keywords`, and `readme` values.
- The README installation snippet matches the crate version being published.
- `cargo test` passes.
- `cargo package --list` contains only files that should be distributed.
- The dependent `ave-common` version is available on crates.io.

## License

This project is a fork of [kore-contract-sdk](https://github.com/kore-ledger/kore-contract-sdk), originally developed by Kore Ledger, SL, modified in 2025 by Averiun Ledger, SL, and distributed under the same AGPL-3.0-only license.