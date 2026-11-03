use crate::{CliConfig, GeneratedWorkload};
use serde::Serialize;
use std::collections::BTreeMap;

pub const INTERACTIVE_SCHEMA: &str = "cbpir-interactive-v1";

#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    #[error("cannot summarize an empty sample")]
    EmptySample,
    #[error("{0}")]
    Execution(String),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DurationSummary {
    pub mean_ns: f64,
    pub median_ns: f64,
    pub standard_deviation_ns: f64,
    pub p95_ns: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ParameterReport {
    pub security: &'static str,
    pub openfhe_version: String,
    pub ckks_ring_dimension: usize,
    pub ckks_complex_slots: usize,
    pub transform_len: usize,
    pub bgv_ring_dimension: usize,
    pub bgv_plaintext_modulus: u64,
    pub record_slots: usize,
    pub retrieval_capacity: usize,
}

impl ParameterReport {
    #[must_use]
    pub fn for_config(config: &CliConfig, openfhe_version: impl Into<String>) -> Self {
        Self {
            security: "HEStd_128_classic",
            openfhe_version: openfhe_version.into(),
            ckks_ring_dimension: config.ckks_ring_dimension,
            ckks_complex_slots: config.ckks_ring_dimension / 2,
            transform_len: config.transform_len,
            bgv_ring_dimension: config.bgv_ring_dimension,
            bgv_plaintext_modulus: 786_433,
            record_slots: config.record_slots,
            retrieval_capacity: config.capacity,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProcedureDurations {
    pub search_query_ns: u64,
    pub search_response_ns: u64,
    pub locate_ns: u64,
    pub fetch_query_ns: u64,
    pub fetch_response_ns: u64,
    pub recover_ns: u64,
    pub search_ns: u64,
    pub fetch_ns: u64,
    pub online_sum_ns: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RawMeasurement {
    pub run_id: usize,
    pub mode: &'static str,
    pub randomizer: Vec<u8>,
    pub candidate_count: usize,
    pub selected_count: usize,
    pub durations: ProcedureDurations,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModeSummary {
    pub search: DurationSummary,
    pub locate: DurationSummary,
    pub fetch: DurationSummary,
    pub online_per_run: DurationSummary,
    pub median_online_per_run_ns: f64,
    pub r1_3_sum_of_stage_medians_ns: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MeasurementsReport {
    pub raw: Vec<RawMeasurement>,
    pub summary: BTreeMap<String, ModeSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SetupMeasurement {
    pub mode: &'static str,
    pub context_generation_ns: u64,
    pub key_generation_ns: u64,
    pub database_preparation_ns: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CommunicationReport {
    pub search_query_ciphertext_bytes: Vec<u64>,
    pub search_query_bytes: u64,
    pub search_response_bytes: u64,
    pub fetch_query_bytes: u64,
    pub fetch_response_bytes: u64,
    pub total_online_bytes: u64,
    pub repeated_sizes_identical: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CorrectnessReport {
    pub candidate_count: usize,
    pub selected_count: usize,
    pub exact_fetch_reconstruction: bool,
    pub selected_windows_exact: bool,
    pub unselected_windows_zero: bool,
    pub unused_tail_zero: bool,
    pub fps_oracle_disagreements: usize,
    pub rfps_threshold_disagreements: usize,
    pub rfps_intrinsic_cancellations: usize,
    pub rfps_threshold_only: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct InteractiveReport {
    pub schema: &'static str,
    pub configuration: CliConfig,
    pub workload: GeneratedWorkload,
    pub parameters: ParameterReport,
    pub setup: Vec<SetupMeasurement>,
    pub measurements: MeasurementsReport,
    pub communication: BTreeMap<String, CommunicationReport>,
    pub correctness: BTreeMap<String, CorrectnessReport>,
}

impl InteractiveReport {
    #[must_use]
    pub fn new(
        configuration: CliConfig,
        workload: GeneratedWorkload,
        parameters: ParameterReport,
    ) -> Self {
        Self {
            schema: INTERACTIVE_SCHEMA,
            configuration,
            workload,
            parameters,
            setup: Vec::new(),
            measurements: MeasurementsReport::default(),
            communication: BTreeMap::new(),
            correctness: BTreeMap::new(),
        }
    }
}

pub fn summarize_ns(samples: &[u64]) -> Result<DurationSummary, ExperimentError> {
    if samples.is_empty() {
        return Err(ExperimentError::EmptySample);
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let len = sorted.len();
    let mean_ns = sorted.iter().map(|value| *value as f64).sum::<f64>() / len as f64;
    let median_ns = if len % 2 == 0 {
        (sorted[len / 2 - 1] as f64 + sorted[len / 2] as f64) / 2.0
    } else {
        sorted[len / 2] as f64
    };
    let variance = sorted
        .iter()
        .map(|value| (*value as f64 - mean_ns).powi(2))
        .sum::<f64>()
        / len as f64;
    let p95_index = (95 * len).div_ceil(100).saturating_sub(1);
    Ok(DurationSummary {
        mean_ns,
        median_ns,
        standard_deviation_ns: variance.sqrt(),
        p95_ns: sorted[p95_index],
    })
}
