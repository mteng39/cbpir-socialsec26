#![cfg(feature = "native")]

use cbpir::multi_report;

#[test]
fn quick_k4_report_covers_frozen_matrix_and_retains_stage_timings() {
    let report = multi_report::experiment(1);
    assert_eq!(report["schema"], "p0d-kgt1-synthetic-v1");
    assert_eq!(report["public_k"], 4);
    assert_eq!(report["case_count"], 14);
    assert_eq!(report["bounded_fixture_gate_passed"], true);
    assert_eq!(report["errors"].as_array().unwrap().len(), 0);

    for case in report["cases"].as_array().unwrap() {
        assert_eq!(case["warmup_count"], 1);
        assert_eq!(case["measured_repetition_count"], 1);
        assert_eq!(case["all_expected"], true, "{}", case["fixture"]["name"]);
        for stage in ["match", "locate", "fetch", "request"] {
            assert!(case["statistics_ns"][stage]["minimum_ns"].is_u64());
            assert!(case["statistics_ns"][stage]["maximum_ns"].is_u64());
        }
    }
}
