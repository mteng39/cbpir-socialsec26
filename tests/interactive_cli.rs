#![cfg(feature = "native")]
#![allow(
    clippy::float_cmp,
    reason = "Fixtures use exactly representable values to test specified statistics"
)]

use cbpir::{
    Cli, Command, InteractiveReport, ParameterReport, RunArgs, SearchMode, build_bgl_workload,
    resize_by_repetition, summarize_ns, validate_run_args,
};
use clap::Parser;

fn valid_args() -> [&'static str; 20] {
    [
        "cbpir",
        "run",
        "--num-records",
        "128",
        "--searchable-bytes",
        "24",
        "--payload-bytes",
        "32",
        "--record-slots",
        "64",
        "--capacity",
        "16",
        "--pattern-length",
        "4",
        "--search-mode",
        "fps",
        "--warmups",
        "0",
        "--trials",
        "1",
    ]
}

fn valid_run_args() -> RunArgs {
    let cli = Cli::try_parse_from(valid_args()).unwrap();
    match cli.command.unwrap() {
        Command::Run(args) => args,
    }
}

#[test]
fn parses_frozen_run_interface() {
    let args = valid_run_args();
    assert_eq!(args.num_records, 128);
    assert_eq!(args.searchable_bytes, 24);
    assert_eq!(args.payload_bytes, 32);
    assert_eq!(args.record_slots, 64);
    assert_eq!(args.capacity, 16);
    assert_eq!(args.pattern_length, 4);
    assert_eq!(args.search_mode, SearchMode::Fps);
    assert_eq!(args.warmups, 0);
    assert_eq!(args.trials, 1);
}

#[test]
fn defaults_dataset_warmups_and_trials() {
    let cli = Cli::try_parse_from([
        "cbpir",
        "run",
        "--num-records",
        "128",
        "--searchable-bytes",
        "24",
        "--payload-bytes",
        "32",
        "--record-slots",
        "64",
        "--capacity",
        "16",
        "--pattern-length",
        "4",
        "--search-mode",
        "both",
    ])
    .unwrap();
    let Command::Run(args) = cli.command.unwrap();
    assert_eq!(args.dataset, "bgl");
    assert_eq!(args.warmups, 1);
    assert_eq!(args.trials, 5);
    assert_eq!(args.search_mode, SearchMode::Both);
}

#[test]
fn rejects_declared_constraint_violations() {
    type InvalidCase = (Box<dyn Fn(&mut RunArgs)>, &'static str);
    let cases: Vec<InvalidCase> = vec![
        (
            Box::new(|a| a.dataset = "other".into()),
            "dataset must be bgl",
        ),
        (
            Box::new(|a| a.num_records = 64),
            "num-records must be one of 128,256,512,1024",
        ),
        (
            Box::new(|a| a.capacity = 2),
            "capacity must be one of 1,4,8,16",
        ),
        (
            Box::new(|a| a.pattern_length = 2),
            "pattern-length must be one of 1,4,8,16",
        ),
        (
            Box::new(|a| a.record_slots = 32),
            "record-slots must be one of 64,128,224,256,384,512",
        ),
        (
            Box::new(|a| a.searchable_bytes = 0),
            "searchable-bytes must be positive",
        ),
        (
            Box::new(|a| a.searchable_bytes = 1),
            "pattern-length must not exceed searchable-bytes",
        ),
        (
            Box::new(|a| a.payload_bytes = 0),
            "payload-bytes must be positive",
        ),
        (
            Box::new(|a| a.payload_bytes = 115),
            "payload-bytes must not exceed 114",
        ),
        (
            Box::new(|a| {
                a.payload_bytes = 60;
                a.record_slots = 64;
            }),
            "14 + payload-bytes must not exceed record-slots",
        ),
        (Box::new(|a| a.trials = 0), "trials must be positive"),
        (
            Box::new(|a| {
                a.num_records = 1024;
                a.searchable_bytes = 65_536;
            }),
            "Search transform length 134217728 exceeds maximum 32768 complex slots",
        ),
        (
            Box::new(|a| {
                a.num_records = 1024;
                a.record_slots = 512;
            }),
            "Fetch requires 524288 slots but maximum supported BGV ring is 131072",
        ),
    ];
    for (change, expected) in cases {
        let mut args = valid_run_args();
        change(&mut args);
        assert_eq!(validate_run_args(&args).unwrap_err().to_string(), expected);
    }
}

#[test]
fn selects_smallest_supported_search_and_fetch_rings() {
    let config = validate_run_args(&valid_run_args()).unwrap();
    assert_eq!(config.text_bytes, 128 * 24 + 127);
    assert_eq!(config.transform_len, 4096);
    assert_eq!(config.ckks_ring_dimension, 8192);
    assert_eq!(config.bgv_ring_dimension, 8192);
}

#[test]
fn repeat_resize_has_exact_deterministic_size() {
    assert_eq!(resize_by_repetition(b"abc", 2).unwrap(), b"ab");
    assert_eq!(resize_by_repetition(b"abc", 8).unwrap(), b"abcabcab");
    assert!(resize_by_repetition(b"", 1).is_err());
}

#[test]
fn generated_bgl_workload_is_deterministic_and_record_bounded() {
    let config = validate_run_args(&valid_run_args()).unwrap();
    let first = build_bgl_workload(&config).unwrap();
    let second = build_bgl_workload(&config).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first.source_sha256,
        "2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496"
    );
    assert_eq!(first.source_line_count, 2000);
    assert_eq!(first.eligible_record_count, 1331);
    assert_eq!(first.records.len(), config.num_records);
    assert!(first.records.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert!(first.records.iter().all(|record| {
        record.searchable.len() == config.searchable_bytes
            && record.payload.len() == config.payload_bytes
    }));
    assert_eq!(first.pattern.bytes.len(), config.pattern_length);
    assert!(!first.pattern.literal_record_ids.is_empty());
    assert_eq!(
        first.pattern.literal_record_ids.len(),
        first.pattern.support
    );
    assert!(first.records.iter().any(|record| {
        record
            .searchable
            .windows(config.pattern_length)
            .any(|window| window == first.pattern.bytes)
    }));
    assert!(
        first
            .pattern
            .literal_record_ids
            .iter()
            .all(|id| first.records.iter().any(|record| record.id == *id))
    );
}

#[test]
fn summary_uses_population_deviation_and_nearest_rank_p95() {
    let summary = summarize_ns(&[10, 20, 30, 40]).unwrap();
    assert_eq!(summary.mean_ns, 25.0);
    assert_eq!(summary.median_ns, 25.0);
    assert!((summary.standard_deviation_ns - 125_f64.sqrt()).abs() < 1e-12);
    assert_eq!(summary.p95_ns, 40);
    assert!(summarize_ns(&[]).is_err());
}

#[test]
fn count_summary_uses_count_units() {
    let summary = cbpir::summarize_counts(&[1, 3, 5]).unwrap();
    assert_eq!(summary.mean, 3.0);
    assert_eq!(summary.median, 3.0);
    assert_eq!(summary.p95, 5);
}

#[test]
fn correctness_summary_covers_every_trial() {
    let row = cbpir::CorrectnessReport {
        measured_trials: 1,
        candidate_count: 3,
        selected_count: 1,
        exact_fetch_reconstruction: true,
        selected_windows_exact: true,
        unselected_windows_zero: true,
        unused_tail_zero: true,
        fps_oracle_disagreements: 0,
        rfps_threshold_disagreements: 2,
        rfps_intrinsic_cancellations: 1,
        rfps_threshold_only: 0,
    };
    let mut second = row.clone();
    second.exact_fetch_reconstruction = false;
    let result = cbpir::aggregate_correctness(&[row, second]).unwrap();
    assert_eq!(result.measured_trials, 2);
    assert_eq!(result.candidate_count, 6);
    assert_eq!(result.rfps_threshold_disagreements, 4);
    assert!(!result.exact_fetch_reconstruction);
}

#[test]
fn every_supported_ring_boundary_is_selected_without_native_setup() {
    for (bytes, ring) in [(31, 8192), (32, 16384), (64, 32768), (128, 65536)] {
        let mut args = valid_run_args();
        args.searchable_bytes = bytes;
        assert_eq!(validate_run_args(&args).unwrap().ckks_ring_dimension, ring);
    }
    for (slots, ring) in [
        (64, 8192),
        (128, 16384),
        (224, 32768),
        (256, 32768),
        (384, 65536),
        (512, 65536),
    ] {
        let mut args = valid_run_args();
        args.record_slots = slots;
        assert_eq!(validate_run_args(&args).unwrap().bgv_ring_dimension, ring);
    }
}

#[test]
fn generated_pattern_has_nearest_available_record_support() {
    use std::collections::{BTreeMap, BTreeSet};
    let config = validate_run_args(&valid_run_args()).unwrap();
    let workload = build_bgl_workload(&config).unwrap();
    let mut support = BTreeMap::<Vec<u8>, BTreeSet<u64>>::new();
    for record in &workload.records {
        for window in record.searchable.windows(config.pattern_length) {
            support
                .entry(window.to_vec())
                .or_default()
                .insert(record.id);
        }
    }
    let distance = support
        .values()
        .map(|ids| ids.len().abs_diff(config.capacity))
        .min()
        .unwrap();
    assert_eq!(workload.pattern.support.abs_diff(config.capacity), distance);
}

#[test]
fn report_has_frozen_top_level_schema() {
    let configuration = validate_run_args(&valid_run_args()).unwrap();
    let workload = build_bgl_workload(&configuration).unwrap();
    let parameters = ParameterReport::for_config(&configuration, "test-openfhe");
    let value =
        serde_json::to_value(InteractiveReport::new(configuration, workload, parameters)).unwrap();
    assert_eq!(value["schema"], "cbpir-interactive-v1");
    let object = value.as_object().unwrap();
    let expected = [
        "schema",
        "configuration",
        "workload",
        "parameters",
        "setup",
        "measurements",
        "communication",
        "correctness",
    ];
    assert_eq!(object.len(), expected.len());
    assert!(expected.iter().all(|key| object.contains_key(*key)));
}
