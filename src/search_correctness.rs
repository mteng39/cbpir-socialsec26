use crate::{P0dError, RecordId};
use serde::Serialize;
use std::collections::BTreeSet;

const FORMAL_RFPS_SEED: u64 = 0xCB_91_27_DF_60_A4_13_5B;
const SEARCH_CORRECTNESS_PATTERN_LENGTHS: [usize; 4] = [1, 4, 8, 16];

pub fn validate_search_correctness_pattern_lengths(lengths: &[usize]) -> bool {
    lengths == SEARCH_CORRECTNESS_PATTERN_LENGTHS
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct FormalRfpsSeed {
    pub source_public_k: usize,
    pub source_run: usize,
    pub invocation: u64,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RecordSetMetrics {
    pub true_positive_ids: Vec<RecordId>,
    pub false_negative_ids: Vec<RecordId>,
    pub false_positive_ids: Vec<RecordId>,
    pub tp: usize,
    pub fn_count: usize,
    pub fp_count: usize,
    pub recall: f64,
    pub precision: Option<f64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RfpsRecordAttribution {
    pub exact_semantic_false_positive_ids: Vec<RecordId>,
    pub ckks_false_positive_ids: Vec<RecordId>,
    pub ckks_false_negative_ids: Vec<RecordId>,
}

fn record_set(ids: &[RecordId]) -> BTreeSet<RecordId> {
    ids.iter().copied().collect()
}

fn difference(left: &BTreeSet<RecordId>, right: &BTreeSet<RecordId>) -> Vec<RecordId> {
    left.difference(right).copied().collect()
}

pub fn record_set_metrics(ground_truth: &[RecordId], candidates: &[RecordId]) -> RecordSetMetrics {
    let ground_truth = record_set(ground_truth);
    let candidates = record_set(candidates);
    let true_positive_ids = ground_truth
        .intersection(&candidates)
        .copied()
        .collect::<Vec<_>>();
    let false_negative_ids = difference(&ground_truth, &candidates);
    let false_positive_ids = difference(&candidates, &ground_truth);
    let tp = true_positive_ids.len();
    let fn_count = false_negative_ids.len();
    let fp_count = false_positive_ids.len();
    let recall = if tp + fn_count == 0 {
        1.0
    } else {
        tp as f64 / (tp + fn_count) as f64
    };
    let precision = (tp + fp_count != 0).then(|| tp as f64 / (tp + fp_count) as f64);
    RecordSetMetrics {
        true_positive_ids,
        false_negative_ids,
        false_positive_ids,
        tp,
        fn_count,
        fp_count,
        recall,
        precision,
    }
}

pub fn rfps_record_attribution(
    literal_ids: &[RecordId],
    exact_rfps_ids: &[RecordId],
    ckks_ids: &[RecordId],
) -> RfpsRecordAttribution {
    let literal = record_set(literal_ids);
    let exact = record_set(exact_rfps_ids);
    let ckks = record_set(ckks_ids);
    RfpsRecordAttribution {
        exact_semantic_false_positive_ids: difference(&exact, &literal),
        ckks_false_positive_ids: difference(&ckks, &exact),
        ckks_false_negative_ids: difference(&exact, &ckks),
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

pub fn fresh_rfps_weights_for_seed(mut seed: u64, length: usize) -> Vec<u8> {
    let rejection_limit = u64::MAX - (u64::MAX % 30);
    (0..length)
        .map(|_| {
            loop {
                let value = splitmix64(&mut seed);
                if value < rejection_limit {
                    return u8::try_from(value % 30).unwrap() + 3;
                }
            }
        })
        .collect()
}

pub fn formal_rfps_seed_schedule(ring_dimension: usize) -> Result<Vec<FormalRfpsSeed>, P0dError> {
    let ring = u64::try_from(ring_dimension)
        .map_err(|_| P0dError::Request("ring dimension exceeds u64"))?;
    let mut invocation = 0_u64;
    let mut seeds = Vec::with_capacity(80);
    for source_public_k in [1_usize, 4, 8, 16] {
        invocation += 3;
        for source_run in 0..20 {
            let seed = FORMAL_RFPS_SEED
                ^ ring
                ^ (u64::try_from(source_public_k).unwrap() << 32)
                ^ invocation;
            seeds.push(FormalRfpsSeed {
                source_public_k,
                source_run,
                invocation,
                seed,
            });
            invocation += 1;
        }
    }
    Ok(seeds)
}
