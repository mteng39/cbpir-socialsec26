#![cfg(feature = "native")]
#![allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    reason = "Check exact JSON equality with the same floating-point representation of measured nanoseconds"
)]

use cbpir::{Cli, Command, build_bgl_workload, run_interactive, validate_run_args};
use clap::Parser;

#[test]
fn generated_help_matches_readme() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cbpir"))
        .args(["run", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    let normalized = help
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(include_str!("../README.md").contains(&normalized));
}

#[test]
#[ignore = "runs native CLI for every supported retrieval capacity"]
fn all_capacities_preserve_output_bytes_and_online_statistics() {
    for capacity in [1, 4, 8, 16] {
        let directory = tempfile::tempdir().unwrap();
        let output_path = directory.path().join("report.json");
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_cbpir"))
            .args([
                "run",
                "--num-records",
                "128",
                "--searchable-bytes",
                "16",
                "--payload-bytes",
                "16",
                "--record-slots",
                "64",
                "--capacity",
                &capacity.to_string(),
                "--pattern-length",
                "4",
                "--search-mode",
                "both",
                "--warmups",
                "1",
                "--trials",
                "3",
                "--output",
            ])
            .arg(&output_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, std::fs::read(output_path).unwrap());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["measurements"]["raw"].as_array().unwrap().len(), 6);
        for mode in ["fps", "rfps"] {
            let rows = report["measurements"]["raw"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["mode"] == mode)
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 3);
            for row in &rows {
                assert!(row["selected_count"].as_u64().unwrap() <= capacity);
                assert_eq!(row["correctness"]["exact_fetch_reconstruction"], true);
            }
            let expected = ["search_ns", "locate_ns", "fetch_ns"]
                .iter()
                .map(|key| {
                    let mut values = rows
                        .iter()
                        .map(|row| row["durations"][key].as_u64().unwrap())
                        .collect::<Vec<_>>();
                    values.sort_unstable();
                    values[1] as f64
                })
                .sum::<f64>();
            assert_eq!(
                report["measurements"]["summary"][mode]["r1_3_sum_of_stage_medians_ns"]
                    .as_f64()
                    .unwrap(),
                expected
            );
            assert_eq!(report["correctness"][mode]["measured_trials"], 3);
        }
    }
}

#[test]
#[ignore = "constructs native CKKS and BGV contexts"]
fn both_modes_run_sequentially_and_reconstruct_exact_payloads() {
    let cli = Cli::try_parse_from([
        "cbpir",
        "run",
        "--num-records",
        "128",
        "--searchable-bytes",
        "16",
        "--payload-bytes",
        "16",
        "--record-slots",
        "64",
        "--capacity",
        "1",
        "--pattern-length",
        "1",
        "--search-mode",
        "both",
        "--warmups",
        "0",
        "--trials",
        "1",
    ])
    .unwrap();
    let config = match cli.command.unwrap() {
        Command::Run(args) => validate_run_args(&args).unwrap(),
    };
    let workload = build_bgl_workload(&config).unwrap();
    let report = run_interactive(config, workload).unwrap();

    assert_eq!(report.measurements.raw.len(), 2);
    assert_eq!(report.measurements.raw[1].mode, "rfps");
    assert!(report.correctness["rfps"].exact_fetch_reconstruction);
    let raw = &report.measurements.raw[0];
    assert_eq!(raw.mode, "fps");
    assert!(raw.durations.search_query_ns > 0);
    assert!(raw.durations.search_response_ns > 0);
    assert!(raw.durations.fetch_query_ns > 0);
    assert!(raw.durations.fetch_response_ns > 0);
    assert!(raw.selected_count <= 1);

    let correctness = &report.correctness["fps"];
    assert_eq!(correctness.fps_oracle_disagreements, 0);
    assert!(correctness.exact_fetch_reconstruction);
    assert!(correctness.selected_windows_exact);
    assert!(correctness.unselected_windows_zero);
    assert!(correctness.unused_tail_zero);

    let communication = &report.communication["fps"];
    assert_eq!(communication.search_query_ciphertext_bytes.len(), 2);
    assert!(communication.search_response_bytes > 0);
    assert!(communication.fetch_query_bytes > 0);
    assert!(communication.fetch_response_bytes > 0);
}
