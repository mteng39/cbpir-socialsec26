use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use cbpir_bgv::{ArtifactPaths, BgvCpir, BgvCpirConfig};
use cbpir_fetch::{FetchPlan, P3BgvBackend, P3FetchResult, RecordId, SUPPORTED_PUBLIC_K};
use serde::Serialize;
use serde_json::{Value, json};

const OPENFHE_RS_COMMIT: &str = "3e6a46b1377e89e09414cbca7c0c0140c5338397";
const OPENFHE_COMMIT: &str = "1306d14f8c26bb6150d3e6ad54f28dfe1007689e";
const RECORD_SLOTS: usize = 128;
const DIFFERENTIAL_SEED: u64 = 0x7a5b_31d9_c4e2_806f;
const DIFFERENTIAL_CASES: usize = 8;

type DynError = Box<dyn Error>;

struct Arguments {
    repetitions: usize,
    artifacts_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct SampleStats {
    samples: usize,
    minimum: u64,
    mean: u64,
    maximum: u64,
}

struct RepeatedP0aRun {
    slots: Vec<u64>,
    query_ns: u64,
    server_ns: u64,
    decrypt_decode_ns: u64,
    total_ns: u64,
}

fn parse_arguments() -> Result<Arguments, DynError> {
    let mut repetitions = 20_usize;
    let mut artifacts_dir = None;
    let mut args = env::args_os().skip(1);
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--repetitions") => {
                let value = args.next().ok_or("--repetitions requires a value")?;
                repetitions = value.to_str().ok_or("repetitions must be UTF-8")?.parse()?;
                if repetitions == 0 {
                    return Err("repetitions must be positive".into());
                }
            }
            Some("--artifacts-dir") => {
                artifacts_dir = Some(PathBuf::from(
                    args.next().ok_or("--artifacts-dir requires a value")?,
                ));
            }
            Some(other) => return Err(format!("unknown argument: {other}").into()),
            None => return Err("arguments must be UTF-8".into()),
        }
    }

    Ok(Arguments {
        repetitions,
        artifacts_dir: artifacts_dir.ok_or("--artifacts-dir is required")?,
    })
}

fn deterministic_records(record_count: usize) -> Vec<Vec<u8>> {
    let mut records = (0..record_count)
        .map(|record_id| {
            let mut bytes = (0..RECORD_SLOTS)
                .map(|offset| {
                    u8::try_from((record_id * 131 + offset * 17 + offset / 7) % 251)
                        .expect("value is reduced modulo 251")
                })
                .collect::<Vec<_>>();
            bytes[0] = u8::try_from(record_id).expect("canonical record IDs fit one byte");
            bytes[1] = 0;
            bytes[RECORD_SLOTS / 2] = 0;
            bytes[RECORD_SLOTS - 1] = 0;
            bytes
        })
        .collect::<Vec<_>>();
    records[0] = vec![0; RECORD_SLOTS];
    records
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
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

fn verify_p3_result(
    backend: &P3BgvBackend,
    plan: &FetchPlan,
    source: &[Vec<u8>],
    result: &P3FetchResult,
) -> Result<(), DynError> {
    let expected_real_ids = plan
        .physical_ids()
        .iter()
        .zip(plan.is_dummy())
        .filter_map(|(&id, &is_dummy)| (!is_dummy).then_some(id))
        .collect::<Vec<_>>();
    if result.records.len() != expected_real_ids.len() {
        return Err(format!(
            "P3 returned {} real records; expected {}",
            result.records.len(),
            expected_real_ids.len()
        )
        .into());
    }
    for (fetched, expected_id) in result.records.iter().zip(expected_real_ids) {
        let index = usize::try_from(expected_id.0)?;
        if fetched.id() != expected_id || fetched.bytes() != source[index] {
            return Err(format!("P3 record mismatch for ID {}", expected_id.0).into());
        }
    }

    let mut expected_slots = vec![0_u64; backend.kernel().actual_ring_dimension()];
    for (&id, &is_dummy) in plan.physical_ids().iter().zip(plan.is_dummy()) {
        if is_dummy {
            continue;
        }
        let index = usize::try_from(id.0)?;
        let start = index * backend.kernel().record_slots();
        for (slot, &byte) in expected_slots[start..start + backend.kernel().record_slots()]
            .iter_mut()
            .zip(&source[index])
        {
            *slot = u64::from(byte);
        }
    }
    if result.decrypted_slots != expected_slots {
        return Err("P3 full-slot response differs from the selected window union".into());
    }
    Ok(())
}

fn fetch_candidates(
    backend: &mut P3BgvBackend,
    source: &[Vec<u8>],
    candidates: &[RecordId],
) -> Result<(FetchPlan, P3FetchResult), DynError> {
    let plan = backend.planner().plan(candidates)?;
    let result = backend.fetch(&plan)?;
    verify_p3_result(backend, &plan, source, &result)?;
    Ok((plan, result))
}

fn run_repeated_p0a(
    backend: &mut P3BgvBackend,
    plan: &FetchPlan,
    source: &[Vec<u8>],
) -> Result<RepeatedP0aRun, DynError> {
    let total_started = Instant::now();
    let mut query_ns = 0_u64;
    let mut server_ns = 0_u64;
    let mut decrypt_decode_ns = 0_u64;
    let mut combined_slots = vec![0_u64; backend.kernel().actual_ring_dimension()];

    for (&id, &is_dummy) in plan.physical_ids().iter().zip(plan.is_dummy()) {
        let index = usize::try_from(id.0)?;

        let phase_started = Instant::now();
        let query = backend.kernel().encrypt_query(index)?;
        query_ns = query_ns.saturating_add(elapsed_ns(phase_started));

        let phase_started = Instant::now();
        let response = backend.answer_p0a_query(&query);
        server_ns = server_ns.saturating_add(elapsed_ns(phase_started));

        let phase_started = Instant::now();
        let slots = backend.kernel().decrypt_slots(&response)?;
        let decoded =
            backend
                .kernel()
                .decode_fixed_window(&slots, index, backend.kernel().record_slots())?;
        decrypt_decode_ns = decrypt_decode_ns.saturating_add(elapsed_ns(phase_started));

        let expected = if is_dummy {
            vec![0_u8; backend.kernel().record_slots()]
        } else {
            source[index].clone()
        };
        if decoded != expected {
            return Err(format!("repeated P0-A mismatch for physical ID {}", id.0).into());
        }
        for (combined, slot) in combined_slots.iter_mut().zip(slots) {
            *combined = combined.saturating_add(slot);
        }
    }

    Ok(RepeatedP0aRun {
        slots: combined_slots,
        query_ns,
        server_ns,
        decrypt_decode_ns,
        total_ns: elapsed_ns(total_started),
    })
}

fn adjacent_ids(public_k: usize) -> Vec<RecordId> {
    (0..public_k)
        .map(|index| RecordId(u64::try_from(index).expect("supported K fits u64")))
        .collect()
}

fn non_adjacent_ids(real_record_count: usize, public_k: usize) -> Vec<RecordId> {
    if public_k == 1 {
        return vec![RecordId(
            u64::try_from(real_record_count / 2).expect("capacity fits u64"),
        )];
    }
    (0..public_k)
        .map(|offset| {
            let index = offset * (real_record_count - 1) / (public_k - 1);
            RecordId(u64::try_from(index).expect("capacity fits u64"))
        })
        .collect()
}

fn boundary_ids(real_record_count: usize, public_k: usize) -> Vec<RecordId> {
    if public_k == 1 {
        return vec![RecordId(
            u64::try_from(real_record_count - 1).expect("capacity fits u64"),
        )];
    }
    let mut ids = vec![
        RecordId(0),
        RecordId(u64::try_from(real_record_count - 1).expect("capacity fits u64")),
    ];
    ids.extend(
        (1..real_record_count - 1)
            .take(public_k - 2)
            .map(|index| RecordId(u64::try_from(index).expect("capacity fits u64"))),
    );
    ids
}

fn remove_serialized_artifacts(paths: &ArtifactPaths) -> Result<(), DynError> {
    for path in [
        &paths.context,
        &paths.public_key,
        &paths.private_key,
        &paths.query,
        &paths.response,
    ] {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "the experiment keeps the complete evidence protocol visible in one orchestration function"
)]
fn run_case(
    arguments: &Arguments,
    config: BgvCpirConfig,
    public_k: usize,
) -> Result<(Value, usize, usize), DynError> {
    let real_record_count = config.ring_dimension / config.record_slots - public_k;
    let source = deterministic_records(real_record_count);
    let (mut backend, setup) = P3BgvBackend::new(&source, public_k, config)?;
    let physical_capacity = backend.kernel().capacity();

    let all_ids = (0..real_record_count)
        .map(|index| RecordId(u64::try_from(index).expect("capacity fits u64")))
        .collect::<Vec<_>>();
    let final_record_start = real_record_count - 1;
    let mut exhaustive_batches = 0_usize;
    let mut exhaustive_records_passed = 0_usize;
    let mut dummy_padded_batch_seen = false;
    for candidates in all_ids[..final_record_start]
        .chunks(public_k)
        .chain(std::iter::once(&all_ids[final_record_start..]))
    {
        let (plan, result) = fetch_candidates(&mut backend, &source, candidates)?;
        exhaustive_batches += 1;
        exhaustive_records_passed += result.records.len();
        dummy_padded_batch_seen |= plan.is_dummy().iter().any(|&is_dummy| is_dummy);
    }
    if exhaustive_records_passed != real_record_count {
        return Err(format!(
            "K={public_k} exhaustive sweep passed {exhaustive_records_passed} records; expected {real_record_count}"
        )
        .into());
    }

    let adjacent = adjacent_ids(public_k);
    let _adjacent = fetch_candidates(&mut backend, &source, &adjacent)?;

    let non_adjacent = non_adjacent_ids(real_record_count, public_k);
    let _non_adjacent = fetch_candidates(&mut backend, &source, &non_adjacent)?;

    let boundary = boundary_ids(real_record_count, public_k);
    let _boundary = fetch_candidates(&mut backend, &source, &boundary)?;

    let mut duplicates = adjacent.clone();
    duplicates.insert(1, duplicates[0]);
    let (duplicate_plan, _) = fetch_candidates(&mut backend, &source, &duplicates)?;
    if duplicate_plan.selected_real_count() != public_k
        || duplicate_plan.truncated_unique_count() != 0
    {
        return Err(format!("K={public_k} duplicate handling changed plan semantics").into());
    }

    let (zero_plan, zero_result) = fetch_candidates(&mut backend, &source, &[])?;
    if !zero_result.records.is_empty() || zero_plan.is_dummy().iter().any(|&flag| !flag) {
        return Err(format!("K={public_k} zero-candidate plan was not all dummy").into());
    }
    dummy_padded_batch_seen = true;

    let truncation = (0..=public_k)
        .map(|index| RecordId(u64::try_from(index).expect("supported K fits u64")))
        .collect::<Vec<_>>();
    let (truncation_plan, _) = fetch_candidates(&mut backend, &source, &truncation)?;
    if truncation_plan.truncated_unique_count() != 1
        || truncation_plan.selected_real_count() != public_k
    {
        return Err(format!("K={public_k} truncation count was not one").into());
    }

    let mut state = DIFFERENTIAL_SEED ^ u64::try_from(public_k)?;
    let mut differential_passed = 0_usize;
    for _ in 0..DIFFERENTIAL_CASES {
        let candidates = (0..public_k + 3)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                RecordId(state % u64::try_from(real_record_count).expect("capacity fits u64"))
            })
            .collect::<Vec<_>>();
        let (plan, p3_result) = fetch_candidates(&mut backend, &source, &candidates)?;
        let repeated = run_repeated_p0a(&mut backend, &plan, &source)?;
        if p3_result.decrypted_slots != repeated.slots {
            return Err(format!("K={public_k} differential response mismatch").into());
        }
        differential_passed += 1;
    }

    let benchmark_candidates = non_adjacent_ids(real_record_count, public_k);
    let warmup_plan = backend.planner().plan(&benchmark_candidates)?;
    let warmup_p3 = backend.fetch(&warmup_plan)?;
    verify_p3_result(&backend, &warmup_plan, &source, &warmup_p3)?;
    let warmup_repeated = run_repeated_p0a(&mut backend, &warmup_plan, &source)?;
    if warmup_p3.decrypted_slots != warmup_repeated.slots {
        return Err(format!("K={public_k} warmup differential mismatch").into());
    }

    let mut planning_ns = Vec::with_capacity(arguments.repetitions);
    let mut p3_query_ns = Vec::with_capacity(arguments.repetitions);
    let mut p3_server_ns = Vec::with_capacity(arguments.repetitions);
    let mut p3_decrypt_decode_ns = Vec::with_capacity(arguments.repetitions);
    let mut p3_total_ns = Vec::with_capacity(arguments.repetitions);
    let mut p3_total_with_planning_ns = Vec::with_capacity(arguments.repetitions);
    let mut repeated_query_ns = Vec::with_capacity(arguments.repetitions);
    let mut repeated_server_ns = Vec::with_capacity(arguments.repetitions);
    let mut repeated_decrypt_decode_ns = Vec::with_capacity(arguments.repetitions);
    let mut repeated_total_ns = Vec::with_capacity(arguments.repetitions);

    for _ in 0..arguments.repetitions {
        let planning_started = Instant::now();
        let plan = backend.planner().plan(&benchmark_candidates)?;
        let planning_elapsed = elapsed_ns(planning_started);
        planning_ns.push(planning_elapsed);

        let p3_started = Instant::now();
        let p3_result = backend.fetch(&plan)?;
        let p3_elapsed = elapsed_ns(p3_started);
        verify_p3_result(&backend, &plan, &source, &p3_result)?;
        p3_query_ns.push(p3_result.timings.query_encode_encrypt_ns);
        p3_server_ns.push(p3_result.timings.server_multiply_ns);
        p3_decrypt_decode_ns.push(p3_result.timings.client_decrypt_decode_ns);
        p3_total_ns.push(p3_elapsed);
        p3_total_with_planning_ns.push(p3_elapsed.saturating_add(planning_elapsed));

        let repeated = run_repeated_p0a(&mut backend, &plan, &source)?;
        if p3_result.decrypted_slots != repeated.slots {
            return Err(format!("K={public_k} measured differential mismatch").into());
        }
        repeated_query_ns.push(repeated.query_ns);
        repeated_server_ns.push(repeated.server_ns);
        repeated_decrypt_decode_ns.push(repeated.decrypt_decode_ns);
        repeated_total_ns.push(repeated.total_ns);
    }

    let serialization_plan = backend.planner().plan(&boundary)?;
    let serialization_directory = arguments.artifacts_dir.join(format!("k-{public_k}"));
    let serialized = backend.fetch_and_serialize(&serialization_plan, &serialization_directory);
    let (serialized_result, bundle) = serialized?;
    let sizes = bundle.sizes;
    let round_trip = BgvCpir::decrypt_serialized_response(&bundle.paths);
    let cleanup = remove_serialized_artifacts(&bundle.paths);
    let round_trip_slots = round_trip?;
    cleanup?;
    if round_trip_slots != serialized_result.decrypted_slots {
        return Err(format!("K={public_k} serialized full-slot response changed").into());
    }
    let round_trip_records = backend.decode_plan_slots(&serialization_plan, &round_trip_slots)?;
    if round_trip_records != serialized_result.records {
        return Err(format!("K={public_k} serialized record reconstruction changed").into());
    }
    verify_p3_result(&backend, &serialization_plan, &source, &serialized_result)?;

    let repeated_query_bytes = sizes.query_bytes.saturating_mul(u64::try_from(public_k)?);
    let repeated_response_bytes = sizes
        .response_bytes
        .saturating_mul(u64::try_from(public_k)?);
    let passed = exhaustive_records_passed + 6 + differential_passed + 1;

    let report = json!({
        "public_k": public_k,
        "real_records": real_record_count,
        "dummy_records": public_k,
        "physical_records": physical_capacity,
        "setup_timings_ns": {
            "context_generation": setup.context_generation_ns,
            "key_generation": setup.key_generation_ns,
            "database_assembly_and_encode": setup.database_assembly_and_encode_ns,
        },
        "correctness": {
            "passed": passed,
            "failed": 0,
            "exhaustive_batches": exhaustive_batches,
            "exhaustive_real_records_tested": real_record_count,
            "exhaustive_real_records_passed": exhaustive_records_passed,
            "dummy_padded_batch_seen": dummy_padded_batch_seen,
            "adjacent_windows": true,
            "non_adjacent_windows": true,
            "boundary_windows": true,
            "stable_duplicate_elimination": true,
            "zero_candidate_all_dummy": true,
            "stable_truncation": true,
            "differential_seed": format!("0x{DIFFERENTIAL_SEED:016x}"),
            "differential_cases": DIFFERENTIAL_CASES,
            "differential_passed": differential_passed,
            "differential_failed": 0,
            "binary_serialization_round_trip": true,
            "full_slot_isolation_checked": true,
            "interior_and_trailing_zero_bytes_checked": true,
        },
        "serialization_bytes": {
            "context": sizes.context_bytes,
            "public_key": sizes.public_key_bytes,
            "private_key": sizes.private_key_bytes,
            "query": sizes.query_bytes,
            "response": sizes.response_bytes,
            "evaluation_key": sizes.evaluation_key_bytes,
            "artifact_files_retained": false,
            "retention_reason": "generated context/key/ciphertext files were deleted after fresh-object round-trip; measured lengths remain in this report",
        },
        "matched_repeated_p0a_communication_bytes": {
            "classification": "derived from measured single-ciphertext serialized lengths and public K",
            "query": repeated_query_bytes,
            "response": repeated_response_bytes,
        },
        "timings_ns": {
            "repetitions": arguments.repetitions,
            "warmup_repetitions": 1,
            "planning": timing_summary(&planning_ns),
            "p3_query_encode_encrypt": timing_summary(&p3_query_ns),
            "p3_server_ciphertext_plaintext_multiply": timing_summary(&p3_server_ns),
            "p3_client_decrypt_decode": timing_summary(&p3_decrypt_decode_ns),
            "p3_total_online": timing_summary(&p3_total_ns),
            "p3_total_with_planning": timing_summary(&p3_total_with_planning_ns),
            "repeated_p0a_query_encode_encrypt": timing_summary(&repeated_query_ns),
            "repeated_p0a_server_ciphertext_plaintext_multiply": timing_summary(&repeated_server_ns),
            "repeated_p0a_client_decrypt_decode": timing_summary(&repeated_decrypt_decode_ns),
            "repeated_p0a_total_online": timing_summary(&repeated_total_ns),
        },
    });

    Ok((
        report,
        backend.kernel().actual_ring_dimension(),
        physical_capacity,
    ))
}

#[allow(
    clippy::too_many_lines,
    reason = "the final JSON keeps evidence classifications and limitations explicit"
)]
fn main() -> Result<(), DynError> {
    let arguments = parse_arguments()?;
    fs::create_dir_all(&arguments.artifacts_dir)?;
    let config = BgvCpirConfig::prior_mini();

    let mut cases = Vec::with_capacity(SUPPORTED_PUBLIC_K.len());
    let mut actual_ring_dimension = None;
    let mut physical_capacity = None;
    for public_k in SUPPORTED_PUBLIC_K {
        let (case, observed_ring_dimension, observed_capacity) =
            run_case(&arguments, config, public_k)?;
        if actual_ring_dimension
            .replace(observed_ring_dimension)
            .is_some_and(|previous| previous != observed_ring_dimension)
        {
            return Err("OpenFHE changed ring dimension between K cases".into());
        }
        if physical_capacity
            .replace(observed_capacity)
            .is_some_and(|previous| previous != observed_capacity)
        {
            return Err("physical capacity changed between K cases".into());
        }
        cases.push(case);
    }

    let actual_ring_dimension = actual_ring_dimension.ok_or("no K cases executed")?;
    let physical_capacity = physical_capacity.ok_or("no K cases executed")?;
    let report = json!({
        "schema": "p3-multi-record-openfhe-bgv-v1",
        "classification": "measured-local-experiment",
        "supported_public_k": SUPPORTED_PUBLIC_K,
        "dependencies": {
            "openfhe_rs_commit": OPENFHE_RS_COMMIT,
            "openfhe_rs_crate_version": "0.3.2",
            "openfhe_commit": OPENFHE_COMMIT,
            "openfhe_reported_version": BgvCpir::openfhe_version(),
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
            "cargo_profile": "release",
        },
        "invocation": {
            "repetitions": arguments.repetitions,
            "warmup_repetitions_per_path_and_k": 1,
            "artifacts_directory": arguments.artifacts_dir.to_string_lossy(),
        },
        "parameters": {
            "scheme": "BGV-RNS",
            "security_setting": "HEStd_128_classic",
            "requested_ring_dimension": config.ring_dimension,
            "actual_ring_dimension": actual_ring_dimension,
            "plaintext_modulus": config.plaintext_modulus,
            "multiplicative_depth": config.multiplicative_depth,
            "ciphertext_modulus_chain": "OpenFHE-generated; unavailable through selected wrapper",
        },
        "packing": {
            "encoding": "one unsigned byte per packed SIMD slot",
            "record_slots": config.record_slots,
            "record_bytes": config.record_slots,
            "physical_capacity_records": physical_capacity,
            "real_capacity_formula": "physical_capacity_records - public_k",
            "dummy_windows": "public_k distinct zero windows reserved after the real prefix",
        },
        "selector": {
            "construction": "one full-ring binary vector; fill every slot in each selected fixed-width window with one",
            "lineage": "HLF-PIR described multi-hot full-window selector generalized to exactly public K distinct windows",
            "candidate_interface": "ordered proposal-level RecordId values from client Locate; validate, stable-deduplicate, truncate, then distinct-dummy pad",
        },
        "operations": {
            "p3_query_ciphertexts": 1,
            "p3_response_ciphertexts": 1,
            "p3_server_multiplications": 1,
            "p3_server_operation": "ciphertext-plaintext packed multiplication",
            "repeated_p0a_query_ciphertexts": "public_k",
            "repeated_p0a_response_ciphertexts": "public_k",
            "repeated_p0a_server_multiplications": "public_k",
            "ciphertext_ciphertext_multiplications": 0,
            "rotations": 0,
            "evaluation_keys_generated": 0,
            "bootstrapping": false,
        },
        "security_evidence": {
            "library_security_setting": "HEStd_128_classic",
            "independent_security_estimator": "unavailable",
            "residual_noise_budget": "unavailable",
            "correctness_evidence": "exact decrypt/decode sweeps and P3-vs-repeated-P0-A differential checks; not a residual-noise measurement",
            "query_privacy_scope": "conditional on OpenFHE BGV IND-CPA security, secret-key confidentiality, and honest generation of a well-formed selector",
            "malicious_selector_validation": "not provided server-side",
        },
        "methodology": {
            "database_generation": "deterministic fixed-width binary records; record zero is all-zero; other records contain interior and trailing zeros",
            "differential_seed": format!("0x{DIFFERENTIAL_SEED:016x}"),
            "differential_cases_per_k": DIFFERENTIAL_CASES,
            "comparison": "same process, HE context, keys, packed database, record width, physical IDs, and K; one P3 exchange versus exactly K repeated P0-A exchanges",
            "timing_clock": "std::time::Instant wall-clock nanoseconds",
            "serialization": "OpenFHE binary archive lengths measured from files; files deleted after fresh-object response decryption",
        },
        "cases": cases,
        "scope_limits": {
            "retained_configuration": "8192 slots, 128-byte records, public K in {1,4,8,16}, at most 64-K real records",
            "chapter_7_database_sweep": "not supported by this single-ring layout",
            "chapter_7_payloads_256_and_1024_bytes": "not supported by the retained 128-slot window",
            "p4": "not attempted",
        },
        "limitations": [
            "OpenFHE-rs is a WIP CXX wrapper; this prototype pins a focused unmerged fork to exact OpenFHE 1.5.1",
            "release-profile native compilation is required in this memory-constrained container",
            "no fresh matched Go/Lattigo or Fabric execution is available",
            "the server assumes an honestly generated binary selector and does not prove fixed public K",
            "library parameter labeling is not an independent security-estimator transcript",
            "exact decryption is not a residual-noise-budget measurement",
            "results do not establish proposal-scale database or payload feasibility",
            "P1/P2 Locate and P4 composition remain outside this experiment",
        ],
    });

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
