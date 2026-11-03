#![cfg(feature = "native")]

use cbpir::{
    Cli, Command, build_bgl_workload, run_interactive, validate_run_args,
};
use clap::Parser;

#[test]
#[ignore = "constructs native CKKS and BGV contexts"]
fn minimal_fps_run_reports_exact_search_and_fetch() {
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
        "fps",
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

    assert_eq!(report.measurements.raw.len(), 1);
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
