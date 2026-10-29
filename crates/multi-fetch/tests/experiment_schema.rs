use std::process::Command;

use serde_json::{Value, json};

#[test]
fn canonical_experiment_emits_classified_complete_json() {
    let directory = tempfile::tempdir().expect("temporary experiment artifacts");
    let output = Command::new(env!("CARGO_BIN_EXE_p3_multi_record_experiment"))
        .arg("--repetitions")
        .arg("1")
        .arg("--artifacts-dir")
        .arg(directory.path())
        .output()
        .expect("run canonical P3 experiment binary");
    assert!(
        output.status.success(),
        "experiment failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).expect("valid JSON report");
    assert_eq!(report["schema"], "p3-multi-record-openfhe-bgv-v1");
    assert_eq!(report["classification"], "measured-local-experiment");
    assert_eq!(report["supported_public_k"], json!([1, 4, 8, 16]));
    assert_eq!(
        report["dependencies"]["openfhe_rs_commit"],
        "3e6a46b1377e89e09414cbca7c0c0140c5338397"
    );
    assert_eq!(
        report["dependencies"]["openfhe_commit"],
        "1306d14f8c26bb6150d3e6ad54f28dfe1007689e"
    );
    assert_eq!(report["dependencies"]["openfhe_reported_version"], "1.5.1");
    assert_eq!(report["parameters"]["scheme"], "BGV-RNS");
    assert_eq!(report["parameters"]["actual_ring_dimension"], 8192);
    assert_eq!(report["parameters"]["plaintext_modulus"], 65_537);
    assert_eq!(report["packing"]["record_slots"], 128);
    assert_eq!(report["packing"]["physical_capacity_records"], 64);
    assert_eq!(report["operations"]["p3_query_ciphertexts"], 1);
    assert_eq!(report["operations"]["p3_response_ciphertexts"], 1);
    assert_eq!(report["operations"]["p3_server_multiplications"], 1);
    assert_eq!(report["operations"]["evaluation_keys_generated"], 0);
    assert_eq!(
        report["security_evidence"]["residual_noise_budget"],
        "unavailable"
    );
    assert_eq!(
        report["security_evidence"]["independent_security_estimator"],
        "unavailable"
    );

    let cases = report["cases"].as_array().expect("one case per K");
    assert_eq!(cases.len(), 4);
    for (case, expected_k) in cases.iter().zip([1_u64, 4, 8, 16]) {
        assert_eq!(case["public_k"], expected_k);
        assert_eq!(case["physical_records"], 64);
        assert_eq!(case["real_records"], 64 - expected_k);
        assert_eq!(case["correctness"]["failed"], 0);
        assert_eq!(case["correctness"]["differential_failed"], 0);
        assert!(case["correctness"]["passed"].as_u64().unwrap() > 0);
        assert!(case["serialization_bytes"]["query"].as_u64().unwrap() > 0);
        assert!(case["serialization_bytes"]["response"].as_u64().unwrap() > 0);
        assert_eq!(case["serialization_bytes"]["evaluation_key"], Value::Null);
        assert_eq!(
            case["serialization_bytes"]["artifact_files_retained"],
            false
        );
        assert_eq!(case["timings_ns"]["planning"]["samples"], 1);
        assert_eq!(case["timings_ns"]["p3_total_online"]["samples"], 1);
        assert_eq!(
            case["timings_ns"]["repeated_p0a_total_online"]["samples"],
            1
        );
    }
}
