use std::collections::HashSet;
use std::process::Command;

use serde_json::Value;

fn locked_version(crate_name: &str) -> &str {
    let mut current_name = None;
    for line in include_str!("../../../Cargo.lock").lines() {
        if line == "[[package]]" {
            current_name = None;
        } else if let Some(name) = line
            .strip_prefix("name = \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            current_name = Some(name);
        } else if current_name == Some(crate_name)
            && let Some(version) = line
                .strip_prefix("version = \"")
                .and_then(|value| value.strip_suffix('"'))
        {
            return version;
        }
    }
    panic!("{crate_name} missing from Cargo.lock");
}

fn classification_total(counts: &Value) -> u64 {
    counts
        .as_object()
        .expect("classification count object")
        .values()
        .map(|count| count.as_u64().expect("classification count"))
        .sum()
}

fn assert_timings(timings: &Value, label: &str) {
    for phase in [
        "context_and_key_generation",
        "client_setup",
        "client_query",
        "server_evaluation",
        "client_decrypt_idft",
    ] {
        assert!(
            timings[phase].as_u64().is_some_and(|value| value > 0),
            "missing {label}/{phase}"
        );
    }
}

fn assert_metrics(metrics: &Value, protocol: &str) {
    assert_eq!(metrics["scaling_technique"], "FIXEDMANUAL");
    assert_eq!(
        metrics["operations"]["ciphertext_constant_multiplications"],
        1
    );
    assert_eq!(
        metrics["operations"]["explicit_rescaling_operations"],
        usize::from(protocol == "fps")
    );
    if protocol == "rfps" {
        assert_eq!(metrics["scaling_modulus_bits"], 24);
    }
    assert_timings(&metrics["phase_timings_ns"], protocol);
}

fn assert_artifacts(sizes: &Value, protocol: &str, error_limit: f64) {
    for artifact in [
        "context_bytes",
        "public_key_bytes",
        "private_key_bytes",
        "eval_mult_key_bytes",
        "response_ciphertext_bytes",
    ] {
        assert!(
            sizes[artifact].as_u64().is_some_and(|value| value > 0),
            "empty {protocol}/{artifact}"
        );
    }
    assert!(
        !sizes["setup_ciphertext_bytes"]
            .as_array()
            .expect("setup size array")
            .is_empty()
    );
    assert!(
        !sizes["query_ciphertext_bytes"]
            .as_array()
            .expect("query size array")
            .is_empty()
    );
    assert!(
        sizes["reload_max_abs_error"]
            .as_f64()
            .is_some_and(|error| error.is_finite() && error <= error_limit),
        "{protocol} reload error"
    );
}

fn assert_software_and_configuration(report: &Value) {
    assert_eq!(report["schema"], "p0c-experiment-v3");
    assert_eq!(
        report["authority"]["path"],
        "docs/proposal/proposal_upd.pdf"
    );
    assert_eq!(
        report["authority"]["sha256"],
        "0f1a72d6fca44095cc8a24fd8447387e79f6f5b4e07901391e9b74701f6f4f87"
    );
    assert_eq!(
        report["workload_fixture"]["sha256"],
        "5c3ac1b262acd67f9b64e3147e5e6ba9e2c54b7bcc484caedabde4a1d775bd77"
    );
    assert_eq!(report["workload_fixture"]["synthetic_case_count"], 24);
    assert_eq!(
        report["workload_fixture"]["rfps_mismatch_trial_count"],
        16_384
    );
    assert_eq!(
        report["workload_fixture"]["rfps_true_match_control_count"],
        1_024
    );
    assert_eq!(report["workload_fixture"]["bgl_case_count"], 4);

    assert_eq!(report["software"]["openfhe_version"], "1.5.1");
    assert_eq!(
        report["software"]["openfhe_rs_commit"],
        "3e6a46b1377e89e09414cbca7c0c0140c5338397"
    );
    assert_eq!(
        report["software"]["openfhe_commit"],
        "1306d14f8c26bb6150d3e6ad54f28dfe1007689e"
    );
    assert_eq!(
        report["software"]["num_complex"],
        locked_version("num-complex")
    );
    assert_eq!(
        report["software"]["serde_json"],
        locked_version("serde_json")
    );
    assert_eq!(report["config"]["phase1_fixture_text_len"], 1024);
    assert_eq!(report["config"]["max_pattern_len"], 64);
    assert_eq!(report["config"]["phase1_fixture_transform_len"], 2048);
    assert_eq!(report["config"]["max_ckks_packed_slot_count"], 4096);
    assert_eq!(report["config"]["ckks_data_type"], "COMPLEX");
    assert_eq!(report["config"]["security_level"], "HEStd_128_classic");
    assert_eq!(
        report["config"]["feer_reported"]["rfps"]["encoding_scale_bits"],
        20
    );

    let fps = &report["config"]["openfhe_translation"]["fps"];
    assert_eq!(fps["multiplicative_depth"], 2);
    assert_eq!(fps["scaling_modulus_bits"], 40);
    assert_eq!(fps["first_modulus_bits"], 60);
    assert_eq!(fps["explicit_rescaling_operations"], 1);
    assert_eq!(fps["decision_threshold"], 0.5);

    let rfps = &report["config"]["openfhe_translation"]["rfps"];
    assert_eq!(rfps["multiplicative_depth"], 1);
    assert_eq!(rfps["initial_source_fidelity_scaling_modulus_bits"], 20);
    assert_eq!(rfps["scaling_modulus_bits"], 24);
    assert_eq!(rfps["first_modulus_bits"], 60);
    assert_eq!(rfps["explicit_rescaling_operations"], 0);
    assert_eq!(rfps["source_decision_threshold"], 4.0);
    assert_eq!(rfps["diagnostic_threshold"], 0.5);
    assert!(
        rfps["parameter_change_reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("rejected valid true matches"))
    );
}

fn assert_fixed_runs(report: &Value) {
    for protocol in ["fps", "rfps"] {
        let run = &report["runs"][protocol];
        let error_limit = if protocol == "fps" { 1e-3 } else { 0.02 };
        assert!(
            run["max_abs_error"]
                .as_f64()
                .is_some_and(|error| error <= error_limit)
        );
        assert!(!run["scores"].as_array().expect("score array").is_empty());
        assert_metrics(&run["metrics"], protocol);
    }
}

fn assert_case(case: &Value, ids: &mut HashSet<String>) {
    let id = case["id"].as_str().expect("case ID");
    assert!(ids.insert(id.to_owned()), "duplicate case ID {id}");
    let text_len = case["text_len"].as_u64().expect("text length");
    let pattern_len = case["pattern_len"].as_u64().expect("pattern length");
    let score_count = usize::try_from(text_len - pattern_len + 1).unwrap();
    for field in [
        "exact_fps_scores",
        "exact_rfps_scores",
        "approximate_fps_scores",
        "approximate_rfps_scores",
    ] {
        assert_eq!(
            case[field].as_array().expect("score array").len(),
            score_count,
            "{id}/{field}"
        );
    }
    assert!(
        case["approximate_fps_scores"]
            .as_array()
            .unwrap()
            .iter()
            .all(|score| score.as_f64().is_some_and(f64::is_finite))
    );
    assert!(
        case["approximate_rfps_scores"]
            .as_array()
            .unwrap()
            .iter()
            .all(|score| score.as_f64().is_some_and(f64::is_finite))
    );
    assert_eq!(case["fps_internal_exact_agreement"], true, "{id}");
    assert_eq!(case["fps_true_matches_preserved_at_0_5"], true, "{id}");
    assert_eq!(case["rfps_true_matches_preserved_at_0_5"], true, "{id}");
    assert!(
        case["fps_max_abs_error"]
            .as_f64()
            .is_some_and(|error| error.is_finite() && error >= 0.0)
    );
    assert!(
        case["rfps_max_abs_error"]
            .as_f64()
            .is_some_and(|error| error.is_finite() && error >= 0.0)
    );
    for regime in ["diagnostic_0_5", "source_4_0"] {
        let regime = &case["rfps_decision_regimes"][regime];
        assert_eq!(
            classification_total(&regime["classification_counts"]),
            u64::try_from(score_count).unwrap(),
            "{id}"
        );
        assert_eq!(regime["classified_score_count"], score_count);
        assert!(
            regime["accepted_mismatches"]
                .as_array()
                .unwrap()
                .iter()
                .all(|decision| decision["cause"].as_str().is_some())
        );
    }
    assert_metrics(&case["fps_metrics"], "fps");
    assert_metrics(&case["rfps_metrics"], "rfps");
}

fn assert_matrix_and_bgl(report: &Value) {
    let synthetic = report["synthetic_matrix"]
        .as_array()
        .expect("synthetic matrix");
    let bgl = report["bgl_cases"].as_array().expect("BGL cases");
    assert_eq!(synthetic.len(), 24);
    assert_eq!(bgl.len(), 4);
    let mut ids = HashSet::new();
    for case in synthetic.iter().chain(bgl) {
        assert_case(case, &mut ids);
    }
    assert_eq!(
        report["bgl_source"]["sha256"],
        "2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496"
    );
    assert_eq!(
        report["bgl_source"]["license_sha256"],
        "9ffa6ae259833cdc6e7ed8a0219fec72ef6455fb457b3c44096cb1ac1f0696d4"
    );
}

#[allow(clippy::too_many_lines)] // Keep all matched trial/control schema gates in one audit.
fn assert_probability(report: &Value) {
    let reports = report["rfps_probability"]
        .as_array()
        .expect("RFPS probability reports");
    assert_eq!(reports.len(), 4);
    let expected_batches = [2_u64, 4, 4, 8];
    let expected_zeros = [0_u64, 49, 34, 29];
    let expected_theory = [
        ("0", "180"),
        ("412", "32400"),
        ("47052", "5832000"),
        ("7573724", "1049760000"),
    ];
    let mut ids = HashSet::new();

    for (index, item) in reports.iter().enumerate() {
        let k = u64::try_from(index + 1).unwrap();
        assert_eq!(item["mismatch_count"], k);
        assert_eq!(item["fixture_trials"], 4096);
        assert_eq!(item["exact_zero_count"], expected_zeros[index]);
        assert_eq!(item["packed_batch_count"], expected_batches[index]);
        assert_eq!(
            item["packed_batch_count"],
            item["expected_packed_batch_count"]
        );
        assert_eq!(
            item["p0b_theory_reference"]["theoretical_zero_count"],
            expected_theory[index].0
        );
        assert_eq!(
            item["p0b_theory_reference"]["theoretical_total_count"],
            expected_theory[index].1
        );
        assert!(
            item["max_abs_error"]
                .as_f64()
                .is_some_and(|error| error.is_finite() && error <= 2.0)
        );
        assert_eq!(
            classification_total(&item["classification_counts_at_0_5"]),
            4096
        );
        assert_eq!(
            classification_total(&item["classification_counts_at_4_0"]),
            4096
        );
        assert_timings(&item["aggregate_phase_timings_ns"], "packed mismatch");
        let outcomes = item["trial_outcomes"].as_array().expect("trial outcomes");
        assert_eq!(outcomes.len(), 4096);
        for outcome in outcomes {
            let id = outcome["id"].as_str().expect("trial ID");
            assert!(ids.insert(id.to_owned()), "duplicate trial ID {id}");
            assert!(outcome["exact_score"].as_i64().is_some());
            assert!(
                outcome["approximate_score"]
                    .as_f64()
                    .is_some_and(f64::is_finite)
            );
            assert_eq!(outcome["window_matches"], false);
            assert!(outcome["cause_at_0_5"].as_str().is_some());
            assert!(outcome["cause_at_4_0"].as_str().is_some());
        }
        let batches = item["batches"].as_array().expect("packed batches");
        assert_eq!(
            u64::try_from(batches.len()).unwrap(),
            expected_batches[index]
        );
        for batch in batches {
            assert_eq!(batch["padded_slot_count"], 4096);
            assert!(batch["encoded_slot_count"].as_u64().unwrap() <= 4096);
            assert_metrics(&batch["metrics"], "rfps");
        }

        assert_eq!(item["true_match_control_count"], 256);
        assert_eq!(
            item["true_match_controls_in_probability_denominator"],
            false
        );
        assert_eq!(item["true_match_control_batch_count"], 1);
        assert_eq!(
            classification_total(&item["true_match_control_classification_counts_at_0_5"]),
            256
        );
        assert_eq!(
            item["true_match_control_classification_counts_at_0_5"]["true_match_accepted"],
            256
        );
        assert_eq!(
            classification_total(&item["true_match_control_classification_counts_at_4_0"]),
            256
        );
        assert_timings(
            &item["true_match_control_aggregate_phase_timings_ns"],
            "packed true control",
        );
        let controls = item["true_match_controls"]
            .as_array()
            .expect("true controls");
        assert_eq!(controls.len(), 256);
        for control in controls {
            let id = control["id"].as_str().expect("control ID");
            assert!(ids.insert(id.to_owned()), "duplicate control ID {id}");
            assert_eq!(control["exact_score"], 0);
            assert_eq!(control["window_matches"], true);
            assert!(
                control["approximate_score"]
                    .as_f64()
                    .is_some_and(f64::is_finite)
            );
        }
        let control_batches = item["true_match_control_batches"]
            .as_array()
            .expect("control batches");
        assert_eq!(control_batches.len(), 1);
        assert_metrics(&control_batches[0]["metrics"], "rfps");
    }
    assert_eq!(ids.len(), 17_408);
}

fn assert_serialization_boundary_and_limits(report: &Value) {
    let fixed = &report["serialization"]["fixed_vector"];
    assert_artifacts(&fixed["fps"], "fixed FPS", 1e-3);
    assert_artifacts(&fixed["rfps"], "fixed RFPS", 0.02);

    let maximum = &report["serialization"]["maximum_workload"];
    assert_eq!(maximum["case_id"], "synthetic-n1024-m64-exact");
    assert_eq!(maximum["text_len"], 1024);
    assert_eq!(maximum["pattern_len"], 64);
    assert_eq!(maximum["fps"]["reload_preserved_expected_positions"], true);
    assert_eq!(maximum["rfps"]["reload_preserved_expected_positions"], true);
    assert_artifacts(&maximum["fps"]["artifacts"], "maximum FPS", 2.0);
    assert_artifacts(&maximum["rfps"]["artifacts"], "maximum RFPS", 2.0);

    let boundary = &report["domain_boundary"];
    assert_eq!(boundary["configuration"]["text_len"], 1024);
    assert_eq!(boundary["configuration"]["pattern_len"], 64);
    assert_eq!(boundary["fps_true_match"]["decision_preserved"], true);
    assert_eq!(boundary["fps_max_mismatch"]["decision_preserved"], true);
    assert_eq!(boundary["rfps_true_match"]["decision_preserved"], true);
    assert_eq!(boundary["rfps_max_mismatch"]["decision_preserved"], true);
    assert_eq!(
        boundary["fps_max_mismatch"]["exact_score"].as_f64(),
        Some(4_161_600.0)
    );
    assert_eq!(
        boundary["rfps_max_mismatch"]["exact_score"].as_i64(),
        Some(522_240)
    );

    assert!(
        report["memory"]["process_peak_rss_kib"]
            .as_u64()
            .is_some_and(|rss| rss > 0)
    );
    assert_eq!(
        report["memory"]["definition"],
        "Linux VmHWM for experiment process"
    );
    assert!(
        report["effect_separation"]["ckks_threshold_crossings"]["observed_count"]
            .as_u64()
            .is_some()
    );
    assert!(
        report["failure_cases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|failure| failure["case"]
                .as_str()
                .is_some_and(|case| case.contains("ScalingModSize 20")))
    );
    assert!(
        report["limitations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| {
                item.as_str()
                    .is_some_and(|text| text.contains("uniform CKKS error bound"))
            })
    );
}

#[test]
fn experiment_emits_proposal_sized_reproducibility_and_separation_evidence() {
    let output = Command::new(env!("CARGO_BIN_EXE_p0c_experiment"))
        .arg("--quick")
        .output()
        .expect("experiment binary starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).expect("valid JSON report");
    assert_software_and_configuration(&report);
    assert_fixed_runs(&report);
    assert_matrix_and_bgl(&report);
    assert_probability(&report);
    assert_serialization_boundary_and_limits(&report);
}
