//! Prometheus counters and histograms for contract execution.

use std::time::Duration;

use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::{counter::Counter, family::Family, histogram::Histogram};
use prometheus_client::registry::Registry;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct ContractExecutionLabels {
    result: &'static str,
}

/// Prometheus metrics recorded by the contract runtime.
pub struct ContractMetrics {
    contract_executions: Family<ContractExecutionLabels, Counter>,
    contract_execution_seconds: Family<ContractExecutionLabels, Histogram, fn() -> Histogram>,
    contract_fuel_consumed: Family<ContractExecutionLabels, Histogram, fn() -> Histogram>,
    contract_fuel_exhausted_total: Family<ContractExecutionLabels, Counter>,
    contract_memory_peak_bytes: Family<ContractExecutionLabels, Histogram, fn() -> Histogram>,
}

impl ContractMetrics {
    /// Creates a metric set with the default histogram buckets.
    pub fn new() -> Self {
        Self {
            contract_executions: Family::default(),
            contract_execution_seconds: Family::new_with_constructor(|| {
                Histogram::new(vec![
                    0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0,
                ])
            }),
            contract_fuel_consumed: Family::new_with_constructor(|| {
                Histogram::new(vec![
                    1_000.0,
                    10_000.0,
                    100_000.0,
                    500_000.0,
                    1_000_000.0,
                    2_500_000.0,
                    5_000_000.0,
                    7_500_000.0,
                    10_000_000.0,
                ])
            }),
            contract_fuel_exhausted_total: Family::default(),
            contract_memory_peak_bytes: Family::new_with_constructor(|| {
                Histogram::new(vec![
                    4_096.0,
                    16_384.0,
                    65_536.0,
                    262_144.0,
                    1_048_576.0,
                    4_194_304.0,
                    16_777_216.0,
                ])
            }),
        }
    }

    /// Registers all metrics into `registry` under the `ave_contract_sdk_` prefix.
    pub fn register_into(&self, registry: &mut Registry) {
        registry.register(
            "ave_contract_sdk_contract_executions",
            "Contract execution attempts labeled by result.",
            self.contract_executions.clone(),
        );
        registry.register(
            "ave_contract_sdk_contract_execution_seconds",
            "Contract execution duration labeled by result.",
            self.contract_execution_seconds.clone(),
        );
        registry.register(
            "ave_contract_sdk_contract_fuel_consumed",
            "Contract fuel consumed per execution labeled by result.",
            self.contract_fuel_consumed.clone(),
        );
        registry.register(
            "ave_contract_sdk_contract_fuel_exhausted",
            "Total number of contract executions that ran out of fuel.",
            self.contract_fuel_exhausted_total.clone(),
        );
        registry.register(
            "ave_contract_sdk_contract_memory_peak_bytes",
            "Peak WASM linear memory used per contract execution labeled by result.",
            self.contract_memory_peak_bytes.clone(),
        );
    }

    /// Records an execution attempt with `result` and its `duration`.
    pub fn observe_contract_execution(&self, result: &'static str, duration: Duration) {
        let labels = ContractExecutionLabels { result };
        self.contract_executions.get_or_create(&labels).inc();
        self.contract_execution_seconds
            .get_or_create(&labels)
            .observe(duration.as_secs_f64());
    }

    /// Records the `fuel` consumed by an execution with `result`.
    pub fn observe_contract_fuel_consumed(&self, result: &'static str, fuel: u64) {
        let labels = ContractExecutionLabels { result };
        self.contract_fuel_consumed
            .get_or_create(&labels)
            .observe(fuel as f64);
    }

    /// Increments the count of executions that ran out of fuel.
    pub fn observe_contract_fuel_exhausted(&self) {
        self.contract_fuel_exhausted_total
            .get_or_create(&ContractExecutionLabels { result: "error" })
            .inc();
    }

    /// Records the peak linear memory `bytes` of an execution with `result`.
    pub fn observe_contract_memory_peak(&self, result: &'static str, bytes: u64) {
        let labels = ContractExecutionLabels { result };
        self.contract_memory_peak_bytes
            .get_or_create(&labels)
            .observe(bytes as f64);
    }
}

impl Default for ContractMetrics {
    fn default() -> Self {
        Self::new()
    }
}
