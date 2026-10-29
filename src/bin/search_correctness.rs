//! Dedicated pre-First_K Search-correctness runner.

use cbpir::{
    Catalog, LogicalRecord, MatchMode, MatchRequest, RecordId, compare, formal_rfps_seed_schedule,
    fresh_rfps_weights_for_seed, locate_many_from_scores, oracle, record_set_metrics,
    rfps_record_attribution, validate_search_correctness_pattern_lengths,
};
use cbpir_search::{FpsProfile, RfpsProfile, ServerTextCtPtFpsStaged, ServerTextCtPtRfpsStaged};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};

const INPUT_SCHEMA: &str = "cbpir-search-correctness-v2-multipattern-input";
const OUTPUT_SCHEMA: &str = "cbpir-search-correctness-v2-multipattern-output";
const REPETITIONS: usize = 80;
const PUBLIC_KS: [usize; 4] = [1, 4, 8, 16];

#[derive(Clone, Debug, Deserialize)]
struct InputRecord {
    id: u64,
    searchable: Vec<u8>,
    payload: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize)]
struct WorkloadInput {
    ring_dimension: usize,
    n: usize,
    text_bytes: usize,
    transform_len: usize,
    record_slots: usize,
    records: Vec<InputRecord>,
}

#[derive(Clone, Debug, Deserialize)]
struct QueryFixtureInput {
    source_query_id: String,
    pattern: Vec<u8>,
    pattern_escaped: String,
    pattern_hex: String,
    threshold: f64,
    expected_literal_alignment_count: usize,
    expected_literal_record_count: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct RunnerInput {
    schema: String,
    profile: String,
    workload: WorkloadInput,
    query_fixtures: Vec<QueryFixtureInput>,
    modes: Vec<String>,
    public_ks: Vec<usize>,
    repetitions: usize,
}

#[derive(Clone, Debug, Serialize)]
struct CorrectnessRun {
    profile: String,
    ring_dimension: usize,
    n: usize,
    mode: &'static str,
    repetition: usize,
    seed: Option<u64>,
    seed_source_public_k: Option<usize>,
    seed_source_run: Option<usize>,
    seed_invocation: Option<u64>,
    query_id: String,
    pattern: Vec<u8>,
    pattern_escaped: String,
    pattern_hex: String,
    pattern_length: usize,
    threshold: f64,
    randomizer: Vec<u8>,
    literal_alignment_positions: Vec<usize>,
    literal_record_ids: Vec<RecordId>,
    exact_rfps_alignment_positions: Option<Vec<usize>>,
    exact_rfps_record_ids: Option<Vec<RecordId>>,
    ckks_accepted_alignment_positions: Vec<usize>,
    ckks_boundary_rejected_positions: Vec<usize>,
    ckks_candidate_alignment_positions: Vec<usize>,
    ckks_candidate_record_ids: Vec<RecordId>,
    duplicate_alignment_count: usize,
    true_positive_record_ids: Vec<RecordId>,
    false_negative_record_ids: Vec<RecordId>,
    false_positive_record_ids: Vec<RecordId>,
    tp: usize,
    fn_count: usize,
    fp_count: usize,
    recall: f64,
    precision: Option<f64>,
    rfps_exact_semantic_fp_records: Vec<RecordId>,
    rfps_ckks_fp_records: Vec<RecordId>,
    rfps_ckks_fn_records: Vec<RecordId>,
    cancellation_alignment_positions: Vec<usize>,
    threshold_only_alignment_positions: Vec<usize>,
    ckks_false_positive_alignment_positions: Vec<usize>,
    ckks_false_negative_alignment_positions: Vec<usize>,
    k_invariant: bool,
}

#[derive(Debug, Serialize)]
struct RunnerOutput {
    schema: &'static str,
    profile: String,
    ring_dimension: usize,
    n: usize,
    repetitions_per_mode: usize,
    pattern_lengths: Vec<usize>,
    modes: Vec<String>,
    public_ks_checked: Vec<usize>,
    openfhe_version: String,
    fps_runtime: cbpir_search::ServerTextCtPtRuntimeProfile,
    rfps_runtime: cbpir_search::ServerTextCtPtRuntimeProfile,
    runs: Vec<CorrectnessRun>,
}

fn error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    io::Error::other(message.into()).into()
}

fn ckks<T>(result: Result<T, cbpir_search::CkksError>) -> Result<T, Box<dyn std::error::Error>> {
    result.map_err(|failure| error(format!("CKKS stage failed: {failure:?}")))
}

fn profiles(
    ring_dimension: usize,
) -> Result<(FpsProfile, RfpsProfile), Box<dyn std::error::Error>> {
    match ring_dimension {
        8192 => Ok((FpsProfile::Fps13, RfpsProfile::Rfps13)),
        16384 => Ok((FpsProfile::Fps14, RfpsProfile::Rfps14)),
        32768 => Ok((FpsProfile::Fps15, RfpsProfile::Rfps15)),
        _ => Err(error("unsupported formal profile ring dimension")),
    }
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

fn validate_input(input: &RunnerInput) -> Result<(), Box<dyn std::error::Error>> {
    let pattern_lengths = input
        .query_fixtures
        .iter()
        .map(|fixture| fixture.pattern.len())
        .collect::<Vec<_>>();
    if input.schema != INPUT_SCHEMA
        || input.workload.n != input.workload.records.len()
        || input.workload.record_slots != 64
        || input.workload.transform_len != input.workload.ring_dimension / 2
        || input.workload.n * input.workload.record_slots != input.workload.ring_dimension
        || !validate_search_correctness_pattern_lengths(&pattern_lengths)
        || input.query_fixtures.iter().any(|fixture| {
            fixture.pattern.is_empty()
                || fixture.pattern_escaped.is_empty()
                || fixture.pattern_hex.is_empty()
                || fixture.threshold != 0.5
                || fixture.expected_literal_alignment_count == 0
                || fixture.expected_literal_record_count == 0
        })
        || input.repetitions != REPETITIONS
        || input.public_ks != PUBLIC_KS
        || input.modes != ["fps", "rfps"]
    {
        return Err(error("Search-correctness input contract drift"));
    }
    Ok(())
}

fn candidate_signature(trace: &cbpir::MultiLocateTrace) -> Vec<(u64, usize, usize, usize)> {
    trace
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.id.get(),
                candidate.physical_index,
                candidate.global_position,
                candidate.local_offset,
            )
        })
        .collect()
}

fn evaluate_run(
    input: &RunnerInput,
    fixture: &QueryFixtureInput,
    mode: &'static str,
    repetition: usize,
    seed: Option<cbpir::FormalRfpsSeed>,
    randomizer: Vec<u8>,
    catalog: &Catalog,
    scores: &[f64],
) -> Result<CorrectnessRun, Box<dyn std::error::Error>> {
    let request = MatchRequest {
        pattern: fixture.pattern.clone(),
        threshold: fixture.threshold,
        mode: if mode == "fps" {
            MatchMode::Fps
        } else {
            MatchMode::Rfps {
                randomizer: randomizer.clone(),
            }
        },
    };
    let exact = oracle(catalog, &request)?;
    let literal_alignment_count = exact
        .windows
        .iter()
        .filter(|window| window.semantic_match)
        .count();
    if literal_alignment_count != fixture.expected_literal_alignment_count
        || exact.semantic_ids.len() != fixture.expected_literal_record_count
    {
        return Err(error("plaintext-selected query fixture drift"));
    }
    if mode == "fps" && exact.semantic_ids != exact.exact_ids {
        return Err(error("FPS exact oracle differs from literal record IDs"));
    }
    let comparisons = compare(&exact, scores, request.threshold)?;
    let traces = PUBLIC_KS
        .iter()
        .map(|public_k| locate_many_from_scores(catalog, &request, scores, *public_k))
        .collect::<Result<Vec<_>, _>>()?;
    let signature = candidate_signature(&traces[0]);
    let k_invariant = traces
        .iter()
        .all(|trace| candidate_signature(trace) == signature);
    if !k_invariant {
        return Err(error("candidate generation changed with public K"));
    }
    let trace = &traces[0];
    let candidate_ids = trace
        .candidates
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let metrics = record_set_metrics(&exact.semantic_ids, &candidate_ids);
    let attribution =
        rfps_record_attribution(&exact.semantic_ids, &exact.exact_ids, &candidate_ids);
    let positions = |predicate: fn(&cbpir::DecisionComparison) -> bool| {
        comparisons
            .iter()
            .filter(|item| predicate(item))
            .map(|item| item.global_position)
            .collect::<Vec<_>>()
    };
    let seed_value = seed.map(|item| item.seed);
    Ok(CorrectnessRun {
        profile: input.profile.clone(),
        ring_dimension: input.workload.ring_dimension,
        n: input.workload.n,
        mode,
        repetition,
        seed: seed_value,
        seed_source_public_k: seed.map(|item| item.source_public_k),
        seed_source_run: seed.map(|item| item.source_run),
        seed_invocation: seed.map(|item| item.invocation),
        query_id: fixture.source_query_id.clone(),
        pattern: fixture.pattern.clone(),
        pattern_escaped: fixture.pattern_escaped.clone(),
        pattern_hex: fixture.pattern_hex.clone(),
        pattern_length: fixture.pattern.len(),
        threshold: fixture.threshold,
        randomizer,
        literal_alignment_positions: exact
            .windows
            .iter()
            .filter(|window| window.semantic_match)
            .map(|window| window.global_position)
            .collect(),
        literal_record_ids: exact.semantic_ids.clone(),
        exact_rfps_alignment_positions: (mode == "rfps").then(|| {
            comparisons
                .iter()
                .filter(|item| item.exact_accepted)
                .map(|item| item.global_position)
                .collect()
        }),
        exact_rfps_record_ids: (mode == "rfps").then(|| exact.exact_ids.clone()),
        ckks_accepted_alignment_positions: trace.accepted_positions.clone(),
        ckks_boundary_rejected_positions: trace.boundary_rejected_positions.clone(),
        ckks_candidate_alignment_positions: trace
            .candidates
            .iter()
            .map(|candidate| candidate.global_position)
            .collect(),
        ckks_candidate_record_ids: candidate_ids,
        duplicate_alignment_count: trace.duplicate_count,
        true_positive_record_ids: metrics.true_positive_ids,
        false_negative_record_ids: metrics.false_negative_ids,
        false_positive_record_ids: metrics.false_positive_ids,
        tp: metrics.tp,
        fn_count: metrics.fn_count,
        fp_count: metrics.fp_count,
        recall: metrics.recall,
        precision: metrics.precision,
        rfps_exact_semantic_fp_records: if mode == "rfps" {
            attribution.exact_semantic_false_positive_ids
        } else {
            Vec::new()
        },
        rfps_ckks_fp_records: if mode == "rfps" {
            attribution.ckks_false_positive_ids
        } else {
            Vec::new()
        },
        rfps_ckks_fn_records: if mode == "rfps" {
            attribution.ckks_false_negative_ids
        } else {
            Vec::new()
        },
        cancellation_alignment_positions: positions(|item| item.intrinsic_cancellation),
        threshold_only_alignment_positions: positions(|item| item.threshold_only),
        ckks_false_positive_alignment_positions: positions(|item| item.ckks_false_positive),
        ckks_false_negative_alignment_positions: positions(|item| item.ckks_false_negative),
        k_invariant,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("OMP_NUM_THREADS").as_deref() != Ok("1")
        || std::env::var("OMP_DYNAMIC").as_deref() != Ok("FALSE")
    {
        return Err(error("OpenMP environment is not fixed to 1/FALSE"));
    }
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let input: RunnerInput = serde_json::from_str(&raw)?;
    validate_input(&input)?;
    let records = logical_records(&input.workload.records)?;
    let catalog = Catalog::new(records)?;
    if catalog.text().len() != input.workload.text_bytes
        || (catalog.text().len() + 1).next_power_of_two() != input.workload.transform_len
    {
        return Err(error("frozen text/T/L drift"));
    }
    let threshold = input.query_fixtures[0].threshold;
    let (fps_profile, rfps_profile) = profiles(input.workload.ring_dimension)?;
    let fps_context = ckks(ServerTextCtPtFpsStaged::new(
        fps_profile,
        input.workload.transform_len,
        threshold,
    ))?;
    let rfps_context = ckks(ServerTextCtPtRfpsStaged::new(
        rfps_profile,
        input.workload.transform_len,
        threshold,
    ))?;
    let fps_runtime = fps_context.runtime_profile();
    let rfps_runtime = rfps_context.runtime_profile();
    if fps_runtime.ring_dimension as usize != input.workload.ring_dimension
        || rfps_runtime.ring_dimension as usize != input.workload.ring_dimension
        || fps_runtime.batch_size as usize != input.workload.transform_len
        || rfps_runtime.batch_size as usize != input.workload.transform_len
    {
        return Err(error("runtime HE profile drift"));
    }
    let mut fps_server = ckks(fps_context.prepare_server(catalog.text()))?;
    let mut rfps_server = ckks(rfps_context.prepare_server(catalog.text()))?;
    let mut runs = Vec::with_capacity(REPETITIONS * 2 * input.query_fixtures.len());
    for fixture in &input.query_fixtures {
        for repetition in 0..REPETITIONS {
            let query = ckks(fps_context.match_query(catalog.text().len(), &fixture.pattern))?;
            let response = ckks(fps_context.match_response(&mut fps_server, &query))?;
            let scores = ckks(fps_context.decrypt_idft_scores(
                &response,
                catalog.text().len(),
                fixture.pattern.len(),
            ))?;
            runs.push(evaluate_run(
                &input,
                fixture,
                "fps",
                repetition,
                None,
                Vec::new(),
                &catalog,
                &scores,
            )?);
        }
        let seed_schedule = formal_rfps_seed_schedule(input.workload.ring_dimension)?;
        for (repetition, seed) in seed_schedule.into_iter().enumerate() {
            let randomizer = fresh_rfps_weights_for_seed(seed.seed, fixture.pattern.len());
            let query = ckks(rfps_context.match_query(
                catalog.text().len(),
                &fixture.pattern,
                &randomizer,
            ))?;
            let response = ckks(rfps_context.match_response(&mut rfps_server, &query))?;
            let scores = ckks(rfps_context.decrypt_idft_scores(
                &response,
                catalog.text().len(),
                fixture.pattern.len(),
            ))?;
            runs.push(evaluate_run(
                &input,
                fixture,
                "rfps",
                repetition,
                Some(seed),
                randomizer,
                &catalog,
                &scores,
            )?);
        }
    }
    serde_json::to_writer(
        io::stdout().lock(),
        &RunnerOutput {
            schema: OUTPUT_SCHEMA,
            profile: input.profile,
            ring_dimension: input.workload.ring_dimension,
            n: input.workload.n,
            repetitions_per_mode: REPETITIONS,
            pattern_lengths: input
                .query_fixtures
                .iter()
                .map(|fixture| fixture.pattern.len())
                .collect(),
            modes: input.modes,
            public_ks_checked: input.public_ks,
            openfhe_version: cbpir_bgv::BgvCpir::openfhe_version(),
            fps_runtime,
            rfps_runtime,
            runs,
        },
    )?;
    println!();
    Ok(())
}
