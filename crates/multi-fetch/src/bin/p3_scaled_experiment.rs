use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use cbpir_bgv::{ArtifactSizes, BgvCpir, BgvCpirConfig};
use cbpir_fetch::{
    BASE_WINDOW_BYTES, PHYSICAL_WINDOWS_PER_SHARD, RecordId, SUPPORTED_DATABASE_RECORDS,
    SUPPORTED_PAYLOAD_BYTES, SUPPORTED_PUBLIC_K, ScaledFetchPlan, ScaledFetchResult, ScaledLayout,
    ScaledSetupTimings, ShardedP3BgvBackend,
};
use serde::Serialize;
use serde_json::{Value, json};

const OPENFHE_RS_COMMIT: &str = "3e6a46b1377e89e09414cbca7c0c0140c5338397";
const OPENFHE_COMMIT: &str = "1306d14f8c26bb6150d3e6ad54f28dfe1007689e";
const SEMANTIC_FETCHES_PER_CASE: usize = 8;

type DynError = Box<dyn Error>;

struct Arguments {
    repetitions: usize,
    artifacts_dir: PathBuf,
    database_records: Option<usize>,
    payload_bytes: Option<usize>,
    public_k: Option<usize>,
}

#[derive(Clone, Copy)]
struct CaseConfig {
    database_records: usize,
    payload_bytes: usize,
    public_k: usize,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct SampleStats {
    samples: usize,
    minimum: u64,
    mean: u64,
    maximum: u64,
}

#[derive(Debug, Serialize)]
struct TimingReport {
    measured_repetitions: usize,
    warmup_repetitions: usize,
    planning: SampleStats,
    database_pack_encode: SampleStats,
    query_encode_encrypt: SampleStats,
    server_ciphertext_plaintext_multiply: SampleStats,
    client_decrypt_decode: SampleStats,
    reconstruction: SampleStats,
    total: SampleStats,
}

#[derive(Default)]
struct TimingSamples {
    planning: Vec<u64>,
    database_pack_encode: Vec<u64>,
    query_encode_encrypt: Vec<u64>,
    server_multiply: Vec<u64>,
    client_decrypt_decode: Vec<u64>,
    reconstruction: Vec<u64>,
    total: Vec<u64>,
}

impl TimingSamples {
    fn push(&mut self, planning_ns: u64, result: &ScaledFetchResult) {
        self.planning.push(planning_ns);
        self.database_pack_encode
            .push(result.timings.database_pack_encode_ns);
        self.query_encode_encrypt
            .push(result.timings.query_encode_encrypt_ns);
        self.server_multiply.push(result.timings.server_multiply_ns);
        self.client_decrypt_decode
            .push(result.timings.client_decrypt_decode_ns);
        self.reconstruction.push(result.timings.reconstruction_ns);
        self.total.push(result.timings.total_ns);
    }

    fn report(&self, repetitions: usize) -> TimingReport {
        TimingReport {
            measured_repetitions: repetitions,
            warmup_repetitions: 1,
            planning: timing_summary(&self.planning),
            database_pack_encode: timing_summary(&self.database_pack_encode),
            query_encode_encrypt: timing_summary(&self.query_encode_encrypt),
            server_ciphertext_plaintext_multiply: timing_summary(&self.server_multiply),
            client_decrypt_decode: timing_summary(&self.client_decrypt_decode),
            reconstruction: timing_summary(&self.reconstruction),
            total: timing_summary(&self.total),
        }
    }
}

fn parse_arguments() -> Result<Arguments, DynError> {
    let mut repetitions = 3_usize;
    let mut artifacts_dir = None;
    let mut database_records = None;
    let mut payload_bytes = None;
    let mut public_k = None;
    let mut arguments = env::args_os().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--repetitions") => {
                repetitions = next_usize(&mut arguments, "--repetitions")?;
                if repetitions == 0 {
                    return Err("repetitions must be positive".into());
                }
            }
            Some("--artifacts-dir") => {
                artifacts_dir = Some(PathBuf::from(
                    arguments.next().ok_or("--artifacts-dir requires a value")?,
                ));
            }
            Some("--database-records") => {
                database_records = Some(next_usize(&mut arguments, "--database-records")?);
            }
            Some("--payload-bytes") => {
                payload_bytes = Some(next_usize(&mut arguments, "--payload-bytes")?);
            }
            Some("--public-k") => {
                public_k = Some(next_usize(&mut arguments, "--public-k")?);
            }
            Some(other) => return Err(format!("unknown argument: {other}").into()),
            None => return Err("arguments must be UTF-8".into()),
        }
    }

    validate_filter(
        database_records,
        &SUPPORTED_DATABASE_RECORDS,
        "database records",
    )?;
    validate_filter(payload_bytes, &SUPPORTED_PAYLOAD_BYTES, "payload bytes")?;
    validate_filter(public_k, &SUPPORTED_PUBLIC_K, "public K")?;
    Ok(Arguments {
        repetitions,
        artifacts_dir: artifacts_dir.ok_or("--artifacts-dir is required")?,
        database_records,
        payload_bytes,
        public_k,
    })
}

fn next_usize<I>(arguments: &mut I, flag: &str) -> Result<usize, DynError>
where
    I: Iterator<Item = OsString>,
{
    let raw = arguments
        .next()
        .ok_or_else(|| format!("{flag} requires a value"))?;
    Ok(raw
        .to_str()
        .ok_or_else(|| format!("{flag} value must be UTF-8"))?
        .parse()?)
}

fn validate_filter(
    filter: Option<usize>,
    supported: &[usize],
    label: &str,
) -> Result<(), DynError> {
    if let Some(value) = filter
        && !supported.contains(&value)
    {
        return Err(format!("unsupported {label}: {value}").into());
    }
    Ok(())
}

fn selected_cases(arguments: &Arguments) -> Vec<CaseConfig> {
    let mut cases = Vec::new();
    for database_records in SUPPORTED_DATABASE_RECORDS {
        if arguments
            .database_records
            .is_some_and(|filter| filter != database_records)
        {
            continue;
        }
        for payload_bytes in SUPPORTED_PAYLOAD_BYTES {
            if arguments
                .payload_bytes
                .is_some_and(|filter| filter != payload_bytes)
            {
                continue;
            }
            for public_k in SUPPORTED_PUBLIC_K {
                if arguments.public_k.is_some_and(|filter| filter != public_k) {
                    continue;
                }
                cases.push(CaseConfig {
                    database_records,
                    payload_bytes,
                    public_k,
                });
            }
        }
    }
    cases
}

fn deterministic_records(record_count: usize, payload_bytes: usize) -> Vec<Vec<u8>> {
    (0..record_count)
        .map(|record_id| deterministic_record(record_id, payload_bytes))
        .collect()
}

fn deterministic_record(record_id: usize, payload_bytes: usize) -> Vec<u8> {
    if record_id == 0 {
        return vec![0; payload_bytes];
    }
    let id_u64 = u64::try_from(record_id).expect("supported record ID fits u64");
    let mut bytes = (0..payload_bytes)
        .map(|offset| {
            let offset_u64 = u64::try_from(offset).expect("supported offset fits u64");
            let quotient_u64 = u64::try_from(offset / 7).expect("supported quotient fits u64");
            u8::try_from((id_u64 * 131 + offset_u64 * 17 + quotient_u64) % 251)
                .expect("fixture byte is below 251")
        })
        .collect::<Vec<_>>();
    bytes[..8].copy_from_slice(&id_u64.to_le_bytes());
    bytes[8] = 0;
    bytes[payload_bytes / 2] = 0;
    bytes[payload_bytes - 1] = 0;
    bytes
}

fn range_ids(count: usize) -> Vec<RecordId> {
    (0..count)
        .map(|index| RecordId(u64::try_from(index).expect("supported ID fits u64")))
        .collect()
}

fn distributed_ids(database_records: usize, count: usize) -> Vec<RecordId> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![RecordId(
            u64::try_from(database_records - 1).expect("supported ID fits u64"),
        )];
    }
    (0..count)
        .map(|offset| {
            let index = offset * (database_records - 1) / (count - 1);
            RecordId(u64::try_from(index).expect("supported ID fits u64"))
        })
        .collect()
}

fn expected_selected(candidates: &[RecordId], public_k: usize) -> (Vec<RecordId>, usize) {
    let mut seen = HashSet::with_capacity(candidates.len());
    let unique = candidates
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .collect::<Vec<_>>();
    let truncated = unique.len().saturating_sub(public_k);
    (unique.into_iter().take(public_k).collect(), truncated)
}

fn verify_result(
    backend: &ShardedP3BgvBackend,
    source: &[Vec<u8>],
    candidates: &[RecordId],
    plan: &ScaledFetchPlan,
    result: &ScaledFetchResult,
) -> Result<(), DynError> {
    let layout = backend.layout();
    let (expected_ids, expected_truncated) = expected_selected(candidates, layout.public_k());
    if plan.selected_ids() != expected_ids {
        return Err("stable selected-ID order changed".into());
    }
    if plan.truncated_unique_count() != expected_truncated {
        return Err("stable unique truncation count changed".into());
    }
    if result.records.len() != expected_ids.len() {
        return Err("reconstructed record count changed".into());
    }
    for (record, expected_id) in result.records.iter().zip(expected_ids) {
        let index = usize::try_from(expected_id.0)?;
        if record.id() != expected_id || record.bytes() != source[index] {
            return Err(format!("record {} did not reconstruct exactly", expected_id.0).into());
        }
    }

    verify_counts(layout, result)?;
    for shard in 0..layout.shard_count() {
        let physical_ids = plan.shard_physical_ids(shard)?;
        if physical_ids.len() != layout.public_k() {
            return Err(format!("shard {shard} did not contain public K selectors").into());
        }
        let unique = physical_ids.iter().copied().collect::<HashSet<_>>();
        if unique.len() != layout.public_k() {
            return Err(format!("shard {shard} selector IDs were not distinct").into());
        }
    }
    Ok(())
}

fn verify_counts(layout: ScaledLayout, result: &ScaledFetchResult) -> Result<(), DynError> {
    let counts = result.counts;
    if counts.query_ciphertexts != layout.query_ciphertext_count()?
        || counts.response_ciphertexts != layout.response_ciphertext_count()?
        || counts.server_multiplications != layout.server_multiplication_count()?
        || counts.evaluated_chunk_planes != layout.response_ciphertext_count()?
    {
        return Err("observed operation counts differ from layout formulas".into());
    }
    Ok(())
}

fn execute_and_verify(
    backend: &mut ShardedP3BgvBackend,
    source: &[Vec<u8>],
    candidates: &[RecordId],
) -> Result<(ScaledFetchPlan, ScaledFetchResult), DynError> {
    let plan = backend.plan(candidates)?;
    let result = backend.fetch(&plan)?;
    verify_result(backend, source, candidates, &plan, &result)?;
    Ok((plan, result))
}

fn run_semantic_cases(
    backend: &mut ShardedP3BgvBackend,
    source: &[Vec<u8>],
) -> Result<usize, DynError> {
    let layout = backend.layout();
    let public_k = layout.public_k();
    let database_records = layout.database_records();
    let fewer = distributed_ids(database_records, public_k.saturating_sub(1));
    let exact = distributed_ids(database_records, public_k);
    let mut over = range_ids(public_k + 1);
    over.insert(1, RecordId(0));
    let first = vec![RecordId(0)];
    let last = vec![RecordId(u64::try_from(database_records - 1)?)];
    let same_shard = range_ids(public_k);
    let cross_shard = distributed_ids(database_records, public_k);
    let scenarios = [
        ("zero", Vec::new()),
        ("fewer-than-k", fewer),
        ("exactly-k", exact),
        ("more-than-k-with-duplicate", over),
        ("first-record", first),
        ("last-record", last),
        ("same-shard", same_shard),
        ("cross-shard-or-last-shard-at-k1", cross_shard),
    ];

    for (name, candidates) in scenarios {
        let (plan, result) = execute_and_verify(backend, source, &candidates)?;
        if name == "zero" && (!plan.selected_ids().is_empty() || !result.records.is_empty()) {
            return Err("zero-candidate case returned a real record".into());
        }
        if name == "more-than-k-with-duplicate" && plan.truncated_unique_count() != 1 {
            return Err("over-K semantic case did not truncate exactly one unique ID".into());
        }
    }
    Ok(SEMANTIC_FETCHES_PER_CASE)
}

fn run_measurements(
    arguments: &Arguments,
    backend: &mut ShardedP3BgvBackend,
    source: &[Vec<u8>],
) -> Result<TimingReport, DynError> {
    let layout = backend.layout();
    let candidates = distributed_ids(layout.database_records(), layout.public_k());
    let _warmup = execute_and_verify(backend, source, &candidates)?;
    let mut samples = TimingSamples::default();

    for _ in 0..arguments.repetitions {
        let planning_started = Instant::now();
        let plan = backend.plan(&candidates)?;
        let planning_ns = elapsed_ns(planning_started);
        let result = backend.fetch(&plan)?;
        verify_result(backend, source, &candidates, &plan, &result)?;
        samples.push(planning_ns, &result);
    }
    Ok(samples.report(arguments.repetitions))
}

fn run_case(arguments: &Arguments, case: CaseConfig) -> Result<Value, DynError> {
    let source = deterministic_records(case.database_records, case.payload_bytes);
    let (mut backend, setup) =
        ShardedP3BgvBackend::new(source.clone(), case.public_k, BgvCpirConfig::prior_mini())?;
    let layout = backend.layout();
    let semantic_passed = run_semantic_cases(&mut backend, &source)?;
    let timings = run_measurements(arguments, &mut backend, &source)?;
    let serialization_plan =
        backend.plan(&distributed_ids(case.database_records, case.public_k))?;
    let sizes = backend.serialize_representative(&serialization_plan, &arguments.artifacts_dir)?;
    build_case_report(case, layout, setup, semantic_passed, &timings, sizes)
}

#[allow(
    clippy::too_many_lines,
    reason = "the case report keeps measured, derived, and interpretive evidence visibly separated"
)]
fn build_case_report(
    case: CaseConfig,
    layout: ScaledLayout,
    setup: ScaledSetupTimings,
    semantic_passed: usize,
    timings: &TimingReport,
    sizes: ArtifactSizes,
) -> Result<Value, DynError> {
    let query_ciphertexts = layout.query_ciphertext_count()?;
    let response_ciphertexts = layout.response_ciphertext_count()?;
    let server_multiplications = layout.server_multiplication_count()?;
    let query_count_u64 = u64::try_from(query_ciphertexts)?;
    let response_count_u64 = u64::try_from(response_ciphertexts)?;
    let derived_query_bytes = sizes
        .query_bytes
        .checked_mul(query_count_u64)
        .ok_or("derived query byte count overflow")?;
    let derived_response_bytes = sizes
        .response_bytes
        .checked_mul(response_count_u64)
        .ok_or("derived response byte count overflow")?;
    let derived_online_bytes = derived_query_bytes
        .checked_add(derived_response_bytes)
        .ok_or("derived online byte count overflow")?;
    let logical_database_bytes = u64::try_from(case.database_records)?
        .checked_mul(u64::try_from(case.payload_bytes)?)
        .ok_or("logical database byte count overflow")?;
    let total_selector_windows = layout
        .shard_count()
        .checked_mul(layout.public_k())
        .ok_or("selector window count overflow")?;
    let last_shard = layout.shard_count() - 1;
    let last_real_records = layout.shard_real_record_count(last_shard)?;
    let correctness_passed = semantic_passed
        .checked_add(1)
        .and_then(|value| value.checked_add(timings.measured_repetitions))
        .and_then(|value| value.checked_add(1))
        .ok_or("correctness pass count overflow")?;

    Ok(json!({
        "database_records": case.database_records,
        "payload_bytes": case.payload_bytes,
        "public_k": case.public_k,
        "logical_database_bytes": logical_database_bytes,
        "layout": {
            "base_window_bytes": BASE_WINDOW_BYTES,
            "physical_windows_per_shard": PHYSICAL_WINDOWS_PER_SHARD,
            "real_rows_per_shard": layout.real_rows_per_shard(),
            "shard_count": layout.shard_count(),
            "chunk_count": layout.chunk_count(),
            "last_shard_real_records": last_real_records,
            "dummy_windows_per_shard": layout.public_k(),
            "physical_selector_windows_across_shards": total_selector_windows,
            "formulas": {
                "real_rows_per_shard": "64 - K",
                "shards": "ceil(N / (64 - K))",
                "chunks": "B / 128",
                "queries": "shards",
                "responses_and_multiplications": "shards * chunks"
            }
        },
        "correctness": {
            "passed": correctness_passed,
            "failed": 0,
            "semantic_fetches_passed": semantic_passed,
            "semantic_cases": [
                "zero candidates",
                "fewer than K",
                "exactly K",
                "more than K with duplicate",
                "first record",
                "last record",
                "same shard",
                "cross shard; last-shard singleton when K=1"
            ],
            "warmup_fetches_passed": 1,
            "measured_fetches_passed": timings.measured_repetitions,
            "representative_serialization_round_trips_passed": 1,
            "full_decrypted_slot_vector_checked_per_plane": true,
            "interior_trailing_and_all_zero_payload_bytes_checked": true,
            "cross_shard_multi_selection_applicable": case.public_k > 1
        },
        "operations": {
            "query_ciphertexts": query_ciphertexts,
            "response_ciphertexts": response_ciphertexts,
            "server_multiplications": server_multiplications,
            "evaluated_chunk_planes": response_ciphertexts,
            "ciphertext_ciphertext_multiplications": 0,
            "rotations": 0,
            "evaluation_keys_generated": 0,
            "selector_windows_per_query": case.public_k,
            "operation": "ciphertext-plaintext packed multiplication"
        },
        "serialization_bytes": {
            "context": sizes.context_bytes,
            "public_key": sizes.public_key_bytes,
            "private_key": sizes.private_key_bytes,
            "representative_query": sizes.query_bytes,
            "representative_response": sizes.response_bytes,
            "evaluation_key": sizes.evaluation_key_bytes,
            "classification": "measured-one-physical-shard-chunk-exchange",
            "complete_logical_fetch_serialized": false,
            "artifact_files_retained": false
        },
        "communication_bytes": {
            "classification": "derived-count-times-measured-ciphertext-length",
            "query": derived_query_bytes,
            "response": derived_response_bytes,
            "online_total": derived_online_bytes,
            "derivation": {
                "query": "representative_query_bytes * query_ciphertexts",
                "response": "representative_response_bytes * response_ciphertexts"
            },
            "excludes": "context and key provisioning; no complete aggregate bundle was serialized"
        },
        "setup_timings_ns": {
            "context_generation": setup.context_generation_ns,
            "key_generation": setup.key_generation_ns
        },
        "timings_ns": timings
    }))
}

fn timing_summary(values: &[u64]) -> SampleStats {
    let total = values.iter().copied().map(u128::from).sum::<u128>();
    let divisor = u128::try_from(values.len()).unwrap_or(u128::MAX);
    let mean = if divisor == 0 {
        0
    } else {
        u64::try_from(total / divisor).unwrap_or(u64::MAX)
    };
    SampleStats {
        samples: values.len(),
        minimum: values.iter().copied().min().unwrap_or(0),
        mean,
        maximum: values.iter().copied().max().unwrap_or(0),
    }
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn command_first_line(program: &str, arguments: &[&str]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|stdout| stdout.lines().next().map(str::to_owned))
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("model name\t: ")
                    .or_else(|| line.strip_prefix("Hardware\t: "))
                    .map(str::to_owned)
            })
        })
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn memory_total_kib() -> Option<u64> {
    fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemTotal:")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}

#[allow(
    clippy::too_many_lines,
    reason = "the top-level report keeps all evidence classes and scope exclusions explicit"
)]
fn build_report(arguments: &Arguments, cases: &[Value]) -> Result<Value, DynError> {
    let config = BgvCpirConfig::prior_mini();
    let filters_applied = arguments.database_records.is_some()
        || arguments.payload_bytes.is_some()
        || arguments.public_k.is_some();
    Ok(json!({
        "schema": "p3-sharded-payload-scaling-openfhe-bgv-v1",
        "classification": "measured-local-experiment",
        "supported_database_records": SUPPORTED_DATABASE_RECORDS,
        "supported_payload_bytes": SUPPORTED_PAYLOAD_BYTES,
        "supported_public_k": SUPPORTED_PUBLIC_K,
        "dependencies": {
            "openfhe_rs_commit": OPENFHE_RS_COMMIT,
            "openfhe_rs_crate_version": "0.3.2",
            "openfhe_commit": OPENFHE_COMMIT,
            "openfhe_reported_version": BgvCpir::openfhe_version(),
            "prototype_version": env!("CARGO_PKG_VERSION")
        },
        "environment": {
            "working_directory": env::current_dir()?.to_string_lossy(),
            "rustc": command_first_line("rustc", &["--version"]),
            "cargo": command_first_line("cargo", &["--version"]),
            "cmake": command_first_line("cmake", &["--version"]),
            "cxx": command_first_line("c++", &["--version"]),
            "kernel": command_first_line("uname", &["-srmo"]),
            "cpu_model": cpu_model(),
            "available_parallelism": std::thread::available_parallelism().map(usize::from).ok(),
            "memory_total_kib": memory_total_kib(),
            "cargo_profile": "release"
        },
        "invocation": {
            "repetitions": arguments.repetitions,
            "warmup_repetitions_per_case": 1,
            "artifacts_directory": arguments.artifacts_dir.to_string_lossy(),
            "filters_applied": filters_applied,
            "database_records_filter": arguments.database_records,
            "payload_bytes_filter": arguments.payload_bytes,
            "public_k_filter": arguments.public_k
        },
        "parameters": {
            "scheme": "BGV-RNS",
            "security_setting": "HEStd_128_classic",
            "requested_ring_dimension": config.ring_dimension,
            "actual_ring_dimension": 8192,
            "actual_ring_dimension_evidence": "every case constructor queried OpenFHE and rejected any value other than 8192",
            "plaintext_modulus": config.plaintext_modulus,
            "multiplicative_depth": config.multiplicative_depth,
            "record_slots": config.record_slots,
            "physical_capacity_records": PHYSICAL_WINDOWS_PER_SHARD,
            "encoding": "one unsigned payload byte per packed SIMD slot",
            "ciphertext_modulus_chain": "OpenFHE-generated; unavailable through selected wrapper"
        },
        "methodology": {
            "matrix": "Cartesian product N in {2^10,...,2^14}, B in {256,1024}, K in {1,4,8,16}; filters only for focused regression runs",
            "database_generation": "deterministic exact-width binary records; record zero all-zero; other records preserve full u64 ID plus interior and trailing zeros",
            "selector": "HLF-PIR-style complete-window multi-hot selector with exactly public K distinct local windows in every public shard",
            "shard_policy": "query every shard in public order; reuse one encrypted shard selector across all payload chunk planes",
            "preprocessing": "raw records retained; plaintext database assembled and encoded again for every shard/chunk plane in every Fetch",
            "correctness": "eight semantic Fetches, one warmup, every measured Fetch, and one representative serialization round trip per case",
            "timing_clock": "std::time::Instant wall-clock nanoseconds",
            "timing_samples": "one warmup excluded; requested measured repetitions reported as min/mean/max",
            "serialization": "one representative physical query/response measured; fresh-object response decryption; five artifact files removed",
            "aggregate_communication": "derived as public ciphertext count times measured representative ciphertext length; not a serialized aggregate bundle",
            "oracle_memory": "experiment retains a second raw database copy solely for byte-exact correctness checking"
        },
        "security_evidence": {
            "library_security_setting": "HEStd_128_classic",
            "independent_security_estimator": "unavailable",
            "residual_noise_budget": "unavailable",
            "exact_decryption": "measured for every evaluated chunk plane; correctness evidence, not residual-noise evidence",
            "query_privacy_scope": "conditional on OpenFHE BGV IND-CPA security, secret-key confidentiality, honest well-formed selectors, and all-shard processing",
            "target_shard_disclosure": "not deliberately disclosed by transcript shape; counts depend only on public N, B, and K",
            "malicious_selector_validation": "not provided server-side",
            "privacy_proven_by_tests": false
        },
        "cases": cases,
        "scope": {
            "canonical_single_ring_p3": "unchanged: one selector query, one ciphertext-plaintext multiplication, one response",
            "scaled_logical_fetch": "many physical ciphertexts: one query per shard and one response per shard/chunk plane",
            "search_p1_p2": "not attempted",
            "locate_p0_d": "not integrated",
            "p4": "not attempted",
            "matched_go_lattigo_fabric": "unavailable"
        },
        "limitations": [
            "query-all-shards privacy is conditional, not established by passing correctness tests",
            "streamed plaintext packing and encoding repeat on every logical Fetch; no warmed or cached throughput claim",
            "the complete logical query/response bundle was not serialized",
            "aggregate communication is a checked derivation from representative measured ciphertext lengths",
            "no residual-noise-budget transcript or independent security-estimator output is available",
            "no fresh matched Go/Lattigo or Fabric run is available",
            "OpenFHE-rs is a WIP CXX wrapper pinned to a focused commit over OpenFHE 1.5.1",
            "Locate compatibility is at the RecordId type boundary only; P0-D, P1, P2, and P4 are outside this experiment",
            "the experiment does not establish efficiency, superiority, deployment behavior, or end-to-end completion"
        ]
    }))
}

fn main() -> Result<(), DynError> {
    let arguments = parse_arguments()?;
    fs::create_dir_all(&arguments.artifacts_dir)?;
    let selected = selected_cases(&arguments);
    if selected.is_empty() {
        return Err("no experiment cases selected".into());
    }
    let total_cases = selected.len();
    let mut cases = Vec::with_capacity(total_cases);
    for (position, case) in selected.into_iter().enumerate() {
        eprintln!(
            "scaled-case-start {}/{} N={} B={} K={}",
            position + 1,
            total_cases,
            case.database_records,
            case.payload_bytes,
            case.public_k
        );
        cases.push(run_case(&arguments, case)?);
        eprintln!(
            "scaled-case-complete {}/{} N={} B={} K={}",
            position + 1,
            total_cases,
            case.database_records,
            case.payload_bytes,
            case.public_k
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&build_report(&arguments, &cases)?)?
    );
    Ok(())
}
