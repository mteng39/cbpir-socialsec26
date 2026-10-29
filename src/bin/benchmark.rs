//! Formal seven-procedure CBPIR benchmark runner.

use cbpir::{
    Catalog, FormalFetchContext, FormalFetchDatabase, FormalFetchQuery, FormalFetchRecovered,
    FormalFetchResponse, LogicalRecord, MatchMode, MatchRequest, MultiLocateTrace, RecordId,
    compare, locate_many_from_scores, oracle,
};
use cbpir_bgv::BgvProfile;
use cbpir_search::{
    FpsProfile, RfpsProfile, ServerTextCtPtFpsQuery, ServerTextCtPtFpsServerState,
    ServerTextCtPtFpsStaged, ServerTextCtPtResponse, ServerTextCtPtRfpsQuery,
    ServerTextCtPtRfpsServerState, ServerTextCtPtRfpsStaged,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::{self, Read};
use std::time::Instant;

const SCHEMA: &str = "cbpir-formal-seven-procedure-v1";
const RFPS_SEED: u64 = 0xCB_91_27_DF_60_A4_13_5B;

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
struct TimingFixtureInput {
    source_query_id: String,
    pattern: Vec<u8>,
    pattern_ascii: String,
    threshold: f64,
    expected_candidate_count: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct ProbeInput {
    id: String,
    category: String,
    pattern: Vec<u8>,
    weights: Vec<u8>,
    threshold: f64,
    public_k: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct BenchmarkInput {
    schema: String,
    profile: String,
    workload: WorkloadInput,
    timing_fixture: TimingFixtureInput,
    rfps_probes: Vec<ProbeInput>,
    modes: Vec<String>,
    public_ks: Vec<usize>,
    warmups: usize,
    measured_runs: usize,
}

#[derive(Clone, Debug, Serialize)]
struct SetupSample {
    run_id: usize,
    ring_dimension: usize,
    profile: String,
    mode: &'static str,
    procedure: &'static str,
    duration_ns: u64,
    success: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct ProcedureDurations {
    match_query_ns: u64,
    match_response_ns: u64,
    locate_ns: u64,
    fetch_query_ns: u64,
    fetch_response_ns: u64,
    recover_ns: u64,
    online_sum_ns: u64,
    online_outer_ns: u64,
}

#[derive(Clone, Debug, Serialize)]
struct OnlineSample {
    run_id: usize,
    ring_dimension: usize,
    profile: String,
    mode: &'static str,
    query_id: String,
    pattern: Vec<u8>,
    pattern_ascii: String,
    pattern_length: usize,
    threshold: f64,
    public_k: usize,
    candidate_count: usize,
    realized_k: usize,
    randomizer: Vec<u8>,
    durations: ProcedureDurations,
    success: bool,
}

#[derive(Clone, Debug, Serialize)]
struct CorrectnessRow {
    row_type: &'static str,
    run_id: String,
    ring_dimension: usize,
    profile: String,
    mode: &'static str,
    query_id: String,
    pattern: Vec<u8>,
    pattern_ascii: String,
    pattern_length: usize,
    threshold: f64,
    public_k: usize,
    candidate_count: usize,
    realized_k: usize,
    fps_oracle_mismatches: usize,
    rfps_ckks_threshold_disagreements: usize,
    rfps_cancellation_count: usize,
    rfps_threshold_only_count: usize,
    selected_ids_match_exact_first_k: bool,
    fetch_exact_reconstruction: bool,
    selected_windows_exact: bool,
    unselected_windows_zero: bool,
    unused_tail_zero: bool,
    query_ciphertext_count: usize,
    response_ciphertext_count: usize,
    success: bool,
}

#[derive(Clone, Debug, Serialize)]
struct CommunicationRow {
    ring_dimension: usize,
    profile: String,
    mode: &'static str,
    query_id: String,
    pattern: Vec<u8>,
    pattern_length: usize,
    threshold: f64,
    public_k: usize,
    candidate_count: usize,
    realized_k: usize,
    search_query_ciphertext_0_bytes: u64,
    search_query_ciphertext_1_bytes: u64,
    search_query_bytes: u64,
    search_response_bytes: u64,
    fetch_query_bytes: u64,
    fetch_response_bytes: u64,
    total_online_bytes: u64,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct FetchOnlyDurations {
    fetch_query_ns: u64,
    fetch_response_ns: u64,
    recover_ns: u64,
    fetch_sum_ns: u64,
    fetch_outer_ns: u64,
}

#[derive(Clone, Debug, Serialize)]
struct FetchOnlySample {
    run_id: usize,
    ring_dimension: usize,
    profile: String,
    realized_k: usize,
    selected_ids: Vec<RecordId>,
    durations: FetchOnlyDurations,
    query_ciphertext_count: usize,
    response_ciphertext_count: usize,
    fetch_exact_reconstruction: bool,
    selected_windows_exact: bool,
    unselected_windows_zero: bool,
    unused_tail_zero: bool,
    success: bool,
}

#[derive(Clone, Debug, Serialize)]
struct FetchOnlyCommunication {
    ring_dimension: usize,
    profile: String,
    realized_k: usize,
    fetch_query_bytes: u64,
    fetch_response_bytes: u64,
}

#[derive(Debug, Serialize)]
struct BenchmarkOutput {
    schema: &'static str,
    profile: String,
    ring_dimension: usize,
    warmups: usize,
    measured_runs: usize,
    openfhe_version: String,
    fps_runtime: cbpir_search::ServerTextCtPtRuntimeProfile,
    rfps_runtime: cbpir_search::ServerTextCtPtRuntimeProfile,
    fetch_runtime: cbpir::FormalFetchRuntimeProfile,
    setup_samples: Vec<SetupSample>,
    online_samples: Vec<OnlineSample>,
    communication: Vec<CommunicationRow>,
    correctness: Vec<CorrectnessRow>,
    fetch_only_samples: Vec<FetchOnlySample>,
    fetch_only_communication: Vec<FetchOnlyCommunication>,
}

struct FpsArtifacts {
    query: ServerTextCtPtFpsQuery,
    response: ServerTextCtPtResponse,
    locate: MultiLocateTrace,
    fetch_query: FormalFetchQuery,
    fetch_response: FormalFetchResponse,
    recovered: FormalFetchRecovered,
    scores: Vec<f64>,
    durations: ProcedureDurations,
}

struct RfpsArtifacts {
    query: ServerTextCtPtRfpsQuery,
    response: ServerTextCtPtResponse,
    locate: MultiLocateTrace,
    fetch_query: FormalFetchQuery,
    fetch_response: FormalFetchResponse,
    recovered: FormalFetchRecovered,
    scores: Vec<f64>,
    randomizer: Vec<u8>,
    durations: ProcedureDurations,
}

struct FetchOnlyArtifacts {
    query: FormalFetchQuery,
    response: FormalFetchResponse,
    recovered: FormalFetchRecovered,
    durations: FetchOnlyDurations,
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    io::Error::other(message.into()).into()
}

fn ckks<T>(result: Result<T, cbpir_search::CkksError>) -> Result<T, Box<dyn std::error::Error>> {
    result.map_err(|failure| error(format!("CKKS stage failed: {failure:?}")))
}

fn profiles(
    ring_dimension: usize,
) -> Result<(FpsProfile, RfpsProfile, BgvProfile), Box<dyn std::error::Error>> {
    match ring_dimension {
        8192 => Ok((FpsProfile::Fps13, RfpsProfile::Rfps13, BgvProfile::Bgv13)),
        16384 => Ok((FpsProfile::Fps14, RfpsProfile::Rfps14, BgvProfile::Bgv14)),
        32768 => Ok((FpsProfile::Fps15, RfpsProfile::Rfps15, BgvProfile::Bgv15)),
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

fn validate_input(input: &BenchmarkInput) -> Result<(), Box<dyn std::error::Error>> {
    if input.schema != SCHEMA
        || input.workload.n != input.workload.records.len()
        || input.workload.record_slots != 64
        || input.workload.transform_len != input.workload.ring_dimension / 2
        || input.workload.n * input.workload.record_slots != input.workload.ring_dimension
        || input.timing_fixture.pattern.len() != 1
        || input.timing_fixture.pattern_ascii.len() != 1
        || input.timing_fixture.expected_candidate_count < 16
        || input.measured_runs == 0
    {
        return Err(error("formal input contract drift"));
    }
    if input
        .public_ks
        .iter()
        .any(|value| ![1, 4, 8, 16].contains(value))
        || input
            .modes
            .iter()
            .any(|mode| mode != "fps" && mode != "rfps")
    {
        return Err(error("formal matrix mode/K drift"));
    }
    Ok(())
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn fresh_rfps_weights(mut seed: u64, length: usize) -> Vec<u8> {
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

fn prepare_fps(
    records: Vec<LogicalRecord>,
    match_context: &ServerTextCtPtFpsStaged,
    fetch_context: &FormalFetchContext,
) -> Result<(Catalog, ServerTextCtPtFpsServerState, FormalFetchDatabase), Box<dyn std::error::Error>>
{
    let catalog = Catalog::new(records)?;
    let search = ckks(match_context.prepare_server(catalog.text()))?;
    let fetch = fetch_context.prepare_database(catalog.records())?;
    Ok((catalog, search, fetch))
}

fn prepare_rfps(
    records: Vec<LogicalRecord>,
    match_context: &ServerTextCtPtRfpsStaged,
    fetch_context: &FormalFetchContext,
) -> Result<(Catalog, ServerTextCtPtRfpsServerState, FormalFetchDatabase), Box<dyn std::error::Error>>
{
    let catalog = Catalog::new(records)?;
    let search = ckks(match_context.prepare_server(catalog.text()))?;
    let fetch = fetch_context.prepare_database(catalog.records())?;
    Ok((catalog, search, fetch))
}

fn fps_run(
    match_context: &ServerTextCtPtFpsStaged,
    search: &mut ServerTextCtPtFpsServerState,
    fetch_context: &FormalFetchContext,
    fetch: &mut FormalFetchDatabase,
    catalog: &Catalog,
    pattern: &[u8],
    threshold: f64,
    public_k: usize,
) -> Result<FpsArtifacts, Box<dyn std::error::Error>> {
    let outer = Instant::now();
    let started = Instant::now();
    let query = ckks(match_context.match_query(catalog.text().len(), pattern))?;
    let match_query_ns = elapsed_ns(started);
    let started = Instant::now();
    let response = ckks(match_context.match_response(search, &query))?;
    let match_response_ns = elapsed_ns(started);
    let request = MatchRequest {
        pattern: pattern.to_vec(),
        threshold,
        mode: MatchMode::Fps,
    };
    let started = Instant::now();
    let scores =
        ckks(match_context.decrypt_idft_scores(&response, catalog.text().len(), pattern.len()))?;
    let locate = locate_many_from_scores(catalog, &request, &scores, public_k)?;
    let locate_ns = elapsed_ns(started);
    let started = Instant::now();
    let selected_ids = locate
        .selected_candidates
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let fetch_query = fetch_context.fetch_query(fetch, &selected_ids)?;
    let fetch_query_ns = elapsed_ns(started);
    let started = Instant::now();
    let fetch_response = fetch_context.fetch_response(fetch, &fetch_query);
    let fetch_response_ns = elapsed_ns(started);
    let started = Instant::now();
    let recovered = fetch_context.recover(fetch, &fetch_query, &fetch_response)?;
    let recover_ns = elapsed_ns(started);
    let online_outer_ns = elapsed_ns(outer);
    let online_sum_ns = match_query_ns
        .saturating_add(match_response_ns)
        .saturating_add(locate_ns)
        .saturating_add(fetch_query_ns)
        .saturating_add(fetch_response_ns)
        .saturating_add(recover_ns);
    Ok(FpsArtifacts {
        query,
        response,
        locate,
        fetch_query,
        fetch_response,
        recovered,
        scores,
        durations: ProcedureDurations {
            match_query_ns,
            match_response_ns,
            locate_ns,
            fetch_query_ns,
            fetch_response_ns,
            recover_ns,
            online_sum_ns,
            online_outer_ns,
        },
    })
}

fn rfps_run(
    match_context: &ServerTextCtPtRfpsStaged,
    search: &mut ServerTextCtPtRfpsServerState,
    fetch_context: &FormalFetchContext,
    fetch: &mut FormalFetchDatabase,
    catalog: &Catalog,
    pattern: &[u8],
    threshold: f64,
    public_k: usize,
    seed: u64,
) -> Result<RfpsArtifacts, Box<dyn std::error::Error>> {
    let outer = Instant::now();
    let started = Instant::now();
    let randomizer = fresh_rfps_weights(seed, pattern.len());
    let query = ckks(match_context.match_query(catalog.text().len(), pattern, &randomizer))?;
    let match_query_ns = elapsed_ns(started);
    let started = Instant::now();
    let response = ckks(match_context.match_response(search, &query))?;
    let match_response_ns = elapsed_ns(started);
    let request = MatchRequest {
        pattern: pattern.to_vec(),
        threshold,
        mode: MatchMode::Rfps {
            randomizer: randomizer.clone(),
        },
    };
    let started = Instant::now();
    let scores =
        ckks(match_context.decrypt_idft_scores(&response, catalog.text().len(), pattern.len()))?;
    let locate = locate_many_from_scores(catalog, &request, &scores, public_k)?;
    let locate_ns = elapsed_ns(started);
    let started = Instant::now();
    let selected_ids = locate
        .selected_candidates
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let fetch_query = fetch_context.fetch_query(fetch, &selected_ids)?;
    let fetch_query_ns = elapsed_ns(started);
    let started = Instant::now();
    let fetch_response = fetch_context.fetch_response(fetch, &fetch_query);
    let fetch_response_ns = elapsed_ns(started);
    let started = Instant::now();
    let recovered = fetch_context.recover(fetch, &fetch_query, &fetch_response)?;
    let recover_ns = elapsed_ns(started);
    let online_outer_ns = elapsed_ns(outer);
    let online_sum_ns = match_query_ns
        .saturating_add(match_response_ns)
        .saturating_add(locate_ns)
        .saturating_add(fetch_query_ns)
        .saturating_add(fetch_response_ns)
        .saturating_add(recover_ns);
    Ok(RfpsArtifacts {
        query,
        response,
        locate,
        fetch_query,
        fetch_response,
        recovered,
        scores,
        randomizer,
        durations: ProcedureDurations {
            match_query_ns,
            match_response_ns,
            locate_ns,
            fetch_query_ns,
            fetch_response_ns,
            recover_ns,
            online_sum_ns,
            online_outer_ns,
        },
    })
}

fn fetch_only_run(
    fetch_context: &FormalFetchContext,
    fetch: &mut FormalFetchDatabase,
    selected_ids: &[RecordId],
) -> Result<FetchOnlyArtifacts, Box<dyn std::error::Error>> {
    let outer = Instant::now();
    let started = Instant::now();
    let query = fetch_context.fetch_query(fetch, selected_ids)?;
    let fetch_query_ns = elapsed_ns(started);
    let started = Instant::now();
    let response = fetch_context.fetch_response(fetch, &query);
    let fetch_response_ns = elapsed_ns(started);
    let started = Instant::now();
    let recovered = fetch_context.recover(fetch, &query, &response)?;
    let recover_ns = elapsed_ns(started);
    let fetch_outer_ns = elapsed_ns(outer);
    Ok(FetchOnlyArtifacts {
        query,
        response,
        recovered,
        durations: FetchOnlyDurations {
            fetch_query_ns,
            fetch_response_ns,
            recover_ns,
            fetch_sum_ns: fetch_query_ns
                .saturating_add(fetch_response_ns)
                .saturating_add(recover_ns),
            fetch_outer_ns,
        },
    })
}

fn fetch_only_correctness(
    ring_dimension: usize,
    profile: &str,
    run_id: usize,
    selected_ids: &[RecordId],
    fetch: &FormalFetchDatabase,
    run: &FetchOnlyArtifacts,
) -> FetchOnlySample {
    let fetch_exact_reconstruction = run.recovered.records.len() == selected_ids.len()
        && run
            .recovered
            .records
            .iter()
            .zip(selected_ids)
            .all(|(record, id)| record.id == *id);
    let selected_indices = run
        .query
        .selected_indices()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut selected_windows_exact = true;
    let mut unselected_windows_zero = true;
    for index in 0..fetch.capacity() {
        let start = index * fetch.record_slots();
        let end = start + fetch.record_slots();
        if selected_indices.contains(&index) {
            selected_windows_exact &= run.recovered.slots[start..end]
                == fetch.packed_records()[index]
                    .iter()
                    .copied()
                    .map(u64::from)
                    .collect::<Vec<_>>();
        } else {
            unselected_windows_zero &= run.recovered.slots[start..end]
                .iter()
                .all(|value| *value == 0);
        }
    }
    let used = fetch.capacity() * fetch.record_slots();
    let unused_tail_zero = run.recovered.slots[used..].iter().all(|value| *value == 0);
    let query_ciphertext_count = run.query.ciphertext_count();
    let response_ciphertext_count = run.response.ciphertext_count();
    let success = fetch_exact_reconstruction
        && selected_windows_exact
        && unselected_windows_zero
        && unused_tail_zero
        && query_ciphertext_count == 1
        && response_ciphertext_count == 1;
    FetchOnlySample {
        run_id,
        ring_dimension,
        profile: profile.to_owned(),
        realized_k: selected_ids.len(),
        selected_ids: selected_ids.to_vec(),
        durations: run.durations,
        query_ciphertext_count,
        response_ciphertext_count,
        fetch_exact_reconstruction,
        selected_windows_exact,
        unselected_windows_zero,
        unused_tail_zero,
        success,
    }
}

fn correctness(
    row_type: &'static str,
    run_id: String,
    ring_dimension: usize,
    profile: &str,
    mode: &'static str,
    query_id: &str,
    pattern: &[u8],
    pattern_ascii: &str,
    threshold: f64,
    public_k: usize,
    randomizer: &[u8],
    catalog: &Catalog,
    fetch: &FormalFetchDatabase,
    locate: &MultiLocateTrace,
    recovered: &FormalFetchRecovered,
    scores: &[f64],
    query_ciphertext_count: usize,
    response_ciphertext_count: usize,
) -> Result<CorrectnessRow, Box<dyn std::error::Error>> {
    let request = MatchRequest {
        pattern: pattern.to_vec(),
        threshold,
        mode: if mode == "fps" {
            MatchMode::Fps
        } else {
            MatchMode::Rfps {
                randomizer: randomizer.to_vec(),
            }
        },
    };
    let exact = oracle(catalog, &request)?;
    let comparisons = compare(&exact, scores, threshold)?;
    let ckks_disagreements = comparisons
        .iter()
        .filter(|item| item.ckks_false_positive || item.ckks_false_negative)
        .count();
    let cancellation_count = comparisons
        .iter()
        .filter(|item| item.intrinsic_cancellation)
        .count();
    let threshold_only_count = comparisons
        .iter()
        .filter(|item| item.threshold_only)
        .count();
    let selected_ids = locate
        .selected_candidates
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let exact_first_k = exact
        .exact_ids
        .iter()
        .copied()
        .take(public_k)
        .collect::<Vec<_>>();
    let selected_ids_match_exact_first_k = selected_ids == exact_first_k;
    let fetch_exact_reconstruction = recovered.records.len() == selected_ids.len()
        && recovered
            .records
            .iter()
            .zip(&selected_ids)
            .all(|(output, id)| {
                catalog
                    .records()
                    .iter()
                    .find(|record| record.id == *id)
                    .is_some_and(|record| output.id == *id && output.payload == record.payload)
            });
    let selected_indices = locate
        .selected_candidates
        .iter()
        .map(|candidate| candidate.physical_index)
        .collect::<BTreeSet<_>>();
    let mut selected_windows_exact = true;
    let mut unselected_windows_zero = true;
    for index in 0..fetch.capacity() {
        let start = index * fetch.record_slots();
        let end = start + fetch.record_slots();
        if selected_indices.contains(&index) {
            selected_windows_exact &= recovered.slots[start..end]
                == fetch.packed_records()[index]
                    .iter()
                    .copied()
                    .map(u64::from)
                    .collect::<Vec<_>>();
        } else {
            unselected_windows_zero &= recovered.slots[start..end].iter().all(|value| *value == 0);
        }
    }
    let used = fetch.capacity() * fetch.record_slots();
    let unused_tail_zero = recovered.slots[used..].iter().all(|value| *value == 0);
    let fps_oracle_mismatches = if mode == "fps" { ckks_disagreements } else { 0 };
    let rfps_ckks_threshold_disagreements = if mode == "rfps" {
        ckks_disagreements
    } else {
        0
    };
    let strict_match_pass =
        mode == "rfps" || (fps_oracle_mismatches == 0 && selected_ids_match_exact_first_k);
    let success = strict_match_pass
        && fetch_exact_reconstruction
        && selected_windows_exact
        && unselected_windows_zero
        && unused_tail_zero
        && query_ciphertext_count == 1
        && response_ciphertext_count == 1;
    Ok(CorrectnessRow {
        row_type,
        run_id,
        ring_dimension,
        profile: profile.to_owned(),
        mode,
        query_id: query_id.to_owned(),
        pattern: pattern.to_vec(),
        pattern_ascii: pattern_ascii.to_owned(),
        pattern_length: pattern.len(),
        threshold,
        public_k,
        candidate_count: locate.candidates.len(),
        realized_k: locate.selected_candidates.len(),
        fps_oracle_mismatches,
        rfps_ckks_threshold_disagreements,
        rfps_cancellation_count: if mode == "rfps" {
            cancellation_count
        } else {
            0
        },
        rfps_threshold_only_count: if mode == "rfps" {
            threshold_only_count
        } else {
            0
        },
        selected_ids_match_exact_first_k,
        fetch_exact_reconstruction,
        selected_windows_exact,
        unselected_windows_zero,
        unused_tail_zero,
        query_ciphertext_count,
        response_ciphertext_count,
        success,
    })
}

fn communication_row(
    ring_dimension: usize,
    profile: &str,
    mode: &'static str,
    fixture: &TimingFixtureInput,
    public_k: usize,
    candidate_count: usize,
    realized_k: usize,
    search: cbpir_search::ServerTextCtPtMessageSizes,
    fetch: cbpir::FormalFetchMessageSizes,
) -> CommunicationRow {
    let search_query_bytes = search.encrypted_query_ciphertext_bytes.iter().sum::<u64>();
    let total_online_bytes = search_query_bytes
        + search.encrypted_response_ciphertext_bytes
        + fetch.query_bytes
        + fetch.response_bytes;
    CommunicationRow {
        ring_dimension,
        profile: profile.to_owned(),
        mode,
        query_id: fixture.source_query_id.clone(),
        pattern: fixture.pattern.clone(),
        pattern_length: fixture.pattern.len(),
        threshold: fixture.threshold,
        public_k,
        candidate_count,
        realized_k,
        search_query_ciphertext_0_bytes: search.encrypted_query_ciphertext_bytes[0],
        search_query_ciphertext_1_bytes: search.encrypted_query_ciphertext_bytes[1],
        search_query_bytes,
        search_response_bytes: search.encrypted_response_ciphertext_bytes,
        fetch_query_bytes: fetch.query_bytes,
        fetch_response_bytes: fetch.response_bytes,
        total_online_bytes,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("OMP_NUM_THREADS").as_deref() != Ok("1")
        || std::env::var("OMP_DYNAMIC").as_deref() != Ok("FALSE")
    {
        return Err(error("formal OpenMP environment is not fixed to 1/FALSE"));
    }
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let input: BenchmarkInput = serde_json::from_str(&raw)?;
    validate_input(&input)?;
    let records = logical_records(&input.workload.records)?;
    let catalog_check = Catalog::new(records.clone())?;
    if catalog_check.text().len() != input.workload.text_bytes
        || (catalog_check.text().len() + 1).next_power_of_two() != input.workload.transform_len
    {
        return Err(error("frozen text/T/L drift"));
    }
    let (fps_profile, rfps_profile, bgv_profile) = profiles(input.workload.ring_dimension)?;

    // Context and key generation are complete before any Setup timer starts.
    let fps_context = ckks(ServerTextCtPtFpsStaged::new(
        fps_profile,
        input.workload.transform_len,
        input.timing_fixture.threshold,
    ))?;
    let rfps_context = ckks(ServerTextCtPtRfpsStaged::new(
        rfps_profile,
        input.workload.transform_len,
        input.timing_fixture.threshold,
    ))?;
    let fetch_context = FormalFetchContext::new(bgv_profile, input.workload.record_slots)?;
    let fps_runtime = fps_context.runtime_profile();
    let rfps_runtime = rfps_context.runtime_profile();
    let fetch_runtime = fetch_context.runtime_profile().clone();
    if usize::try_from(fps_runtime.ring_dimension)? != input.workload.ring_dimension
        || usize::try_from(rfps_runtime.ring_dimension)? != input.workload.ring_dimension
        || fetch_runtime.actual_ring_dimension != input.workload.ring_dimension
        || fps_runtime.batch_size as usize != input.workload.transform_len
        || rfps_runtime.batch_size as usize != input.workload.transform_len
        || fetch_runtime.plaintext_modulus != 65_537
        || fetch_runtime.multiplicative_depth != 1
    {
        return Err(error("runtime HE profile drift"));
    }

    let mut setup_samples = Vec::new();
    let mut fps_state = None;
    let mut rfps_state = None;
    if input.modes.iter().any(|mode| mode == "fps") {
        for _ in 0..input.warmups {
            let setup_records = records.clone();
            let _ = prepare_fps(setup_records, &fps_context, &fetch_context)?;
        }
        for run_id in 0..input.measured_runs {
            let setup_records = records.clone();
            let started = Instant::now();
            let state = prepare_fps(setup_records, &fps_context, &fetch_context)?;
            let duration_ns = elapsed_ns(started);
            setup_samples.push(SetupSample {
                run_id,
                ring_dimension: input.workload.ring_dimension,
                profile: input.profile.clone(),
                mode: "fps",
                procedure: "Setup",
                duration_ns,
                success: true,
            });
            fps_state = Some(state);
        }
    }
    if input.modes.iter().any(|mode| mode == "rfps") {
        for _ in 0..input.warmups {
            let setup_records = records.clone();
            let _ = prepare_rfps(setup_records, &rfps_context, &fetch_context)?;
        }
        for run_id in 0..input.measured_runs {
            let setup_records = records.clone();
            let started = Instant::now();
            let state = prepare_rfps(setup_records, &rfps_context, &fetch_context)?;
            let duration_ns = elapsed_ns(started);
            setup_samples.push(SetupSample {
                run_id,
                ring_dimension: input.workload.ring_dimension,
                profile: input.profile.clone(),
                mode: "rfps",
                procedure: "Setup",
                duration_ns,
                success: true,
            });
            rfps_state = Some(state);
        }
    }

    let artifacts = tempfile::tempdir()?;
    let mut online_samples = Vec::new();
    let mut communication = Vec::new();
    let mut correctness_rows = Vec::new();
    let mut fetch_only_samples = Vec::new();
    let mut fetch_only_communication = Vec::new();

    if let Some((catalog, mut search, mut fetch)) = fps_state {
        for &public_k in &input.public_ks {
            for _ in 0..input.warmups {
                let warmup = fps_run(
                    &fps_context,
                    &mut search,
                    &fetch_context,
                    &mut fetch,
                    &catalog,
                    &input.timing_fixture.pattern,
                    input.timing_fixture.threshold,
                    public_k,
                )?;
                let check = correctness(
                    "warmup",
                    "warmup".to_owned(),
                    input.workload.ring_dimension,
                    &input.profile,
                    "fps",
                    &input.timing_fixture.source_query_id,
                    &input.timing_fixture.pattern,
                    &input.timing_fixture.pattern_ascii,
                    input.timing_fixture.threshold,
                    public_k,
                    &[],
                    &catalog,
                    &fetch,
                    &warmup.locate,
                    &warmup.recovered,
                    &warmup.scores,
                    warmup.fetch_query.ciphertext_count(),
                    warmup.fetch_response.ciphertext_count(),
                )?;
                if !check.success {
                    return Err(error("FPS warmup correctness failure"));
                }
            }
            for run_id in 0..input.measured_runs {
                let run = fps_run(
                    &fps_context,
                    &mut search,
                    &fetch_context,
                    &mut fetch,
                    &catalog,
                    &input.timing_fixture.pattern,
                    input.timing_fixture.threshold,
                    public_k,
                )?;
                let check = correctness(
                    "measured",
                    run_id.to_string(),
                    input.workload.ring_dimension,
                    &input.profile,
                    "fps",
                    &input.timing_fixture.source_query_id,
                    &input.timing_fixture.pattern,
                    &input.timing_fixture.pattern_ascii,
                    input.timing_fixture.threshold,
                    public_k,
                    &[],
                    &catalog,
                    &fetch,
                    &run.locate,
                    &run.recovered,
                    &run.scores,
                    run.fetch_query.ciphertext_count(),
                    run.fetch_response.ciphertext_count(),
                )?;
                let success = check.success;
                if run_id + 1 == input.measured_runs {
                    let directory = artifacts.path().join(format!("fps-k{public_k}"));
                    let search_sizes = ckks(fps_context.serialize_messages(
                        &directory.join("match"),
                        &run.query,
                        &run.response,
                    ))?;
                    let fetch_sizes = fetch_context.serialize_messages(
                        &directory.join("fetch"),
                        &run.fetch_query,
                        &run.fetch_response,
                    )?;
                    communication.push(communication_row(
                        input.workload.ring_dimension,
                        &input.profile,
                        "fps",
                        &input.timing_fixture,
                        public_k,
                        run.locate.candidates.len(),
                        run.locate.selected_candidates.len(),
                        search_sizes,
                        fetch_sizes,
                    ));
                }
                online_samples.push(OnlineSample {
                    run_id,
                    ring_dimension: input.workload.ring_dimension,
                    profile: input.profile.clone(),
                    mode: "fps",
                    query_id: input.timing_fixture.source_query_id.clone(),
                    pattern: input.timing_fixture.pattern.clone(),
                    pattern_ascii: input.timing_fixture.pattern_ascii.clone(),
                    pattern_length: input.timing_fixture.pattern.len(),
                    threshold: input.timing_fixture.threshold,
                    public_k,
                    candidate_count: run.locate.candidates.len(),
                    realized_k: run.locate.selected_candidates.len(),
                    randomizer: Vec::new(),
                    durations: run.durations,
                    success,
                });
                correctness_rows.push(check);
            }
        }
    }

    if let Some((catalog, mut search, mut fetch)) = rfps_state {
        let mut invocation = 0_u64;
        for &public_k in &input.public_ks {
            for _ in 0..input.warmups {
                let seed = RFPS_SEED
                    ^ u64::try_from(input.workload.ring_dimension)?
                    ^ (u64::try_from(public_k)? << 32)
                    ^ invocation;
                invocation += 1;
                let warmup = rfps_run(
                    &rfps_context,
                    &mut search,
                    &fetch_context,
                    &mut fetch,
                    &catalog,
                    &input.timing_fixture.pattern,
                    input.timing_fixture.threshold,
                    public_k,
                    seed,
                )?;
                let check = correctness(
                    "warmup",
                    "warmup".to_owned(),
                    input.workload.ring_dimension,
                    &input.profile,
                    "rfps",
                    &input.timing_fixture.source_query_id,
                    &input.timing_fixture.pattern,
                    &input.timing_fixture.pattern_ascii,
                    input.timing_fixture.threshold,
                    public_k,
                    &warmup.randomizer,
                    &catalog,
                    &fetch,
                    &warmup.locate,
                    &warmup.recovered,
                    &warmup.scores,
                    warmup.fetch_query.ciphertext_count(),
                    warmup.fetch_response.ciphertext_count(),
                )?;
                if !check.success {
                    return Err(error("RFPS warmup Fetch correctness failure"));
                }
            }
            for run_id in 0..input.measured_runs {
                let seed = RFPS_SEED
                    ^ u64::try_from(input.workload.ring_dimension)?
                    ^ (u64::try_from(public_k)? << 32)
                    ^ invocation;
                invocation += 1;
                let run = rfps_run(
                    &rfps_context,
                    &mut search,
                    &fetch_context,
                    &mut fetch,
                    &catalog,
                    &input.timing_fixture.pattern,
                    input.timing_fixture.threshold,
                    public_k,
                    seed,
                )?;
                let check = correctness(
                    "measured",
                    run_id.to_string(),
                    input.workload.ring_dimension,
                    &input.profile,
                    "rfps",
                    &input.timing_fixture.source_query_id,
                    &input.timing_fixture.pattern,
                    &input.timing_fixture.pattern_ascii,
                    input.timing_fixture.threshold,
                    public_k,
                    &run.randomizer,
                    &catalog,
                    &fetch,
                    &run.locate,
                    &run.recovered,
                    &run.scores,
                    run.fetch_query.ciphertext_count(),
                    run.fetch_response.ciphertext_count(),
                )?;
                let success = check.success;
                if run_id + 1 == input.measured_runs {
                    let directory = artifacts.path().join(format!("rfps-k{public_k}"));
                    let search_sizes = ckks(rfps_context.serialize_messages(
                        &directory.join("match"),
                        &run.query,
                        &run.response,
                    ))?;
                    let fetch_sizes = fetch_context.serialize_messages(
                        &directory.join("fetch"),
                        &run.fetch_query,
                        &run.fetch_response,
                    )?;
                    communication.push(communication_row(
                        input.workload.ring_dimension,
                        &input.profile,
                        "rfps",
                        &input.timing_fixture,
                        public_k,
                        run.locate.candidates.len(),
                        run.locate.selected_candidates.len(),
                        search_sizes,
                        fetch_sizes,
                    ));
                }
                online_samples.push(OnlineSample {
                    run_id,
                    ring_dimension: input.workload.ring_dimension,
                    profile: input.profile.clone(),
                    mode: "rfps",
                    query_id: input.timing_fixture.source_query_id.clone(),
                    pattern: input.timing_fixture.pattern.clone(),
                    pattern_ascii: input.timing_fixture.pattern_ascii.clone(),
                    pattern_length: input.timing_fixture.pattern.len(),
                    threshold: input.timing_fixture.threshold,
                    public_k,
                    candidate_count: run.locate.candidates.len(),
                    realized_k: run.locate.selected_candidates.len(),
                    randomizer: run.randomizer,
                    durations: run.durations,
                    success,
                });
                correctness_rows.push(check);
            }
        }

        // Untimed frozen RFPS evidence probes preserve negative classifications.
        for probe in &input.rfps_probes {
            let query = ckks(rfps_context.match_query(
                catalog.text().len(),
                &probe.pattern,
                &probe.weights,
            ))?;
            let response = ckks(rfps_context.match_response(&mut search, &query))?;
            let scores = ckks(rfps_context.decrypt_idft_scores(
                &response,
                catalog.text().len(),
                probe.pattern.len(),
            ))?;
            let request = MatchRequest {
                pattern: probe.pattern.clone(),
                threshold: probe.threshold,
                mode: MatchMode::Rfps {
                    randomizer: probe.weights.clone(),
                },
            };
            let locate = locate_many_from_scores(&catalog, &request, &scores, probe.public_k)?;
            let selected_ids = locate
                .selected_candidates
                .iter()
                .map(|candidate| candidate.id)
                .collect::<Vec<_>>();
            let fetch_query = fetch_context.fetch_query(&fetch, &selected_ids)?;
            let fetch_response = fetch_context.fetch_response(&mut fetch, &fetch_query);
            let recovered = fetch_context.recover(&fetch, &fetch_query, &fetch_response)?;
            correctness_rows.push(correctness(
                "rfps_preservation_probe",
                "probe".to_owned(),
                input.workload.ring_dimension,
                &input.profile,
                "rfps",
                &probe.id,
                &probe.pattern,
                &String::from_utf8_lossy(&probe.pattern),
                probe.threshold,
                probe.public_k,
                &probe.weights,
                &catalog,
                &fetch,
                &locate,
                &recovered,
                &scores,
                fetch_query.ciphertext_count(),
                fetch_response.ciphertext_count(),
            )?);
            let _ = &probe.category;
        }
    }

    // Controlled Fetch-only rows provide deterministic k scaling when a
    // full-pipeline RFPS decision produces fewer than K candidates.
    let control_catalog = Catalog::new(records.clone())?;
    let mut control_fetch = fetch_context.prepare_database(control_catalog.records())?;
    for realized_k in [1_usize, 4, 8, 16] {
        let selected_ids = control_catalog
            .records()
            .iter()
            .take(realized_k)
            .map(|record| record.id)
            .collect::<Vec<_>>();
        for _ in 0..input.warmups {
            let warmup = fetch_only_run(&fetch_context, &mut control_fetch, &selected_ids)?;
            let check = fetch_only_correctness(
                input.workload.ring_dimension,
                &input.profile,
                usize::MAX,
                &selected_ids,
                &control_fetch,
                &warmup,
            );
            if !check.success {
                return Err(error("controlled Fetch-only warmup correctness failure"));
            }
        }
        for run_id in 0..input.measured_runs {
            let run = fetch_only_run(&fetch_context, &mut control_fetch, &selected_ids)?;
            let check = fetch_only_correctness(
                input.workload.ring_dimension,
                &input.profile,
                run_id,
                &selected_ids,
                &control_fetch,
                &run,
            );
            if !check.success {
                return Err(error("controlled Fetch-only measured correctness failure"));
            }
            if run_id + 1 == input.measured_runs {
                let directory = artifacts.path().join(format!("fetch-only-k{realized_k}"));
                let sizes =
                    fetch_context.serialize_messages(&directory, &run.query, &run.response)?;
                fetch_only_communication.push(FetchOnlyCommunication {
                    ring_dimension: input.workload.ring_dimension,
                    profile: input.profile.clone(),
                    realized_k,
                    fetch_query_bytes: sizes.query_bytes,
                    fetch_response_bytes: sizes.response_bytes,
                });
            }
            fetch_only_samples.push(check);
        }
    }

    if online_samples.iter().any(|sample| !sample.success)
        || correctness_rows
            .iter()
            .filter(|row| row.row_type == "measured")
            .any(|row| !row.success)
    {
        return Err(error("one or more measured runs failed correctness"));
    }
    serde_json::to_writer(
        io::stdout().lock(),
        &BenchmarkOutput {
            schema: SCHEMA,
            profile: input.profile,
            ring_dimension: input.workload.ring_dimension,
            warmups: input.warmups,
            measured_runs: input.measured_runs,
            openfhe_version: cbpir_bgv::BgvCpir::openfhe_version(),
            fps_runtime,
            rfps_runtime,
            fetch_runtime,
            setup_samples,
            online_samples,
            communication,
            correctness: correctness_rows,
            fetch_only_samples,
            fetch_only_communication,
        },
    )?;
    Ok(())
}
