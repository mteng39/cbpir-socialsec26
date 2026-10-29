//! RFPS-15 server-state and encryption-stability diagnostic.

use cbpir_search::{RfpsProfile, ServerTextCtPtRfpsStaged};
use cbpir::{
    Catalog, LogicalRecord, MatchMode, MatchRequest, RecordId, compare, locate_many_from_scores,
    oracle,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::{self, Read};

const SCHEMA: &str = "cbpir-rfps15-state-diagnostic-v1";
const FRESH_R_SEED: u64 = 0xD1_A6_15_5E_4E_5E_00_01;

#[derive(Clone, Debug, Deserialize)]
struct InputRecord {
    id: u64,
    searchable: Vec<u8>,
    payload: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize)]
struct WorkloadInput {
    ring_dimension: usize,
    text_bytes: usize,
    transform_len: usize,
    records: Vec<InputRecord>,
}

#[derive(Clone, Debug, Deserialize)]
struct TimingFixtureInput {
    pattern: Vec<u8>,
    threshold: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct DiagnosticInput {
    workload: WorkloadInput,
    timing_fixture: TimingFixtureInput,
    repetitions: usize,
    fixed_randomizer: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
struct DiagnosticRow {
    case_id: &'static str,
    iteration: usize,
    randomizer: Vec<u8>,
    literal_match_positions: usize,
    literal_match_record_ids: usize,
    exact_rfps_accepted_positions: usize,
    exact_rfps_candidate_record_ids: usize,
    ckks_accepted_positions: usize,
    candidate_record_ids: usize,
    cancellation_count: usize,
    threshold_only_count: usize,
    ckks_threshold_disagreements: usize,
    max_abs_score_error: f64,
    max_score_drift_from_first: f64,
    max_numerical_residual_drift_from_first: f64,
}

#[derive(Debug, Serialize)]
struct DiagnosticOutput {
    schema: &'static str,
    ring_dimension: usize,
    text_bytes: usize,
    transform_len: usize,
    pattern: Vec<u8>,
    threshold: f64,
    repetitions: usize,
    fixed_randomizer: Vec<u8>,
    rows: Vec<DiagnosticRow>,
}

fn error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    io::Error::other(message.into()).into()
}

fn ckks<T>(
    result: Result<T, cbpir_search::CkksError>,
) -> Result<T, Box<dyn std::error::Error>> {
    result.map_err(|failure| error(format!("CKKS diagnostic failed: {failure:?}")))
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn fresh_randomizer(iteration: usize, length: usize) -> Vec<u8> {
    let mut state = FRESH_R_SEED ^ u64::try_from(iteration).unwrap_or(u64::MAX);
    let rejection_limit = u64::MAX - (u64::MAX % 30);
    (0..length)
        .map(|_| {
            loop {
                let value = splitmix64(&mut state);
                if value < rejection_limit {
                    return u8::try_from(value % 30).unwrap() + 3;
                }
            }
        })
        .collect()
}

fn logical_records(
    records: &[InputRecord],
) -> Result<Vec<LogicalRecord>, Box<dyn std::error::Error>> {
    records
        .iter()
        .map(|record| {
            Ok(LogicalRecord {
                id: RecordId::new(record.id)?,
                searchable: record.searchable.clone(),
                payload: record.payload.clone(),
            })
        })
        .collect()
}

fn evaluate(
    case_id: &'static str,
    iteration: usize,
    randomizer: &[u8],
    catalog: &Catalog,
    pattern: &[u8],
    threshold: f64,
    scores: &[f64],
    first_scores: &mut Option<Vec<f64>>,
    first_residuals: &mut Option<Vec<f64>>,
) -> Result<DiagnosticRow, Box<dyn std::error::Error>> {
    let request = MatchRequest {
        pattern: pattern.to_vec(),
        threshold,
        mode: MatchMode::Rfps {
            randomizer: randomizer.to_vec(),
        },
    };
    let exact = oracle(catalog, &request)?;
    let comparisons = compare(&exact, scores, threshold)?;
    let located = locate_many_from_scores(catalog, &request, scores, 16)?;
    let literal_match_record_ids = comparisons
        .iter()
        .filter(|row| row.semantic_match)
        .map(|row| row.id)
        .collect::<BTreeSet<_>>()
        .len();
    let exact_rfps_candidate_record_ids = comparisons
        .iter()
        .filter(|row| row.exact_accepted)
        .map(|row| row.id)
        .collect::<BTreeSet<_>>()
        .len();
    let residuals = comparisons
        .iter()
        .map(|row| row.approximate_score - f64::from(row.exact_score))
        .collect::<Vec<_>>();
    let direct_scores = comparisons
        .iter()
        .map(|row| row.approximate_score)
        .collect::<Vec<_>>();
    let max_abs_score_error = residuals.iter().copied().map(f64::abs).fold(0.0, f64::max);
    let max_score_drift_from_first = first_scores.as_ref().map_or(0.0, |reference| {
        direct_scores
            .iter()
            .zip(reference)
            .map(|(current, first)| (current - first).abs())
            .fold(0.0, f64::max)
    });
    let max_numerical_residual_drift_from_first =
        first_residuals.as_ref().map_or(0.0, |reference| {
            residuals
                .iter()
                .zip(reference)
                .map(|(current, first)| (current - first).abs())
                .fold(0.0, f64::max)
        });
    if first_scores.is_none() {
        *first_scores = Some(direct_scores);
    }
    if first_residuals.is_none() {
        *first_residuals = Some(residuals);
    }
    Ok(DiagnosticRow {
        case_id,
        iteration,
        randomizer: randomizer.to_vec(),
        literal_match_positions: comparisons.iter().filter(|row| row.semantic_match).count(),
        literal_match_record_ids,
        exact_rfps_accepted_positions: comparisons.iter().filter(|row| row.exact_accepted).count(),
        exact_rfps_candidate_record_ids,
        ckks_accepted_positions: comparisons
            .iter()
            .filter(|row| row.approximate_accepted)
            .count(),
        candidate_record_ids: located.candidates.len(),
        cancellation_count: comparisons
            .iter()
            .filter(|row| row.intrinsic_cancellation)
            .count(),
        threshold_only_count: comparisons.iter().filter(|row| row.threshold_only).count(),
        ckks_threshold_disagreements: comparisons
            .iter()
            .filter(|row| row.ckks_false_positive || row.ckks_false_negative)
            .count(),
        max_abs_score_error,
        max_score_drift_from_first,
        max_numerical_residual_drift_from_first,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("OMP_NUM_THREADS").as_deref() != Ok("1")
        || std::env::var("OMP_DYNAMIC").as_deref() != Ok("FALSE")
    {
        return Err(error(
            "diagnostic requires OMP_NUM_THREADS=1 and OMP_DYNAMIC=FALSE",
        ));
    }
    let mut input_json = String::new();
    io::stdin().read_to_string(&mut input_json)?;
    let input: DiagnosticInput = serde_json::from_str(&input_json)?;
    if input.workload.ring_dimension != 32_768
        || input.workload.transform_len != 16_384
        || input.timing_fixture.pattern.is_empty()
        || input.fixed_randomizer.len() != input.timing_fixture.pattern.len()
        || input.repetitions < 2
    {
        return Err(error("RFPS-15 diagnostic input contract drift"));
    }
    let catalog = Catalog::new(logical_records(&input.workload.records)?)?;
    if catalog.text().len() != input.workload.text_bytes {
        return Err(error("frozen text length drift"));
    }
    let staged = ckks(ServerTextCtPtRfpsStaged::new(
        RfpsProfile::Rfps15,
        input.workload.transform_len,
        input.timing_fixture.threshold,
    ))?;
    let pattern = &input.timing_fixture.pattern;
    let threshold = input.timing_fixture.threshold;
    let fixed = &input.fixed_randomizer;
    let fixed_query = ckks(staged.match_query(catalog.text().len(), pattern, fixed))?;
    let mut rows = Vec::new();

    // A: exact same ciphertext and exact same server state.
    let mut reused_server = ckks(staged.prepare_server(catalog.text()))?;
    let mut first_scores = None;
    let mut first_residuals = None;
    for iteration in 0..input.repetitions {
        let response = ckks(staged.match_response(&mut reused_server, &fixed_query))?;
        let scores =
            ckks(staged.decrypt_idft_scores(&response, catalog.text().len(), pattern.len()))?;
        rows.push(evaluate(
            "same_ciphertext_reused_server",
            iteration,
            fixed,
            &catalog,
            pattern,
            threshold,
            &scores,
            &mut first_scores,
            &mut first_residuals,
        )?);
    }

    // B: exact same ciphertext, but a newly prepared server plaintext each time.
    let mut first_scores = None;
    let mut first_residuals = None;
    for iteration in 0..input.repetitions {
        let mut server = ckks(staged.prepare_server(catalog.text()))?;
        let response = ckks(staged.match_response(&mut server, &fixed_query))?;
        let scores =
            ckks(staged.decrypt_idft_scores(&response, catalog.text().len(), pattern.len()))?;
        rows.push(evaluate(
            "same_ciphertext_fresh_server",
            iteration,
            fixed,
            &catalog,
            pattern,
            threshold,
            &scores,
            &mut first_scores,
            &mut first_residuals,
        )?);
    }

    // C: same R, but both query encryption and server plaintext are fresh.
    let mut first_scores = None;
    let mut first_residuals = None;
    for iteration in 0..input.repetitions {
        let query = ckks(staged.match_query(catalog.text().len(), pattern, fixed))?;
        let mut server = ckks(staged.prepare_server(catalog.text()))?;
        let response = ckks(staged.match_response(&mut server, &query))?;
        let scores =
            ckks(staged.decrypt_idft_scores(&response, catalog.text().len(), pattern.len()))?;
        rows.push(evaluate(
            "fixed_randomizer_fresh_encryption_fresh_server",
            iteration,
            fixed,
            &catalog,
            pattern,
            threshold,
            &scores,
            &mut first_scores,
            &mut first_residuals,
        )?);
    }

    // D: fresh R, fresh encrypted query, and fresh server plaintext.
    let mut first_scores = None;
    let mut first_residuals = None;
    for iteration in 0..input.repetitions {
        let randomizer = fresh_randomizer(iteration, pattern.len());
        let query = ckks(staged.match_query(catalog.text().len(), pattern, &randomizer))?;
        let mut server = ckks(staged.prepare_server(catalog.text()))?;
        let response = ckks(staged.match_response(&mut server, &query))?;
        let scores =
            ckks(staged.decrypt_idft_scores(&response, catalog.text().len(), pattern.len()))?;
        rows.push(evaluate(
            "fresh_randomizer_fresh_encryption_fresh_server",
            iteration,
            &randomizer,
            &catalog,
            pattern,
            threshold,
            &scores,
            &mut first_scores,
            &mut first_residuals,
        )?);
    }

    serde_json::to_writer(
        io::stdout().lock(),
        &DiagnosticOutput {
            schema: SCHEMA,
            ring_dimension: input.workload.ring_dimension,
            text_bytes: input.workload.text_bytes,
            transform_len: input.workload.transform_len,
            pattern: pattern.clone(),
            threshold,
            repetitions: input.repetitions,
            fixed_randomizer: fixed.clone(),
            rows,
        },
    )?;
    Ok(())
}
