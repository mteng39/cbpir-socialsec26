//! Final fixed-window full-capacity configuration runner.

use cbpir_bgv::{BgvCpir, BgvProfile};
use cbpir::{
    ALLOWED_FIXED_WINDOW_SLOTS, Catalog, ConfigurableFixedFetch, LogicalRecord, MatchMode,
    MatchRequest, Phase2Profile, RecordId, inspect_fixed_frame, locate_many, run_phase2_match,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io::{self, Read};
use std::time::Instant;

const SCHEMA: &str = "cbpir-phase2-full-capacity-final-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InputRecord {
    id: u64,
    source_line: u64,
    source_label: String,
    searchable: Vec<u8>,
    payload: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WorkloadInput {
    schema: String,
    ring_dimension: usize,
    n: usize,
    text_bytes: usize,
    transform_len: usize,
    ckks_slots: usize,
    match_utilization: f64,
    payload_bytes: usize,
    record_slots: usize,
    fetch_slots_used: usize,
    bgv_slots: usize,
    fetch_window_utilization: f64,
    payload_utilization: f64,
    max_serialized_slots: usize,
    serialized_slots_min: usize,
    serialized_slots_max: usize,
    serialized_slots_mean: f64,
    serialized_slots_median: f64,
    limiting_stage: String,
    searchable_length_min: usize,
    searchable_length_max: usize,
    searchable_length_mean: f64,
    searchable_length_median: f64,
    payload_length_min: usize,
    payload_length_max: usize,
    payload_length_mean: f64,
    payload_length_median: f64,
    selection_seed: String,
    source_sha256: String,
    source_commit: String,
    eligible_records: usize,
    no_physical_dummy_windows: bool,
    records: Vec<InputRecord>,
    spans: Value,
    required_slots_by_record: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct OracleInput {
    accepted_positions: Vec<usize>,
    boundary_rejected_positions: Vec<usize>,
    candidate_record_ids: Vec<u64>,
    selected_record_ids: Vec<u64>,
    duplicate_candidate_offsets: usize,
    truncated_count: usize,
    intrinsic_cancellation_count: usize,
    threshold_only_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct QueryInput {
    schema: String,
    id: String,
    category: String,
    mode: String,
    pattern: Vec<u8>,
    pattern_ascii: String,
    pattern_length: usize,
    weights: Vec<u8>,
    weight_policy: String,
    threshold: f64,
    public_k: usize,
    candidate_relation: String,
    oracle: OracleInput,
}

#[derive(Clone, Debug, Deserialize)]
struct StudyInput {
    workload: WorkloadInput,
    queries: Vec<QueryInput>,
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn error(message: impl Into<String>) -> Box<dyn std::error::Error> {
    io::Error::other(message.into()).into()
}

fn bgv_profile(ring_dimension: usize) -> Result<BgvProfile, Box<dyn std::error::Error>> {
    match ring_dimension {
        8192 => Ok(BgvProfile::Bgv13),
        16384 => Ok(BgvProfile::Bgv14),
        32768 => Ok(BgvProfile::Bgv15),
        _ => Err(error("unsupported final-study ring dimension")),
    }
}

fn request(query: &QueryInput) -> Result<MatchRequest, Box<dyn std::error::Error>> {
    if query.schema != SCHEMA || query.pattern_length != query.pattern.len() {
        return Err(error("query schema or pattern length drift"));
    }
    let mode = match query.mode.as_str() {
        "fps" if query.weights.is_empty() && query.weight_policy == "none" => MatchMode::Fps,
        "rfps" if query.weights.len() == query.pattern.len() => MatchMode::Rfps {
            randomizer: query.weights.clone(),
        },
        _ => return Err(error("query mode/weight contract drift")),
    };
    Ok(MatchRequest {
        pattern: query.pattern.clone(),
        threshold: query.threshold,
        mode,
    })
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

fn ids(values: &[cbpir::Candidate]) -> Vec<u64> {
    values.iter().map(|candidate| candidate.id.get()).collect()
}

fn validate_workload(
    workload: &WorkloadInput,
    catalog: &Catalog,
) -> Result<(), Box<dyn std::error::Error>> {
    if workload.schema != SCHEMA
        || workload.n != workload.records.len()
        || workload.n != catalog.records().len()
        || workload.text_bytes != catalog.text().len()
        || workload.transform_len != (workload.text_bytes + 1).next_power_of_two()
        || workload.transform_len > workload.ring_dimension / 2
        || workload.ckks_slots != workload.ring_dimension / 2
        || !ALLOWED_FIXED_WINDOW_SLOTS.contains(&workload.record_slots)
        || workload.fetch_slots_used != workload.n * workload.record_slots
        || workload.fetch_slots_used > workload.ring_dimension
        || workload.bgv_slots != workload.ring_dimension
        || !workload.no_physical_dummy_windows
        || workload.required_slots_by_record.len() != workload.n
    {
        return Err(error("workload capacity or fixed-layout contract drift"));
    }
    let measured = catalog
        .records()
        .iter()
        .map(inspect_fixed_frame)
        .collect::<Result<Vec<_>, _>>()?;
    if measured
        .iter()
        .map(|inspection| inspection.required_slots)
        .ne(workload.required_slots_by_record.iter().copied())
        || measured
            .iter()
            .any(|inspection| inspection.required_slots > workload.record_slots)
        || measured
            .iter()
            .map(|inspection| inspection.required_slots)
            .max()
            != Some(workload.max_serialized_slots)
    {
        return Err(error("encoded frame slot audit drift"));
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let requested_ring_dimension = std::env::args()
        .nth(1)
        .ok_or_else(|| error("usage: cbpir_phase2_fixed_final N < STUDY_JSON"))?
        .parse::<usize>()?;
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let study: StudyInput = serde_json::from_str(&raw)?;
    if study.workload.ring_dimension != requested_ring_dimension {
        return Err(error("argument/workload ring dimension mismatch"));
    }

    let records = logical_records(&study.workload.records)?;
    let catalog = Catalog::new(records)?;
    validate_workload(&study.workload, &catalog)?;
    let profile = Phase2Profile::for_ring_dimension(requested_ring_dimension)?;
    let bgv_profile = bgv_profile(requested_ring_dimension)?;
    let mut fetch = ConfigurableFixedFetch::prepare(
        catalog.records(),
        bgv_profile,
        study.workload.record_slots,
    )?;
    if fetch.preparation().actual_ring_dimension != requested_ring_dimension
        || fetch.preparation().physical_capacity != study.workload.n
        || fetch.preparation().real_record_count != study.workload.n
        || fetch.preparation().physical_dummy_window_count != 0
        || fetch.preparation().max_required_slots != study.workload.max_serialized_slots
    {
        return Err(error("BGV profile or final fixed database drift"));
    }

    let artifacts = tempfile::tempdir()?;
    let mut query_rows = Vec::new();
    for (query_index, query) in study.queries.iter().enumerate() {
        let request = request(query)?;
        let match_trace = run_phase2_match(&catalog, &request, profile)?;
        if usize::try_from(match_trace.ring_dimension)? != requested_ring_dimension
            || usize::try_from(match_trace.batch_size)? != study.workload.transform_len
        {
            return Err(error("CKKS profile or transform length drift"));
        }
        if query.mode == "fps" && match_trace.decision_mismatches != 0 {
            return Err(error(format!("FPS decision mismatch in {}", query.id)));
        }
        if match_trace.exact_matching_positions != query.oracle.accepted_positions {
            return Err(error(format!(
                "plaintext Match oracle drift in {}",
                query.id
            )));
        }

        let exact_locate = locate_many(
            &catalog,
            &request,
            &match_trace.exact_scores,
            &match_trace.exact_matching_positions,
            query.public_k,
        )?;
        let exact_oracle_agreement = ids(&exact_locate.candidates)
            == query.oracle.candidate_record_ids
            && ids(&exact_locate.selected_candidates) == query.oracle.selected_record_ids
            && exact_locate.boundary_rejected_positions == query.oracle.boundary_rejected_positions
            && exact_locate.duplicate_count == query.oracle.duplicate_candidate_offsets
            && exact_locate.truncated_count == query.oracle.truncated_count;
        if !exact_oracle_agreement {
            return Err(error(format!(
                "Locate plaintext oracle drift in {}",
                query.id
            )));
        }

        let locate_start = Instant::now();
        let locate = locate_many(
            &catalog,
            &request,
            &match_trace.scores,
            &match_trace.matching_positions,
            query.public_k,
        )?;
        let locate_ns = elapsed_ns(locate_start);
        let selected_indices = locate
            .selected_candidates
            .iter()
            .map(|candidate| candidate.physical_index)
            .collect::<Vec<_>>();
        let artifact_directory = artifacts.path().join(format!("query-{query_index}"));
        let fetch_trace = fetch.run(&selected_indices, &artifact_directory)?;
        let output_ids = fetch_trace
            .outputs
            .iter()
            .map(|record| record.id.get())
            .collect::<Vec<_>>();
        let selected_ids = ids(&locate.selected_candidates);
        let end_to_end_exact = output_ids == selected_ids
            && fetch_trace.selected_payloads_exact
            && fetch_trace.unselected_windows_zero
            && fetch_trace.unused_tail_zero;
        if !end_to_end_exact
            || fetch_trace.query_ciphertext_count != 1
            || fetch_trace.response_ciphertext_count != 1
        {
            return Err(error(format!("Fetch composition failure in {}", query.id)));
        }
        let approximate_selection_agrees_with_exact =
            selected_ids == ids(&exact_locate.selected_candidates);

        query_rows.push(json!({
            "query": query,
            "match": match_trace,
            "locate": locate,
            "exact_locate": exact_locate,
            "locate_ns": locate_ns,
            "exact_oracle_agreement": exact_oracle_agreement,
            "approximate_selection_agrees_with_exact": approximate_selection_agrees_with_exact,
            "fetch": fetch_trace,
            "end_to_end_exact": end_to_end_exact,
        }));
    }

    let mut probe_rows = Vec::new();
    for selected_size in 0_usize..=17 {
        let selected = (0..selected_size).collect::<Vec<_>>();
        let artifact_directory = artifacts.path().join(format!("probe-s{selected_size}"));
        let trace = fetch.run(&selected, &artifact_directory)?;
        if trace.query_ciphertext_count != 1
            || trace.response_ciphertext_count != 1
            || !trace.selected_payloads_exact
            || !trace.unselected_windows_zero
            || !trace.unused_tail_zero
            || (selected_size == 0 && !trace.empty_response_all_zero)
        {
            return Err(error("selector cardinality probe failed"));
        }
        probe_rows.push(json!({
            "public_k_reference": 16,
            "selected_size": selected_size,
            "relation": if selected_size < 16 { "fewer" } else if selected_size == 16 { "exact" } else { "more" },
            "fetch_did_not_truncate": trace.outputs.len() == selected_size,
            "trace": trace,
        }));
    }
    let query_sizes = probe_rows
        .iter()
        .map(|row| row["trace"]["query_bytes"].as_u64().unwrap())
        .collect::<BTreeSet<_>>();
    let response_sizes = probe_rows
        .iter()
        .map(|row| row["trace"]["response_bytes"].as_u64().unwrap())
        .collect::<BTreeSet<_>>();
    if query_sizes.len() != 1 || response_sizes.len() != 1 {
        return Err(error(
            "serialized BGV ciphertext size changed with realized k",
        ));
    }

    let output = json!({
        "schema": SCHEMA,
        "phase": "phase2_final_fixed_configuration_selection",
        "timing_scope": "diagnostic_only_not_final_benchmark",
        "match_backend": "server_text_ct_pt",
        "fetch_layout": "uniform_fixed_window",
        "openfhe_version": BgvCpir::openfhe_version(),
        "security": "HEStd_128_classic",
        "workload": study.workload,
        "fetch_preparation": fetch.preparation(),
        "fetch_layout_entries": fetch.layout(),
        "queries": query_rows,
        "selector_probes": probe_rows,
        "serialized_size_independent_of_realized_k": true,
        "query_bytes": query_sizes.into_iter().next().unwrap(),
        "response_bytes": response_sizes.into_iter().next().unwrap(),
    });
    serde_json::to_writer(io::stdout().lock(), &output)?;
    Ok(())
}
