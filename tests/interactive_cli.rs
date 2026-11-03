#![cfg(feature = "native")]

use cbpir::{Cli, Command, RunArgs, SearchMode};
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
