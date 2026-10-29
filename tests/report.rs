#![cfg(feature = "native")]
use serde_json::Value;
use std::process::Command;

#[test]
fn experiment_reports_real_runs_and_explicit_negative_results() {
    let output = Command::new(env!("CARGO_BIN_EXE_cbpir"))
        .arg("--quick")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let v: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(v["schema"], "p0-d-composition-v1");
    assert_eq!(v["bounds"]["public_k"], 1);
    let cases = v["cases"].as_array().unwrap();
    assert!(cases.len() >= 11);
    for case in cases {
        assert_eq!(case["run"]["fetch_count"], 1);
        assert_eq!(case["run"]["ckks_metrics"]["openfhe_version"], "1.5.1");
        assert!(case["run"]["timings"]["request_ns"].as_u64().unwrap() > 0);
    }
    assert!(
        cases
            .iter()
            .any(|c| c["fixture"]["name"] == "rfps_cancellation"
                && c["run"]["semantic_selection_agreement"] == false)
    );
    assert!(
        cases
            .iter()
            .any(|c| c["fixture"]["name"] == "rfps_threshold_only"
                && c["run"]["semantic_selection_agreement"] == false)
    );
    assert!(v["serialization"]["bgv"]["query_bytes"].as_u64().unwrap() > 0);
    assert_eq!(v["performance"][0]["warmup_count"], 1);
    assert_eq!(
        v["performance"][0]["repetitions"].as_array().unwrap().len(),
        1
    );
    assert!(!v["limitations"].as_array().unwrap().is_empty());
    let invalid = Command::new(env!("CARGO_BIN_EXE_cbpir"))
        .arg("--nonsense")
        .output()
        .unwrap();
    assert!(!invalid.status.success());
}
