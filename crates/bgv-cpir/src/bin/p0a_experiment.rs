use cbpir_bgv::{BgvCpir, BgvCpirConfig};
use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

const OPENFHE_RS_COMMIT: &str = "3e6a46b1377e89e09414cbca7c0c0140c5338397";
const OPENFHE_COMMIT: &str = "1306d14f8c26bb6150d3e6ad54f28dfe1007689e";
const RECORD_SLOTS: usize = 128;

struct Arguments {
    repetitions: usize,
    artifacts_dir: PathBuf,
}

fn parse_arguments() -> Result<Arguments, Box<dyn Error>> {
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

fn deterministic_records(capacity: usize) -> Vec<Vec<u8>> {
    (0..capacity)
        .map(|index| {
            let mut record = if index == capacity / 2 {
                b"A\0B".to_vec()
            } else {
                format!("{{\"id\":{index},\"payload\":\"record-{index:02}\"}}").into_bytes()
            };
            record.resize(RECORD_SLOTS, 0);
            record
        })
        .collect()
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn timing_summary(values: &[u64]) -> Value {
    let total = values.iter().copied().map(u128::from).sum::<u128>();
    let mean = u64::try_from(total / values.len() as u128).unwrap_or(u64::MAX);
    json!({
        "minimum": values.iter().min().copied().unwrap_or(0),
        "mean": mean,
        "maximum": values.iter().max().copied().unwrap_or(0),
    })
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
    reason = "the experiment keeps its complete evidence schema visible in one orchestration function"
)]
fn main() -> Result<(), Box<dyn Error>> {
    let arguments = parse_arguments()?;
    fs::create_dir_all(&arguments.artifacts_dir)?;

    let config = BgvCpirConfig::prior_mini();
    let (kernel, setup_timings) = BgvCpir::new_with_setup_timings(config)?;
    let records = deterministic_records(kernel.capacity());

    let preprocessing_start = Instant::now();
    let mut database = kernel.prepare_database(&records)?;
    let preprocessing_ns = elapsed_ns(preprocessing_start);

    let mut passed_indices = 0_usize;
    for (index, expected_record) in records.iter().enumerate() {
        let query = kernel.encrypt_query(index)?;
        let response = kernel.answer(&mut database, &query);
        let slots = kernel.decrypt_slots(&response)?;
        if kernel.decode_fixed_window(&slots, index, RECORD_SLOTS)? == *expected_record {
            passed_indices += 1;
        }
    }

    let warmup_query = kernel.encrypt_query(0)?;
    let warmup_response = kernel.answer(&mut database, &warmup_query);
    let _warmup_slots = kernel.decrypt_slots(&warmup_response)?;

    let mut query_ns = Vec::with_capacity(arguments.repetitions);
    let mut server_ns = Vec::with_capacity(arguments.repetitions);
    let mut decrypt_decode_ns = Vec::with_capacity(arguments.repetitions);
    let mut final_exchange = None;

    for repetition in 0..arguments.repetitions {
        let index = repetition % kernel.capacity();
        let start = Instant::now();
        let query = kernel.encrypt_query(index)?;
        query_ns.push(elapsed_ns(start));

        let start = Instant::now();
        let response = kernel.answer(&mut database, &query);
        server_ns.push(elapsed_ns(start));

        let start = Instant::now();
        let slots = kernel.decrypt_slots(&response)?;
        let decoded = kernel.decode_fixed_window(&slots, index, RECORD_SLOTS)?;
        decrypt_decode_ns.push(elapsed_ns(start));
        if decoded != records[index] {
            return Err(format!("measured repetition failed at index {index}").into());
        }

        if repetition + 1 == arguments.repetitions {
            final_exchange = Some((index, query, response));
        }
    }

    let (serialized_index, query, response) = final_exchange.ok_or("no measured exchange")?;
    let serialization_start = Instant::now();
    let bundle = kernel.serialize_binary_bundle(&arguments.artifacts_dir, &query, &response)?;
    let serialization_ns = elapsed_ns(serialization_start);
    let deserialization_start = Instant::now();
    let round_trip_slots = BgvCpir::decrypt_serialized_response(&bundle.paths)?;
    let deserialization_decrypt_ns = elapsed_ns(deserialization_start);
    if kernel.decode_fixed_window(&round_trip_slots, serialized_index, RECORD_SLOTS)?
        != records[serialized_index]
    {
        return Err("serialized response round trip changed the record".into());
    }
    for sensitive_artifact in [
        &bundle.paths.context,
        &bundle.paths.public_key,
        &bundle.paths.private_key,
        &bundle.paths.query,
        &bundle.paths.response,
    ] {
        fs::remove_file(sensitive_artifact)?;
    }

    let report = json!({
        "schema": "p0-a-openfhe-bgv-v1",
        "classification": "measured-local-experiment",
        "dependencies": {
            "openfhe_rs_commit": OPENFHE_RS_COMMIT,
            "openfhe_rs_crate_version": "0.3.2",
            "openfhe_commit": OPENFHE_COMMIT,
            "openfhe_reported_version": BgvCpir::openfhe_version(),
        },
        "environment": {
            "rustc": command_first_line("rustc", &["--version"]),
            "cargo": command_first_line("cargo", &["--version"]),
            "cmake": command_first_line("cmake", &["--version"]),
            "cxx": command_first_line("c++", &["--version"]),
            "kernel": command_first_line("uname", &["-srmo"]),
            "cpu_model": cpu_model(),
            "available_parallelism": std::thread::available_parallelism().map(usize::from).ok(),
            "memory_total_kib": memory_total_kib(),
        },
        "parameters": {
            "scheme": "BGV-RNS",
            "security_setting": "HEStd_128_classic",
            "requested_ring_dimension": kernel.requested_ring_dimension(),
            "actual_ring_dimension": kernel.actual_ring_dimension(),
            "plaintext_modulus": kernel.plaintext_modulus(),
            "multiplicative_depth": config.multiplicative_depth,
            "ciphertext_modulus_chain": "OpenFHE-generated; unavailable through selected wrapper",
        },
        "packing": {
            "encoding": "one unsigned byte per packed SIMD slot",
            "record_slots": kernel.record_slots(),
            "capacity_records": kernel.capacity(),
            "logical_database_bytes": records.len() * RECORD_SLOTS,
            "occupied_slots": records.len() * RECORD_SLOTS,
        },
        "operations": {
            "ciphertext_plaintext_multiplications_per_query": 1,
            "ciphertext_ciphertext_multiplications_per_query": 0,
            "rotations_per_query": 0,
            "evaluation_keys_generated": 0,
            "bootstrapping": false,
        },
        "correctness": {
            "tested_indices": kernel.capacity(),
            "passed_indices": passed_indices,
            "boundary_indices_included": [0, kernel.capacity() - 1],
            "interior_zero_fixed_decode_tested": true,
            "binary_serialization_round_trip": true,
            "residual_noise_budget": "unavailable",
            "noise_evidence_interpretation": "Exact decryption sweeps are correctness evidence, not a measured residual noise budget",
        },
        "serialization_bytes": {
            "context": bundle.sizes.context_bytes,
            "public_key": bundle.sizes.public_key_bytes,
            "private_key": bundle.sizes.private_key_bytes,
            "query": bundle.sizes.query_bytes,
            "response": bundle.sizes.response_bytes,
            "evaluation_key": bundle.sizes.evaluation_key_bytes,
            "artifact_files_retained": false,
            "retention_reason": "generated key/ciphertext artifacts were deleted after measured round-trip; sizes remain in this report"
        },
        "timings_ns": {
            "repetitions": arguments.repetitions,
            "warmup_repetitions": 1,
            "context_generation": setup_timings.context_generation_ns,
            "key_generation": setup_timings.key_generation_ns,
            "database_pack_encode": preprocessing_ns,
            "query_encode_encrypt": timing_summary(&query_ns),
            "server_ciphertext_plaintext_multiply": timing_summary(&server_ns),
            "client_decrypt_decode": timing_summary(&decrypt_decode_ns),
            "serialize_bundle": serialization_ns,
            "deserialize_and_decrypt_response": deserialization_decrypt_ns,
        },
        "comparison_scope": {
            "matched_to_prior_go": [
                "ring dimension 8192",
                "plaintext modulus 65537",
                "64 records",
                "128 slots per record",
                "multi-hot window selector",
                "one ciphertext-plaintext multiplication"
            ],
            "not_matched_to_prior_go": [
                "ciphertext RNS modulus chain",
                "HE implementation",
                "compiler/runtime environment"
            ]
        },
        "limitations": [
            "OpenFHE-rs is a WIP CXX wrapper; this prototype pins a focused unmerged fork to exact OpenFHE 1.5.1",
            "debug-profile wrapper compilation exceeded this container memory limit; release profile is required here",
            "no fresh Go/Fabric execution is available in this environment",
            "legacy zero-terminated decode cannot represent arbitrary binary records",
            "selector well-formedness is assumed under the proposal's honest-client scope"
        ]
    });

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
