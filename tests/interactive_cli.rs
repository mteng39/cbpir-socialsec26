#![cfg(feature = "native")]

use cbpir::{Cli, Command, RunArgs, SearchMode, validate_run_args};
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
    let args = match cli.command.unwrap() {
        Command::Run(args) => args,
    };
    assert_eq!(args.dataset, "bgl");
    assert_eq!(args.warmups, 1);
    assert_eq!(args.trials, 5);
    assert_eq!(args.search_mode, SearchMode::Both);
}

#[test]
fn rejects_declared_constraint_violations() {
    let cases: Vec<(Box<dyn Fn(&mut RunArgs)>, &str)> = vec![
        (Box::new(|a| a.dataset = "other".into()), "dataset must be bgl"),
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
        (
            Box::new(|a| a.trials = 0),
            "trials must be positive",
        ),
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
