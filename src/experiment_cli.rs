#![allow(
    clippy::too_many_lines,
    reason = "Keep timed procedures and their validation visible in one execution path"
)]
#![allow(
    clippy::cast_precision_loss,
    reason = "Summary statistics use floating-point nanoseconds; raw integer samples remain authoritative"
)]

use crate::{
    Catalog, CliConfig, FormalFetchContext, FormalFetchDatabase, FormalFetchQuery,
    FormalFetchRecovered, FormalFetchResponse, GeneratedWorkload, LogicalRecord, MatchMode,
    MatchRequest, MultiLocateTrace, RecordId, SearchMode, compare, locate_many_from_scores, oracle,
};
use cbpir_bgv::BgvCpirConfig;
use cbpir_search::{
    FpsCkksParameters, FpsKeySwitchTechnique, ServerTextCtPtFpsQuery, ServerTextCtPtFpsServerState,
    ServerTextCtPtFpsStaged, ServerTextCtPtResponse, ServerTextCtPtRfpsQuery,
    ServerTextCtPtRfpsServerState, ServerTextCtPtRfpsStaged,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

pub const INTERACTIVE_SCHEMA: &str = "cbpir-interactive-v1";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CountSummary {
    pub mean: f64,
    pub median: f64,
    pub standard_deviation: f64,
    pub p95: u64,
}

pub fn summarize_counts(samples: &[usize]) -> Result<CountSummary, ExperimentError> {
    let values = samples
        .iter()
        .map(|value| {
            u64::try_from(*value).map_err(|error| execution_error("count conversion", error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let summary = summarize_ns(&values)?;
    Ok(CountSummary {
        mean: summary.mean_ns,
        median: summary.median_ns,
        standard_deviation: summary.standard_deviation_ns,
        p95: summary.p95_ns,
    })
}

/// Counts sum across trials; correctness flags require every trial to pass.
pub fn aggregate_correctness(
    rows: &[CorrectnessReport],
) -> Result<CorrectnessReport, ExperimentError> {
    let mut result = rows.first().cloned().ok_or(ExperimentError::EmptySample)?;
    for row in &rows[1..] {
        result.measured_trials += row.measured_trials;
        result.candidate_count += row.candidate_count;
        result.selected_count += row.selected_count;
        result.fps_oracle_disagreements += row.fps_oracle_disagreements;
        result.rfps_threshold_disagreements += row.rfps_threshold_disagreements;
        result.rfps_intrinsic_cancellations += row.rfps_intrinsic_cancellations;
        result.rfps_threshold_only += row.rfps_threshold_only;
        result.exact_fetch_reconstruction &= row.exact_fetch_reconstruction;
        result.selected_windows_exact &= row.selected_windows_exact;
        result.unselected_windows_zero &= row.unselected_windows_zero;
        result.unused_tail_zero &= row.unused_tail_zero;
    }
    Ok(result)
}

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
    pub rfps_base_seed: u64,
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
            rfps_base_seed: 0xCB91_27DF_60A4_135B,
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
    pub communication: CommunicationReport,
    pub correctness: CorrectnessReport,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModeSummary {
    pub candidate_count: CountSummary,
    pub selected_count: CountSummary,
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
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent correctness checks must remain separately observable"
)]
pub struct CorrectnessReport {
    pub measured_trials: usize,
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
    let median_ns = if len.is_multiple_of(2) {
        f64::midpoint(sorted[len / 2 - 1] as f64, sorted[len / 2] as f64)
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

struct FpsRun {
    query: ServerTextCtPtFpsQuery,
    response: ServerTextCtPtResponse,
    locate: MultiLocateTrace,
    fetch_query: FormalFetchQuery,
    fetch_response: FormalFetchResponse,
    recovered: FormalFetchRecovered,
    scores: Vec<f64>,
    durations: ProcedureDurations,
}

fn execution_error(stage: &str, error: impl std::fmt::Display) -> ExperimentError {
    ExperimentError::Execution(format!("{stage}: {error}"))
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn logical_records(workload: &GeneratedWorkload) -> Result<Vec<LogicalRecord>, ExperimentError> {
    workload
        .records
        .iter()
        .map(|record| {
            Ok(LogicalRecord {
                id: RecordId::new(record.id)
                    .map_err(|error| execution_error("workload record ID", error))?,
                searchable: record.searchable.clone(),
                payload: record.payload.clone(),
            })
        })
        .collect()
}

fn fps_once(
    context: &ServerTextCtPtFpsStaged,
    search: &mut ServerTextCtPtFpsServerState,
    fetch_context: &FormalFetchContext,
    fetch: &mut FormalFetchDatabase,
    catalog: &Catalog,
    pattern: &[u8],
    capacity: usize,
) -> Result<FpsRun, ExperimentError> {
    let started = Instant::now();
    let query = context
        .match_query(catalog.text().len(), pattern)
        .map_err(|error| execution_error("SearchQuery", format!("{error:?}")))?;
    let search_query_ns = elapsed_ns(started);

    let started = Instant::now();
    let response = context
        .match_response(search, &query)
        .map_err(|error| execution_error("SearchResponse", format!("{error:?}")))?;
    let search_response_ns = elapsed_ns(started);

    let request = MatchRequest {
        pattern: pattern.to_vec(),
        threshold: 0.5,
        mode: MatchMode::Fps,
    };
    let started = Instant::now();
    let scores = context
        .decrypt_idft_scores(&response, catalog.text().len(), pattern.len())
        .map_err(|error| execution_error("Locate score recovery", format!("{error:?}")))?;
    let locate = locate_many_from_scores(catalog, &request, &scores, capacity)
        .map_err(|error| execution_error("Locate", error))?;
    let locate_ns = elapsed_ns(started);

    let started = Instant::now();
    let selected_ids = locate
        .selected_candidates
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let fetch_query = fetch_context
        .fetch_query(fetch, &selected_ids)
        .map_err(|error| execution_error("FetchQuery", error))?;
    let fetch_query_ns = elapsed_ns(started);
    let started = Instant::now();
    let fetch_response = fetch_context.fetch_response(fetch, &fetch_query);
    let fetch_response_ns = elapsed_ns(started);
    let started = Instant::now();
    let recovered = fetch_context
        .recover(fetch, &fetch_query, &fetch_response)
        .map_err(|error| execution_error("Recover", error))?;
    let recover_ns = elapsed_ns(started);
    let search_ns = search_query_ns.saturating_add(search_response_ns);
    let fetch_ns = fetch_query_ns
        .saturating_add(fetch_response_ns)
        .saturating_add(recover_ns);
    let online_sum_ns = search_ns.saturating_add(locate_ns).saturating_add(fetch_ns);
    Ok(FpsRun {
        query,
        response,
        locate,
        fetch_query,
        fetch_response,
        recovered,
        scores,
        durations: ProcedureDurations {
            search_query_ns,
            search_response_ns,
            locate_ns,
            fetch_query_ns,
            fetch_response_ns,
            recover_ns,
            search_ns,
            fetch_ns,
            online_sum_ns,
        },
    })
}

fn mode_summary(samples: &[RawMeasurement]) -> Result<ModeSummary, ExperimentError> {
    let search = summarize_ns(
        &samples
            .iter()
            .map(|sample| sample.durations.search_ns)
            .collect::<Vec<_>>(),
    )?;
    let locate = summarize_ns(
        &samples
            .iter()
            .map(|sample| sample.durations.locate_ns)
            .collect::<Vec<_>>(),
    )?;
    let fetch = summarize_ns(
        &samples
            .iter()
            .map(|sample| sample.durations.fetch_ns)
            .collect::<Vec<_>>(),
    )?;
    let online_per_run = summarize_ns(
        &samples
            .iter()
            .map(|sample| sample.durations.online_sum_ns)
            .collect::<Vec<_>>(),
    )?;
    Ok(ModeSummary {
        candidate_count: summarize_counts(
            &samples
                .iter()
                .map(|sample| sample.candidate_count)
                .collect::<Vec<_>>(),
        )?,
        selected_count: summarize_counts(
            &samples
                .iter()
                .map(|sample| sample.selected_count)
                .collect::<Vec<_>>(),
        )?,
        median_online_per_run_ns: online_per_run.median_ns,
        r1_3_sum_of_stage_medians_ns: search.median_ns + locate.median_ns + fetch.median_ns,
        search,
        locate,
        fetch,
        online_per_run,
    })
}

fn verify_fetch(
    catalog: &Catalog,
    fetch: &FormalFetchDatabase,
    locate: &MultiLocateTrace,
    recovered: &FormalFetchRecovered,
    fetch_query: &FormalFetchQuery,
    fetch_response: &FormalFetchResponse,
    selected_ids: &[RecordId],
) -> Result<(bool, bool, bool, bool), ExperimentError> {
    let exact_fetch_reconstruction = recovered.records.len() == selected_ids.len()
        && recovered
            .records
            .iter()
            .zip(selected_ids)
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
        .collect::<std::collections::BTreeSet<_>>();
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
    let used_slots = fetch.capacity() * fetch.record_slots();
    let unused_tail_zero = recovered.slots[used_slots..]
        .iter()
        .all(|value| *value == 0);
    if !exact_fetch_reconstruction
        || !selected_windows_exact
        || !unselected_windows_zero
        || !unused_tail_zero
        || fetch_query.ciphertext_count() != 1
        || fetch_response.ciphertext_count() != 1
    {
        return Err(execution_error(
            "Fetch correctness",
            "encrypted Fetch reconstruction or packed-window invariant failed",
        ));
    }
    Ok((
        exact_fetch_reconstruction,
        selected_windows_exact,
        unselected_windows_zero,
        unused_tail_zero,
    ))
}

fn validate_fps_correctness(
    catalog: &Catalog,
    fetch: &FormalFetchDatabase,
    run: &FpsRun,
    pattern: &[u8],
    capacity: usize,
) -> Result<CorrectnessReport, ExperimentError> {
    let request = MatchRequest {
        pattern: pattern.to_vec(),
        threshold: 0.5,
        mode: MatchMode::Fps,
    };
    let exact = oracle(catalog, &request).map_err(|error| execution_error("FPS oracle", error))?;
    let comparisons = compare(&exact, &run.scores, 0.5)
        .map_err(|error| execution_error("FPS comparison", error))?;
    let fps_oracle_disagreements = comparisons
        .iter()
        .filter(|row| row.ckks_false_positive || row.ckks_false_negative)
        .count();
    let selected_ids = run
        .locate
        .selected_candidates
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let exact_first_k = exact
        .exact_ids
        .iter()
        .copied()
        .take(capacity)
        .collect::<Vec<_>>();
    if selected_ids != exact_first_k || fps_oracle_disagreements != 0 {
        return Err(execution_error(
            "FPS correctness",
            "encrypted Search/Locate differs from literal oracle",
        ));
    }
    let (
        exact_fetch_reconstruction,
        selected_windows_exact,
        unselected_windows_zero,
        unused_tail_zero,
    ) = verify_fetch(
        catalog,
        fetch,
        &run.locate,
        &run.recovered,
        &run.fetch_query,
        &run.fetch_response,
        &selected_ids,
    )?;
    Ok(CorrectnessReport {
        measured_trials: 1,
        candidate_count: run.locate.candidates.len(),
        selected_count: run.locate.selected_candidates.len(),
        exact_fetch_reconstruction,
        selected_windows_exact,
        unselected_windows_zero,
        unused_tail_zero,
        fps_oracle_disagreements,
        rfps_threshold_disagreements: 0,
        rfps_intrinsic_cancellations: 0,
        rfps_threshold_only: 0,
    })
}

fn communication_report(
    search: cbpir_search::ServerTextCtPtMessageSizes,
    fetch: crate::FormalFetchMessageSizes,
) -> CommunicationReport {
    let search_query_bytes = search.encrypted_query_ciphertext_bytes.iter().sum::<u64>();
    CommunicationReport {
        total_online_bytes: search_query_bytes
            + search.encrypted_response_ciphertext_bytes
            + fetch.query_bytes
            + fetch.response_bytes,
        search_query_ciphertext_bytes: search.encrypted_query_ciphertext_bytes,
        search_query_bytes,
        search_response_bytes: search.encrypted_response_ciphertext_bytes,
        fetch_query_bytes: fetch.query_bytes,
        fetch_response_bytes: fetch.response_bytes,
        repeated_sizes_identical: true,
    }
}

fn finish_mode(report: &mut InteractiveReport, mode: &str) -> Result<(), ExperimentError> {
    let samples = report
        .measurements
        .raw
        .iter()
        .filter(|sample| sample.mode == mode)
        .cloned()
        .collect::<Vec<_>>();
    let first = samples
        .first()
        .ok_or_else(|| execution_error("report", "no measured trial"))?;
    let mut communication = first.communication.clone();
    communication.repeated_sizes_identical = samples
        .iter()
        .all(|sample| sample.communication == first.communication);
    let correctness = samples
        .iter()
        .map(|sample| sample.correctness.clone())
        .collect::<Vec<_>>();
    report
        .measurements
        .summary
        .insert(mode.into(), mode_summary(&samples)?);
    report.communication.insert(mode.into(), communication);
    report
        .correctness
        .insert(mode.into(), aggregate_correctness(&correctness)?);
    Ok(())
}

fn prepare_fetch_context(config: &CliConfig) -> Result<FormalFetchContext, ExperimentError> {
    FormalFetchContext::from_config(
        "BGV-interactive",
        BgvCpirConfig {
            ring_dimension: config.bgv_ring_dimension,
            plaintext_modulus: 786_433,
            record_slots: config.record_slots,
            multiplicative_depth: 1,
        },
    )
    .map_err(|error| execution_error("Fetch context", error))
}

fn run_fps(
    report: &mut InteractiveReport,
    records: Vec<LogicalRecord>,
    artifact_root: &Path,
) -> Result<(), ExperimentError> {
    let config = &report.configuration;
    let (scaling_modulus_bits, key_switch_technique) = match config.ckks_ring_dimension {
        8192 => (50, FpsKeySwitchTechnique::Bv),
        16_384 => (40, FpsKeySwitchTechnique::Hybrid),
        32_768 | 65_536 => (50, FpsKeySwitchTechnique::Hybrid),
        ring => {
            return Err(execution_error(
                "FPS parameters",
                format!("unsupported CKKS ring dimension {ring}"),
            ));
        }
    };
    let parameters = FpsCkksParameters::fixed_manual(
        u32::try_from(config.ckks_ring_dimension)
            .map_err(|error| execution_error("FPS ring conversion", error))?,
        config.transform_len,
        60,
        scaling_modulus_bits,
        key_switch_technique,
    )
    .map_err(|error| execution_error("FPS parameters", format!("{error:?}")))?;
    let context = ServerTextCtPtFpsStaged::from_parameters(parameters, config.transform_len, 0.5)
        .map_err(|error| execution_error("FPS context", format!("{error:?}")))?;
    let fetch_context = prepare_fetch_context(config)?;
    let fps_runtime = context.runtime_profile();
    let fetch_runtime = fetch_context.runtime_profile().clone();
    let setup_started = Instant::now();
    let catalog = Catalog::new(records).map_err(|error| execution_error("Catalog", error))?;
    let mut search = context
        .prepare_server(catalog.text())
        .map_err(|error| execution_error("Search Setup", format!("{error:?}")))?;
    let mut fetch = fetch_context
        .prepare_database(catalog.records())
        .map_err(|error| execution_error("Fetch Setup", error))?;
    let database_preparation_ns = elapsed_ns(setup_started);
    report.setup.push(SetupMeasurement {
        mode: "fps",
        context_generation_ns: fps_runtime
            .context_generation_ns
            .saturating_add(fetch_runtime.context_generation_ns),
        key_generation_ns: fps_runtime
            .key_generation_ns
            .saturating_add(fetch_runtime.key_generation_ns),
        database_preparation_ns,
    });
    for _ in 0..config.warmups {
        let warmup = fps_once(
            &context,
            &mut search,
            &fetch_context,
            &mut fetch,
            &catalog,
            &report.workload.pattern.bytes,
            config.capacity,
        )?;
        validate_fps_correctness(
            &catalog,
            &fetch,
            &warmup,
            &report.workload.pattern.bytes,
            config.capacity,
        )?;
    }

    for run_id in 0..config.trials {
        let run = fps_once(
            &context,
            &mut search,
            &fetch_context,
            &mut fetch,
            &catalog,
            &report.workload.pattern.bytes,
            config.capacity,
        )?;
        let check = validate_fps_correctness(
            &catalog,
            &fetch,
            &run,
            &report.workload.pattern.bytes,
            config.capacity,
        )?;
        let directory = artifact_root.join(format!("fps-run-{run_id}"));
        let search_sizes = context
            .serialize_messages(&directory.join("search"), &run.query, &run.response)
            .map_err(|error| execution_error("Search serialization", format!("{error:?}")))?;
        let fetch_sizes = fetch_context
            .serialize_messages(
                &directory.join("fetch"),
                &run.fetch_query,
                &run.fetch_response,
            )
            .map_err(|error| execution_error("Fetch serialization", error))?;
        let sizes = communication_report(search_sizes, fetch_sizes);
        report.measurements.raw.push(RawMeasurement {
            run_id,
            mode: "fps",
            randomizer: Vec::new(),
            candidate_count: run.locate.candidates.len(),
            selected_count: run.locate.selected_candidates.len(),
            durations: run.durations,
            communication: sizes,
            correctness: check,
        });
    }
    finish_mode(report, "fps")
}

struct RfpsRun {
    query: ServerTextCtPtRfpsQuery,
    response: ServerTextCtPtResponse,
    locate: MultiLocateTrace,
    fetch_query: FormalFetchQuery,
    fetch_response: FormalFetchResponse,
    recovered: FormalFetchRecovered,
    scores: Vec<f64>,
    durations: ProcedureDurations,
}

#[allow(
    clippy::too_many_arguments,
    reason = "Explicit stage state plus RFPS randomizer defines the measured procedure"
)]
fn rfps_once(
    context: &ServerTextCtPtRfpsStaged,
    search: &mut ServerTextCtPtRfpsServerState,
    fetch_context: &FormalFetchContext,
    fetch: &mut FormalFetchDatabase,
    catalog: &Catalog,
    pattern: &[u8],
    capacity: usize,
    randomizer: &[u8],
) -> Result<RfpsRun, ExperimentError> {
    let started = Instant::now();
    let query = context
        .match_query(catalog.text().len(), pattern, randomizer)
        .map_err(|error| execution_error("SearchQuery", format!("{error:?}")))?;
    let search_query_ns = elapsed_ns(started);

    let started = Instant::now();
    let response = context
        .match_response(search, &query)
        .map_err(|error| execution_error("SearchResponse", format!("{error:?}")))?;
    let search_response_ns = elapsed_ns(started);

    let request = MatchRequest {
        pattern: pattern.to_vec(),
        threshold: 0.5,
        mode: MatchMode::Rfps {
            randomizer: randomizer.to_vec(),
        },
    };
    let started = Instant::now();
    let scores = context
        .decrypt_idft_scores(&response, catalog.text().len(), pattern.len())
        .map_err(|error| execution_error("Locate score recovery", format!("{error:?}")))?;
    let locate = locate_many_from_scores(catalog, &request, &scores, capacity)
        .map_err(|error| execution_error("Locate", error))?;
    let locate_ns = elapsed_ns(started);

    let started = Instant::now();
    let selected_ids = locate
        .selected_candidates
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let fetch_query = fetch_context
        .fetch_query(fetch, &selected_ids)
        .map_err(|error| execution_error("FetchQuery", error))?;
    let fetch_query_ns = elapsed_ns(started);
    let started = Instant::now();
    let fetch_response = fetch_context.fetch_response(fetch, &fetch_query);
    let fetch_response_ns = elapsed_ns(started);
    let started = Instant::now();
    let recovered = fetch_context
        .recover(fetch, &fetch_query, &fetch_response)
        .map_err(|error| execution_error("Recover", error))?;
    let recover_ns = elapsed_ns(started);
    let search_ns = search_query_ns.saturating_add(search_response_ns);
    let fetch_ns = fetch_query_ns
        .saturating_add(fetch_response_ns)
        .saturating_add(recover_ns);
    let online_sum_ns = search_ns.saturating_add(locate_ns).saturating_add(fetch_ns);
    Ok(RfpsRun {
        query,
        response,
        locate,
        fetch_query,
        fetch_response,
        recovered,
        scores,
        durations: ProcedureDurations {
            search_query_ns,
            search_response_ns,
            locate_ns,
            fetch_query_ns,
            fetch_response_ns,
            recover_ns,
            search_ns,
            fetch_ns,
            online_sum_ns,
        },
    })
}

fn validate_rfps_correctness(
    catalog: &Catalog,
    fetch: &FormalFetchDatabase,
    run: &RfpsRun,
    pattern: &[u8],
    capacity: usize,
    randomizer: &[u8],
) -> Result<CorrectnessReport, ExperimentError> {
    let request = MatchRequest {
        pattern: pattern.to_vec(),
        threshold: 0.5,
        mode: MatchMode::Rfps {
            randomizer: randomizer.to_vec(),
        },
    };
    let exact = oracle(catalog, &request).map_err(|error| execution_error("RFPS oracle", error))?;
    let comparisons = compare(&exact, &run.scores, 0.5)
        .map_err(|error| execution_error("RFPS comparison", error))?;
    let rfps_disagreements = comparisons
        .iter()
        .filter(|row| row.ckks_false_positive || row.ckks_false_negative)
        .count();
    let selected_ids = run
        .locate
        .selected_candidates
        .iter()
        .map(|candidate| candidate.id)
        .collect::<Vec<_>>();
    let _exact_first_k = exact
        .exact_ids
        .iter()
        .copied()
        .take(capacity)
        .collect::<Vec<_>>();
    if selected_ids.len() > capacity {
        return Err(execution_error(
            "RFPS correctness",
            "selected record count exceeds retrieval capacity",
        ));
    }
    let (
        exact_fetch_reconstruction,
        selected_windows_exact,
        unselected_windows_zero,
        unused_tail_zero,
    ) = verify_fetch(
        catalog,
        fetch,
        &run.locate,
        &run.recovered,
        &run.fetch_query,
        &run.fetch_response,
        &selected_ids,
    )?;
    Ok(CorrectnessReport {
        measured_trials: 1,
        candidate_count: run.locate.candidates.len(),
        selected_count: run.locate.selected_candidates.len(),
        exact_fetch_reconstruction,
        selected_windows_exact,
        unselected_windows_zero,
        unused_tail_zero,
        fps_oracle_disagreements: 0,
        rfps_threshold_disagreements: rfps_disagreements,
        rfps_intrinsic_cancellations: comparisons
            .iter()
            .filter(|row| row.intrinsic_cancellation)
            .count(),
        rfps_threshold_only: comparisons.iter().filter(|row| row.threshold_only).count(),
    })
}

fn run_rfps(
    report: &mut InteractiveReport,
    records: Vec<LogicalRecord>,
    artifact_root: &Path,
) -> Result<(), ExperimentError> {
    let config = &report.configuration;
    let context = ServerTextCtPtRfpsStaged::from_config(
        cbpir_search::CkksConfig {
            ring_dimension: Some(
                u32::try_from(config.ckks_ring_dimension)
                    .map_err(|error| execution_error("RFPS ring", error))?,
            ),
            batch_size: u32::try_from(config.transform_len)
                .map_err(|error| execution_error("RFPS batch", error))?,
            multiplicative_depth: 1,
            scaling_modulus_bits: 24,
            first_modulus_bits: 60,
            decision_threshold: 0.5,
            scaling_technique: cbpir_search::FpsScalingTechnique::FixedManual,
            key_switch_technique: FpsKeySwitchTechnique::Hybrid,
        },
        config.transform_len,
        0.5,
    )
    .map_err(|error| execution_error("RFPS context", format!("{error:?}")))?;
    let fetch_context = prepare_fetch_context(config)?;
    let rfps_runtime = context.runtime_profile();
    let fetch_runtime = fetch_context.runtime_profile().clone();
    let setup_started = Instant::now();
    let catalog = Catalog::new(records).map_err(|error| execution_error("Catalog", error))?;
    let mut search = context
        .prepare_server(catalog.text())
        .map_err(|error| execution_error("Search Setup", format!("{error:?}")))?;
    let mut fetch = fetch_context
        .prepare_database(catalog.records())
        .map_err(|error| execution_error("Fetch Setup", error))?;
    let database_preparation_ns = elapsed_ns(setup_started);
    report.setup.push(SetupMeasurement {
        mode: "rfps",
        context_generation_ns: rfps_runtime
            .context_generation_ns
            .saturating_add(fetch_runtime.context_generation_ns),
        key_generation_ns: rfps_runtime
            .key_generation_ns
            .saturating_add(fetch_runtime.key_generation_ns),
        database_preparation_ns,
    });
    for _ in 0..config.warmups {
        let randomizer =
            crate::fresh_rfps_weights_for_seed(0xCB91_27DF_60A4_135B, config.pattern_length);
        let warmup = rfps_once(
            &context,
            &mut search,
            &fetch_context,
            &mut fetch,
            &catalog,
            &report.workload.pattern.bytes,
            config.capacity,
            &randomizer,
        )?;
        validate_rfps_correctness(
            &catalog,
            &fetch,
            &warmup,
            &report.workload.pattern.bytes,
            config.capacity,
            &randomizer,
        )?;
    }

    for run_id in 0..config.trials {
        let randomizer = crate::fresh_rfps_weights_for_seed(
            0xCB91_27DF_60A4_135B ^ run_id as u64,
            config.pattern_length,
        );
        let run = rfps_once(
            &context,
            &mut search,
            &fetch_context,
            &mut fetch,
            &catalog,
            &report.workload.pattern.bytes,
            config.capacity,
            &randomizer,
        )?;
        let check = validate_rfps_correctness(
            &catalog,
            &fetch,
            &run,
            &report.workload.pattern.bytes,
            config.capacity,
            &randomizer,
        )?;
        let directory = artifact_root.join(format!("rfps-run-{run_id}"));
        let search_sizes = context
            .serialize_messages(&directory.join("search"), &run.query, &run.response)
            .map_err(|error| execution_error("Search serialization", format!("{error:?}")))?;
        let fetch_sizes = fetch_context
            .serialize_messages(
                &directory.join("fetch"),
                &run.fetch_query,
                &run.fetch_response,
            )
            .map_err(|error| execution_error("Fetch serialization", error))?;
        let sizes = communication_report(search_sizes, fetch_sizes);
        report.measurements.raw.push(RawMeasurement {
            run_id,
            mode: "rfps",
            randomizer: randomizer.clone(),
            candidate_count: run.locate.candidates.len(),
            selected_count: run.locate.selected_candidates.len(),
            durations: run.durations,
            communication: sizes,
            correctness: check,
        });
    }
    finish_mode(report, "rfps")
}

pub fn run_interactive(
    config: CliConfig,
    workload: GeneratedWorkload,
) -> Result<InteractiveReport, ExperimentError> {
    let records = logical_records(&workload)?;
    let parameters = ParameterReport::for_config(&config, cbpir_bgv::BgvCpir::openfhe_version());
    let mode = config.search_mode;
    let mut report = InteractiveReport::new(config, workload, parameters);
    let artifact_parent = Path::new("target/reproduction/interactive");
    fs::create_dir_all(artifact_parent)
        .map_err(|error| execution_error("artifact directory", error))?;
    let artifacts = tempfile::Builder::new()
        .prefix("cbpir-")
        .tempdir_in(artifact_parent)
        .map_err(|error| execution_error("artifact directory", error))?;
    match mode {
        SearchMode::Fps => run_fps(&mut report, records, artifacts.path())?,
        SearchMode::Rfps => run_rfps(&mut report, records, artifacts.path())?,
        SearchMode::Both => {
            run_fps(&mut report, records.clone(), artifacts.path())?;
            run_rfps(&mut report, records, artifacts.path())?;
        }
    }
    Ok(report)
}
