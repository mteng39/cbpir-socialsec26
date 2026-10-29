//! Independent expected K>1 decisions from the frozen realistic plaintext oracle.

use crate::{P0dError, realistic::CaseInput, validate_multi_public_k};
use serde::Serialize;
use serde_json::Value;

pub const RESULT_SCHEMA: &str = "p0d-bgl-kgt1-native-result-v1";
pub const METADATA_SCHEMA: &str = "p0d-bgl-kgt1-native-metadata-v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RealisticKgt1Expected {
    pub candidate_ids: Vec<u64>,
    pub selected_ids: Vec<u64>,
    pub literal_ids: Vec<u64>,
    pub boundary_positions: Vec<usize>,
    pub exact_valid_positions: Vec<usize>,
    pub intrinsic_cancellation_positions: Vec<usize>,
    pub threshold_only_positions: Vec<usize>,
    pub duplicate_candidate_offsets: usize,
    pub valid_windows: usize,
}

fn array<'a>(oracle: &'a Value, name: &'static str) -> Result<&'a [Value], P0dError> {
    oracle
        .get(name)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(P0dError::Request("realistic oracle array missing"))
}

fn u64_values(oracle: &Value, name: &'static str) -> Result<Vec<u64>, P0dError> {
    array(oracle, name)?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .ok_or(P0dError::Request("realistic oracle integer invalid"))
        })
        .collect()
}

fn usize_values(oracle: &Value, name: &'static str) -> Result<Vec<usize>, P0dError> {
    u64_values(oracle, name)?
        .into_iter()
        .map(|value| {
            usize::try_from(value)
                .map_err(|_| P0dError::Request("realistic oracle integer exceeds usize"))
        })
        .collect()
}

fn usize_value(oracle: &Value, name: &'static str) -> Result<usize, P0dError> {
    let value = oracle
        .get(name)
        .and_then(Value::as_u64)
        .ok_or(P0dError::Request("realistic oracle count missing"))?;
    usize::try_from(value).map_err(|_| P0dError::Request("realistic oracle count exceeds usize"))
}

fn strictly_ascending(values: &[u64]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

pub fn expected(case: &CaseInput, public_k: usize) -> Result<RealisticKgt1Expected, P0dError> {
    validate_multi_public_k(public_k)?;
    let oracle = &case.oracle;
    let candidate_ids = u64_values(oracle, "valid_candidate_record_ids")?;
    let literal_ids = u64_values(oracle, "literal_record_ids")?;
    if candidate_ids.contains(&0)
        || literal_ids.contains(&0)
        || !strictly_ascending(&candidate_ids)
        || !strictly_ascending(&literal_ids)
    {
        return Err(P0dError::Request(
            "realistic oracle RecordIds are invalid or unordered",
        ));
    }
    let selected_ids = candidate_ids.iter().copied().take(public_k).collect();
    Ok(RealisticKgt1Expected {
        candidate_ids,
        selected_ids,
        literal_ids,
        boundary_positions: usize_values(oracle, "exact_accepted_boundary_positions")?,
        exact_valid_positions: usize_values(oracle, "exact_accepted_valid_positions")?,
        intrinsic_cancellation_positions: usize_values(oracle, "intrinsic_cancellation_positions")?,
        threshold_only_positions: usize_values(oracle, "threshold_only_positions")?,
        duplicate_candidate_offsets: usize_value(oracle, "duplicate_candidate_offsets")?,
        valid_windows: usize_value(oracle, "valid_windows")?,
    })
}

#[cfg(feature = "native")]
pub fn run_case(pipeline: &mut crate::MultiPipeline, case: &CaseInput) -> serde_json::Value {
    let result = crate::realistic::request(case)
        .and_then(|request| pipeline.run(&request).map_err(|error| error.to_string()));
    match result {
        Ok(run) => serde_json::json!({
            "schema": RESULT_SCHEMA,
            "case": case,
            "run": run,
            "error": null,
        }),
        Err(error) => serde_json::json!({
            "schema": RESULT_SCHEMA,
            "case": case,
            "run": null,
            "error": error,
        }),
    }
}
