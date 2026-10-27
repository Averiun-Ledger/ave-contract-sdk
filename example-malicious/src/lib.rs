//! MALICIOUS EXAMPLE - must NOT build on the node.
//!
//! It tries to read host environment at compile time via `env!()`.
//! Node builds run with a scrubbed environment (`ave-build` allowlist),
//! so this contract fails to compile there even when the outer process
//! exports the variable. Do NOT use as a template for real contracts.
use ave_contract_sdk as sdk;
use serde::{Deserialize, Serialize};

/// Secret smuggled at compile time. Absent from the node build
/// environment, so `env!` fails the build instead of embedding it.
const STOLEN: &str = env!("AVE_SECRET_SENTINEL");

/// Contract state used by the example.
#[derive(Serialize, Deserialize, Clone)]
struct State {
    pub one: u32,
}

#[derive(Serialize, Deserialize)]
enum StateEvent {
    ModOne { data: u32 },
}

#[unsafe(no_mangle)]
pub unsafe fn main_function(
    state_ptr: i32,
    init_state_ptr: i32,
    event_ptr: i32,
    is_owner: i32,
) -> u32 {
    // Reference the stolen value so it is not optimized away in a
    // successful (unsafe) build; the node must reject it earlier.
    let _ = STOLEN.len();
    sdk::execute_contract(state_ptr, init_state_ptr, event_ptr, is_owner, contract_logic)
}

#[unsafe(no_mangle)]
pub unsafe fn init_check_function(state_ptr: i32) -> u32 {
    sdk::check_init_data(state_ptr, init_logic)
}

fn init_logic(_state: &State, contract_result: &mut sdk::ContractInitCheck) {
    contract_result.accept();
}

fn contract_logic(
    context: &sdk::Context<StateEvent>,
    contract_result: &mut sdk::ContractResult<State>,
) {
    let state = &mut contract_result.state;
    match &context.event {
        StateEvent::ModOne { data } => {
            state.one = *data;
        }
    }
    contract_result.accept();
}
