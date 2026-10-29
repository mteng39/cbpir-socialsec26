#![cfg(feature = "openfhe-backend")]

use serde_json::Value;
use std::process::Command;

#[test]
fn prototype_p0_a_experiment_emits_evidence_classified_json() {
    let directory = tempfile::tempdir().expect("temporary experiment artifacts");
    let output = Command::new(env!("CARGO_BIN_EXE_p0a_experiment"))
        .arg("--repetitions")
        .arg("1")
        .arg("--artifacts-dir")
        .arg(directory.path())
        .output()
        .expect("run P0-A experiment binary");
    assert!(
        output.status.success(),
        "experiment failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).expect("valid JSON report");
    assert_eq!(report["schema"], "p0-a-openfhe-bgv-v1");
    assert_eq!(report["classification"], "measured-local-experiment");
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
    assert_eq!(report["parameters"]["plaintext_modulus"], 65_537);
    assert_eq!(report["parameters"]["actual_ring_dimension"], 8192);
    assert_eq!(report["packing"]["record_slots"], 128);
    assert_eq!(report["packing"]["capacity_records"], 64);
    assert_eq!(report["correctness"]["passed_indices"], 64);
    assert_eq!(report["correctness"]["tested_indices"], 64);
    assert_eq!(
        report["correctness"]["residual_noise_budget"],
        "unavailable"
    );
    assert_eq!(
        report["operations"]["ciphertext_plaintext_multiplications_per_query"],
        1
    );
    assert_eq!(report["operations"]["evaluation_keys_generated"], 0);
    assert!(report["serialization_bytes"]["query"].as_u64().unwrap() > 0);
    assert!(report["serialization_bytes"]["response"].as_u64().unwrap() > 0);
    assert_eq!(report["timings_ns"]["repetitions"], 1);
}
