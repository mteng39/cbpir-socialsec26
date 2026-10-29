//! Parser and validator for the canonical proposal-sized P0 search fixture.

use std::{collections::HashSet, error::Error, fmt};

use serde::Deserialize;
use sha2::{Digest, Sha256};

const FIXTURE_BYTES: &[u8] = include_bytes!("../../../data/workloads/search_workloads_v1.json");
const EXPECTED_SCHEMA: &str = "p0-search-workloads-v1";
const EXPECTED_AUTHORITY_SHA256: &str =
    "0f1a72d6fca44095cc8a24fd8447387e79f6f5b4e07901391e9b74701f6f4f87";
const EXPECTED_BGL_SHA256: &str =
    "2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496";
const EXPECTED_BGL_LICENSE_SHA256: &str =
    "9ffa6ae259833cdc6e7ed8a0219fec72ef6455fb457b3c44096cb1ac1f0696d4";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalWorkloads {
    pub schema: String,
    pub authority: AuthorityMetadata,
    pub generator: GeneratorMetadata,
    pub domain: WorkloadDomain,
    pub synthetic_cases: Vec<SyntheticCase>,
    pub rfps_probability: RfpsProbabilityWorkload,
    pub bgl: BglWorkload,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityMetadata {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorMetadata {
    pub seed: u64,
    pub algorithm: String,
    pub command: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadDomain {
    pub text_lengths: Vec<usize>,
    pub pattern_lengths: Vec<usize>,
    pub text_len_min: usize,
    pub text_len_max: usize,
    pub pattern_len_min: usize,
    pub pattern_len_max: usize,
    pub symbol_domain: [u16; 2],
    pub rfps_api_randomizer_domain: [u8; 2],
    pub rfps_experiment_randomizer_domain: [u8; 2],
    pub max_single_search_transform_len: usize,
    pub ckks_packed_slot_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyntheticCase {
    pub id: String,
    pub kind: String,
    pub text_len: usize,
    pub pattern_len: usize,
    pub text: Vec<u8>,
    pub pattern: Vec<u8>,
    pub randomizer: Vec<u8>,
    pub expected_match_positions: Vec<usize>,
    pub designated_position: usize,
    pub mismatch_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RfpsProbabilityWorkload {
    pub model: RfpsProbabilityModel,
    pub trials: Vec<RfpsTrial>,
    pub true_match_controls: Vec<RfpsTrueMatchControl>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RfpsProbabilityModel {
    pub alphabet: Vec<u8>,
    pub ordered_distinct_pairs: Vec<[u8; 2]>,
    pub pair_sampling: String,
    pub randomizer_domain: [u8; 2],
    pub randomizer_sampling: String,
    pub independence: String,
    pub conditioning_event: String,
    pub algebraic_domain: String,
    pub score: String,
    pub mismatch_trials_per_k: usize,
    pub true_match_controls_per_k: usize,
    pub true_match_controls_in_probability_denominator: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RfpsTrial {
    pub id: String,
    pub k: usize,
    pub text: Vec<u8>,
    pub pattern: Vec<u8>,
    pub randomizer: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RfpsTrueMatchControl {
    pub id: String,
    pub k: usize,
    pub text: Vec<u8>,
    pub pattern: Vec<u8>,
    pub randomizer: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BglWorkload {
    pub source: BglSource,
    pub cases: Vec<BglCase>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BglSource {
    pub repository: String,
    pub commit: String,
    pub path: String,
    pub sha256: String,
    pub size_bytes: usize,
    pub record_count: usize,
    pub terminal_newline: bool,
    pub license_path: String,
    pub license_sha256: String,
    pub license_size_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BglCase {
    pub id: String,
    pub source_line: usize,
    pub text_len: usize,
    pub pattern_len: usize,
    pub text: Vec<u8>,
    pub pattern: Vec<u8>,
    pub pattern_offset: usize,
    pub randomizer: Vec<u8>,
    pub expected_match_positions: Vec<usize>,
}

#[derive(Debug)]
pub enum WorkloadError {
    Json(serde_json::Error),
    Invalid(String),
}

impl fmt::Display for WorkloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid workload JSON: {error}"),
            Self::Invalid(detail) => write!(formatter, "invalid workload fixture: {detail}"),
        }
    }
}

impl Error for WorkloadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<serde_json::Error> for WorkloadError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Return the SHA-256 digest of the exact bytes compiled into this crate.
#[must_use]
pub fn fixture_sha256() -> String {
    format!("{:x}", Sha256::digest(FIXTURE_BYTES))
}

/// Parse and validate the canonical fixture compiled into this crate.
///
/// # Errors
///
/// Returns [`WorkloadError::Json`] if embedded bytes are not valid JSON or
/// [`WorkloadError::Invalid`] if any metadata, domain, ID, or case invariant
/// differs from the canonical workload contract.
pub fn proposal_workloads() -> Result<ProposalWorkloads, WorkloadError> {
    parse_and_validate(FIXTURE_BYTES)
}

fn invalid(detail: impl Into<String>) -> WorkloadError {
    WorkloadError::Invalid(detail.into())
}

fn direct_positions(text: &[u8], pattern: &[u8]) -> Vec<usize> {
    text.windows(pattern.len())
        .enumerate()
        .filter_map(|(position, window)| (window == pattern).then_some(position))
        .collect()
}

fn validate_search_shape(
    id: &str,
    text_len: usize,
    pattern_len: usize,
    text: &[u8],
    pattern: &[u8],
    randomizer: &[u8],
) -> Result<(), WorkloadError> {
    if text_len != text.len() || pattern_len != pattern.len() {
        return Err(invalid(format!("{id}: declared length differs from bytes")));
    }
    if text.is_empty() || text.len() > 1024 {
        return Err(invalid(format!("{id}: text length outside 1..=1024")));
    }
    if pattern.is_empty() || pattern.len() > 64 || pattern.len() > text.len() {
        return Err(invalid(format!(
            "{id}: pattern length outside supported domain"
        )));
    }
    if randomizer.len() != pattern.len() || randomizer.iter().any(|value| !(3..=32).contains(value))
    {
        return Err(invalid(format!("{id}: invalid experiment randomizer")));
    }
    Ok(())
}

fn validate_metadata(fixture: &ProposalWorkloads) -> Result<(), WorkloadError> {
    if fixture.schema != EXPECTED_SCHEMA {
        return Err(invalid(format!("unexpected schema: {}", fixture.schema)));
    }
    if fixture.authority.path != "docs/proposal/proposal_upd.pdf"
        || fixture.authority.sha256 != EXPECTED_AUTHORITY_SHA256
    {
        return Err(invalid("unexpected proposal authority metadata"));
    }
    if fixture.generator.seed != 1_784_000_001
        || fixture.generator.algorithm != "SplitMix64 with unbiased 64-bit rejection sampling"
        || fixture.generator.command.is_empty()
    {
        return Err(invalid("unexpected generator metadata"));
    }
    let domain = &fixture.domain;
    if domain.text_lengths != [64, 256, 1024]
        || domain.pattern_lengths != [8, 16, 32, 64]
        || (domain.text_len_min, domain.text_len_max) != (1, 1024)
        || (domain.pattern_len_min, domain.pattern_len_max) != (1, 64)
        || domain.symbol_domain != [0, 255]
        || domain.rfps_api_randomizer_domain != [1, 32]
        || domain.rfps_experiment_randomizer_domain != [3, 32]
        || domain.max_single_search_transform_len != 2048
        || domain.ckks_packed_slot_count != 4096
    {
        return Err(invalid("unexpected executable-domain metadata"));
    }
    Ok(())
}

fn validate_synthetic_cases(
    fixture: &ProposalWorkloads,
    ids: &mut HashSet<String>,
) -> Result<(), WorkloadError> {
    if fixture.synthetic_cases.len() != 24 {
        return Err(invalid("synthetic case count must be 24"));
    }
    let mut variants = HashSet::new();
    for case in &fixture.synthetic_cases {
        if !ids.insert(case.id.clone()) {
            return Err(invalid(format!("duplicate ID: {}", case.id)));
        }
        validate_search_shape(
            &case.id,
            case.text_len,
            case.pattern_len,
            &case.text,
            &case.pattern,
            &case.randomizer,
        )?;
        if direct_positions(&case.text, &case.pattern) != case.expected_match_positions {
            return Err(invalid(format!(
                "{}: incorrect expected positions",
                case.id
            )));
        }
        if case.designated_position + case.pattern_len > case.text_len {
            return Err(invalid(format!(
                "{}: designated window is out of range",
                case.id
            )));
        }
        let mismatch_count = case.text
            [case.designated_position..case.designated_position + case.pattern_len]
            .iter()
            .zip(&case.pattern)
            .filter(|(left, right)| left != right)
            .count();
        match case.kind.as_str() {
            "exact_match"
                if case.mismatch_count == 0
                    && mismatch_count == 0
                    && !case.expected_match_positions.is_empty() => {}
            "near_match"
                if (1..=4).contains(&case.mismatch_count)
                    && mismatch_count == case.mismatch_count
                    && case.expected_match_positions.is_empty() => {}
            _ => return Err(invalid(format!("{}: invalid case semantics", case.id))),
        }
        if !variants.insert((case.text_len, case.pattern_len, case.kind.clone())) {
            return Err(invalid(format!("duplicate matrix variant: {}", case.id)));
        }
    }
    for text_len in [64, 256, 1024] {
        for pattern_len in [8, 16, 32, 64] {
            for kind in ["exact_match", "near_match"] {
                if !variants.contains(&(text_len, pattern_len, kind.to_owned())) {
                    return Err(invalid("incomplete synthetic size matrix"));
                }
            }
        }
    }
    Ok(())
}

fn validate_probability_workload(
    fixture: &ProposalWorkloads,
    ids: &mut HashSet<String>,
) -> Result<(), WorkloadError> {
    let workload = &fixture.rfps_probability;
    let model = &workload.model;
    let expected_pairs = vec![[0, 1], [0, 2], [1, 0], [1, 2], [2, 0], [2, 1]];
    if model.alphabet != [0, 1, 2]
        || model.ordered_distinct_pairs != expected_pairs
        || model.pair_sampling != "uniform over ordered distinct pairs"
        || model.randomizer_domain != [3, 32]
        || model.randomizer_sampling != "uniform over inclusive integer domain"
        || model.independence != "coordinates and randomizers are mutually independent draws"
        || model.conditioning_event != "exactly k mismatching coordinates"
        || model.algebraic_domain != "ordinary integers"
        || model.score != "S_k = sum_i (X_i - P_i) R_i"
        || model.mismatch_trials_per_k != 4096
        || model.true_match_controls_per_k != 256
        || model.true_match_controls_in_probability_denominator
    {
        return Err(invalid("unexpected RFPS probability-model metadata"));
    }
    if workload.trials.len() != 16_384 || workload.true_match_controls.len() != 1_024 {
        return Err(invalid("unexpected RFPS trial/control count"));
    }

    let mut trial_counts = [0_usize; 5];
    for trial in &workload.trials {
        if !ids.insert(trial.id.clone()) {
            return Err(invalid(format!("duplicate ID: {}", trial.id)));
        }
        if !(1..=4).contains(&trial.k)
            || trial.text.len() != trial.k
            || trial.pattern.len() != trial.k
            || trial.randomizer.len() != trial.k
            || trial.text.iter().any(|value| *value > 2)
            || trial.pattern.iter().any(|value| *value > 2)
            || trial
                .text
                .iter()
                .zip(&trial.pattern)
                .any(|(left, right)| left == right)
            || trial
                .randomizer
                .iter()
                .any(|value| !(3..=32).contains(value))
        {
            return Err(invalid(format!("{}: invalid mismatch trial", trial.id)));
        }
        trial_counts[trial.k] += 1;
    }
    if trial_counts[1..] != [4096, 4096, 4096, 4096] {
        return Err(invalid("RFPS mismatch trials are not balanced by k"));
    }

    let mut control_counts = [0_usize; 5];
    for control in &workload.true_match_controls {
        if !ids.insert(control.id.clone()) {
            return Err(invalid(format!("duplicate ID: {}", control.id)));
        }
        if !(1..=4).contains(&control.k)
            || control.text.len() != control.k
            || control.pattern.len() != control.k
            || control.randomizer.len() != control.k
            || control.text.iter().any(|value| *value > 2)
            || control.text != control.pattern
            || control
                .randomizer
                .iter()
                .any(|value| !(3..=32).contains(value))
        {
            return Err(invalid(format!(
                "{}: invalid true-match control",
                control.id
            )));
        }
        control_counts[control.k] += 1;
    }
    if control_counts[1..] != [256, 256, 256, 256] {
        return Err(invalid("RFPS true-match controls are not balanced by k"));
    }
    Ok(())
}

fn validate_bgl(
    fixture: &ProposalWorkloads,
    ids: &mut HashSet<String>,
) -> Result<(), WorkloadError> {
    let source = &fixture.bgl.source;
    if source.repository != "https://github.com/logpai/loghub"
        || source.commit != "dd61d0952749ee7963bde24220d1be5ede023033"
        || source.path != "BGL/BGL_2k.log"
        || source.sha256 != EXPECTED_BGL_SHA256
        || source.size_bytes != 317_150
        || source.record_count != 2_000
        || source.terminal_newline
        || source.license_path != "LICENSE"
        || source.license_sha256 != EXPECTED_BGL_LICENSE_SHA256
        || source.license_size_bytes != 553
    {
        return Err(invalid("unexpected BGL source metadata"));
    }
    if fixture.bgl.cases.len() != 4 {
        return Err(invalid("BGL case count must be four"));
    }
    let mut pattern_lengths = Vec::new();
    let mut source_lines = HashSet::new();
    for case in &fixture.bgl.cases {
        if !ids.insert(case.id.clone()) || !source_lines.insert(case.source_line) {
            return Err(invalid(format!(
                "duplicate BGL ID/source line: {}",
                case.id
            )));
        }
        validate_search_shape(
            &case.id,
            case.text_len,
            case.pattern_len,
            &case.text,
            &case.pattern,
            &case.randomizer,
        )?;
        if case.id != format!("bgl_2k_line_{}", case.source_line)
            || !(1..=2_000).contains(&case.source_line)
            || case.pattern_offset + case.pattern_len > case.text_len
            || case.text[case.pattern_offset..case.pattern_offset + case.pattern_len]
                != case.pattern
            || direct_positions(&case.text, &case.pattern) != case.expected_match_positions
        {
            return Err(invalid(format!("{}: invalid BGL case", case.id)));
        }
        pattern_lengths.push(case.pattern_len);
    }
    if pattern_lengths != [8, 16, 32, 64] {
        return Err(invalid("unexpected BGL pattern-length sequence"));
    }
    Ok(())
}

fn parse_and_validate(bytes: &[u8]) -> Result<ProposalWorkloads, WorkloadError> {
    let fixture: ProposalWorkloads = serde_json::from_slice(bytes)?;
    validate_metadata(&fixture)?;
    let mut ids = HashSet::new();
    validate_synthetic_cases(&fixture, &mut ids)?;
    validate_probability_workload(&fixture, &mut ids)?;
    validate_bgl(&fixture, &mut ids)?;
    Ok(fixture)
}

#[cfg(test)]
mod tests {
    use super::{FIXTURE_BYTES, parse_and_validate};

    fn assert_rejected(mutator: impl FnOnce(&mut serde_json::Value)) {
        let mut value: serde_json::Value = serde_json::from_slice(FIXTURE_BYTES).unwrap();
        mutator(&mut value);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(parse_and_validate(&bytes).is_err());
    }

    #[test]
    fn rejects_corrupt_schema_and_semantics() {
        assert_rejected(|value| value["schema"] = "wrong".into());
        assert_rejected(|value| {
            value["synthetic_cases"][1]["id"] = value["synthetic_cases"][0]["id"].clone();
        });
        assert_rejected(|value| value["synthetic_cases"][0]["text_len"] = 63.into());
        assert_rejected(|value| value["synthetic_cases"][0]["randomizer"][0] = 0.into());
        assert_rejected(|value| {
            value["synthetic_cases"][0]["expected_match_positions"] = serde_json::json!([1]);
        });
    }
}
