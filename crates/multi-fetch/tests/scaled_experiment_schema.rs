use std::process::Command;

use serde_json::{Value, json};

#[test]
fn scaled_experiment_emits_complete_classified_case() {
    let artifacts = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_p3_scaled_experiment"))
        .args([
            "--database-records",
            "1024",
            "--payload-bytes",
            "256",
            "--public-k",
            "4",
            "--repetitions",
            "1",
            "--artifacts-dir",
        ])
        .arg(artifacts.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        report["schema"],
        "p3-sharded-payload-scaling-openfhe-bgv-v1"
    );
    assert_eq!(report["classification"], "measured-local-experiment");
    assert_eq!(report["dependencies"]["openfhe_reported_version"], "1.5.1");
    assert_eq!(report["invocation"]["filters_applied"], true);
    assert_eq!(report["cases"].as_array().unwrap().len(), 1);
    let case = &report["cases"][0];
    assert_eq!(case["database_records"], 1024);
    assert_eq!(case["payload_bytes"], 256);
    assert_eq!(case["public_k"], 4);
    assert_eq!(case["layout"]["shard_count"], 18);
    assert_eq!(case["layout"]["chunk_count"], 2);
    assert_eq!(case["operations"]["query_ciphertexts"], 18);
    assert_eq!(case["operations"]["response_ciphertexts"], 36);
    assert_eq!(case["operations"]["server_multiplications"], 36);
    assert_eq!(case["correctness"]["failed"], 0);
    assert_eq!(case["timings_ns"]["measured_repetitions"], 1);
    assert_eq!(
        case["communication_bytes"]["classification"],
        "derived-count-times-measured-ciphertext-length"
    );
    assert_eq!(
        case["serialization_bytes"]["artifact_files_retained"],
        false
    );
    assert_eq!(report["scope"]["p4"], "not attempted");
    assert_eq!(report["supported_payload_bytes"], json!([256, 1024]));
    assert_eq!(std::fs::read_dir(artifacts.path()).unwrap().count(), 0);
}
