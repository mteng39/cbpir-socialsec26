//! Local measurements; interpretation is explicit in the emitted report.
use crate::{Fixture, MatchBackend, P0dError, Pipeline, Run, fixtures};
use serde_json::{Value, json};
use std::{fs, process::Command};

pub const WRAPPER_COMMIT: &str = "3e6a46b1377e89e09414cbca7c0c0140c5338397";
pub const CORE_COMMIT: &str = "1306d14f8c26bb6150d3e6ad54f28dfe1007689e";

fn command(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "unavailable".into(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_owned(),
        )
}
fn environment() -> Value {
    let cpu = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("model name"))
        .unwrap_or("unavailable")
        .to_owned();
    json!({"rustc":command("rustc",&["--version"]),"cargo":command("cargo",&["--version"]),"kernel":command("uname",&["-srmo"]),
        "cmake":command("cmake",&["--version"]),"cxx":command("c++",&["--version"]),"cpu":cpu,
        "available_parallelism":std::thread::available_parallelism().map(usize::from).ok(),
        "omp_num_threads":std::env::var("OMP_NUM_THREADS").ok(),
        "openfhe_install_dir":std::env::var("OPENFHE_INSTALL_DIR").ok(),
        "meminfo":fs::read_to_string("/proc/meminfo").unwrap_or_default(),"argv":std::env::args().collect::<Vec<_>>()})
}
fn summary(mut values: Vec<u64>) -> Value {
    values.sort_unstable();
    let total: u128 = values.iter().copied().map(u128::from).sum();
    // Counts and measured nanoseconds in this bounded experiment fit f64.
    #[allow(clippy::cast_precision_loss)]
    let mean = total as f64 / values.len() as f64;
    json!({"minimum_ns":values[0],"median_ns":values[values.len()/2],"mean_ns":mean,"maximum_ns":values[values.len()-1]})
}
fn expected(f: &Fixture, r: &Run) -> bool {
    r.output.as_ref().map(|x| x.id.get()) == f.expected_id
        && r.exact_selection_agreement
        && r.semantic_selection_agreement != f.semantic_negative
}
fn performance(f: &Fixture, repetitions: usize, backend: MatchBackend) -> Result<Value, P0dError> {
    let mut p = Pipeline::prepare_with_match_backend(f.records.clone(), backend)?;
    let warmup = p.run(&f.request)?;
    let mut runs = Vec::new();
    for _ in 0..repetitions {
        runs.push(p.run(&f.request)?);
    }
    let stats = json!({"match":summary(runs.iter().map(|r|r.timings.match_ns).collect()),
        "locate":summary(runs.iter().map(|r|r.timings.locate_ns).collect()),"fetch":summary(runs.iter().map(|r|r.timings.fetch_ns).collect()),
        "request":summary(runs.iter().map(|r|r.timings.request_ns).collect()),"derived_online":summary(runs.iter().map(|r|r.timings.derived_online_sum_ns).collect())});
    Ok(
        json!({"fixture":f,"warmup_count":1,"warmup":warmup,"warmup_excluded_from_statistics":true,"preparation":p.preparation,
        "all_expected":expected(f,&warmup) && runs.iter().all(|r|expected(f,r)),"repetitions":runs,"statistics":stats}),
    )
}

/// Run the fixed local composition fixtures and retain every outcome.
///
/// # Panics
/// Panics if an internal performance fixture is removed without updating its name.
pub fn experiment(quick: bool) -> Value {
    experiment_with_match_backend(quick, MatchBackend::CtCt)
}

/// Runs the K=1 synthetic composition with an explicit P0-C backend.
///
/// # Panics
///
/// Panics if an internal performance fixture is removed without updating its name.
#[must_use]
pub fn experiment_with_match_backend(quick: bool, backend: MatchBackend) -> Value {
    let fixtures = fixtures();
    let mut cases = Vec::new();
    let mut errors = Vec::new();
    for f in &fixtures {
        let result = Pipeline::prepare_with_match_backend(f.records.clone(), backend)
            .and_then(|mut p| p.run(&f.request).map(|r| (p, r)));
        match result {
            Ok((p,r))=>cases.push(json!({"fixture":f,"catalog":p.catalog(),"preparation":p.preparation,"expected_observed":expected(f,&r),"run":r})),
            Err(e)=>errors.push(json!({"case":f.name,"error":e.to_string()})),
        }
    }
    let mut measured = Vec::new();
    for name in ["exact_first", "rfps_exact"] {
        let f = fixtures
            .iter()
            .find(|f| f.name == name)
            .expect("fixed performance fixture");
        match performance(f, if quick { 1 } else { 5 }, backend) {
            Ok(v) => measured.push(v),
            Err(e) => errors.push(json!({"performance":name,"error":e.to_string()})),
        }
    }
    let bgv = match Pipeline::prepare_with_match_backend(fixtures[0].records.clone(), backend)
        .and_then(|mut p| p.serialization_probe())
    {
        Ok(v) => v,
        Err(e) => {
            errors.push(json!({"serialization":"bgv","error":e.to_string()}));
            Value::Null
        }
    };
    let passed = errors.is_empty()
        && cases.iter().all(|c| c["expected_observed"] == true)
        && measured.iter().all(|m| m["all_expected"] == true);
    json!({"schema":"p0-d-composition-v1","classification":"measured-local-experiment","match_backend":backend,"bounded_fixture_gate_passed":passed,
        "software":{"openfhe_rs_commit":WRAPPER_COMMIT,"openfhe_commit":CORE_COMMIT},
        "authority":{"path":"docs/proposal/proposal_upd.pdf","sha256":"0f1a72d6fca44095cc8a24fd8447387e79f6f5b4e07901391e9b74701f6f4f87"},
        "environment":environment(),"bounds":{"public_k":1,"max_real_records":63,"frame_bytes":128,"max_payload_bytes":114,"max_text_bytes":1024,"max_pattern_bytes":64},
        "bgv_configuration":{"ring_dimension":8192,"record_slots":128,"plaintext_modulus":65537,"depth":1,"evaluation_keys":0},
        "cases":cases,"performance":measured,"serialization":{"bgv":bgv,"ckks":"separate hash-verified source snapshot probe; see ckks_sizes.json"},
        "errors":errors,"timing_definitions":{"request":"outer Match through exact Fetch; excludes independent P0-D oracle",
            "cold":"since_prepare_start_through_fetch_ns is measured on first request; includes any inter-call delay; runner calls immediately",
            "client_decrypt_idft":"P0-C component timer also includes exact reference scoring and decision instrumentation",
            "derived_online":"sum of measured subphases, includes P0-C reference instrumentation; no network transport"},
        "security_and_leakage":"Under honest-but-curious server and pinned CKKS/BGV construction and parameter assumptions, plaintext pattern and selected index are not sent to server. Client sees full decrypted scores and candidate IDs. Public layout, pattern length, K=1, sizes, timing, phase linkage and repeated-query behavior remain visible. This experiment is not a security proof.",
        "limitations":["11 small synthetic composition fixtures; no general correctness or scale feasibility claim",
            "RFPS cancellation and broad thresholds can select semantic mismatches; exact BGV reconstruction does not repair search decisions",
            "No independent security estimate, residual noise measurement, network deployment, or malicious-party protection",
            "Client, server, catalog and oracle coexist in one local harness; source-byte reconstruction verification is diagnostic work included in Fetch timing",
            "Dummy Fetch fixes retrieval count only; total cardinality and timing privacy are not proved",
            "CKKS approximation crossings are randomness-dependent; deterministic injected crossings are covered in pure tests"]})
}
