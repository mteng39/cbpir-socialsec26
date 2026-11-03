use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::path::PathBuf;

pub const RECORD_COUNTS: [usize; 4] = [128, 256, 512, 1024];
pub const CAPACITIES: [usize; 4] = [1, 4, 8, 16];
pub const PATTERN_LENGTHS: [usize; 4] = [1, 4, 8, 16];
pub const RECORD_SLOTS: [usize; 6] = [64, 128, 224, 256, 384, 512];
pub const CKKS_RINGS: [usize; 4] = [8192, 16_384, 32_768, 65_536];
pub const BGV_RINGS: [usize; 5] = [8192, 16_384, 32_768, 65_536, 131_072];

#[derive(Debug, Parser)]
#[command(name = "cbpir", disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[arg(long)]
    pub quick: bool,
    #[arg(long, default_value = "ct_ct")]
    pub match_backend: String,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Run(RunArgs),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[value(rename_all = "lower")]
pub enum SearchMode {
    Fps,
    Rfps,
    Both,
}

#[derive(Clone, Debug, Args)]
pub struct RunArgs {
    #[arg(long, default_value = "bgl")]
    pub dataset: String,
    #[arg(long)]
    pub num_records: usize,
    #[arg(long)]
    pub searchable_bytes: usize,
    #[arg(long)]
    pub payload_bytes: usize,
    #[arg(long)]
    pub record_slots: usize,
    #[arg(long)]
    pub capacity: usize,
    #[arg(long)]
    pub pattern_length: usize,
    #[arg(long, value_enum)]
    pub search_mode: SearchMode,
    #[arg(long, default_value_t = 1)]
    pub warmups: usize,
    #[arg(long, default_value_t = 5)]
    pub trials: usize,
    #[arg(long)]
    pub output: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CliConfig {
    pub num_records: usize,
    pub searchable_bytes: usize,
    pub payload_bytes: usize,
    pub record_slots: usize,
    pub capacity: usize,
    pub pattern_length: usize,
    pub search_mode: SearchMode,
    pub warmups: usize,
    pub trials: usize,
    pub text_bytes: usize,
    pub transform_len: usize,
    pub ckks_ring_dimension: usize,
    pub bgv_ring_dimension: usize,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
#[error("{0}")]
pub struct ConfigError(pub String);

fn require_member(value: usize, allowed: &[usize], label: &str) -> Result<(), ConfigError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(ConfigError(format!(
            "{label} {value} is invalid; expected {}",
            allowed
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )))
    }
}

pub fn validate_run_args(args: &RunArgs) -> Result<CliConfig, ConfigError> {
    if args.dataset != "bgl" {
        return Err(ConfigError(format!(
            "dataset '{}' is invalid; expected bgl",
            args.dataset
        )));
    }
    require_member(args.num_records, &RECORD_COUNTS, "num-records")?;
    require_member(args.capacity, &CAPACITIES, "capacity")?;
    require_member(args.pattern_length, &PATTERN_LENGTHS, "pattern-length")?;
    require_member(args.record_slots, &RECORD_SLOTS, "record-slots")?;
    if args.searchable_bytes == 0 {
        return Err(ConfigError(
            "searchable-bytes 0 is invalid; expected a positive value".into(),
        ));
    }
    if args.pattern_length > args.searchable_bytes {
        return Err(ConfigError(format!(
            "pattern-length {} exceeds searchable-bytes {}",
            args.pattern_length, args.searchable_bytes
        )));
    }
    if args.payload_bytes == 0 {
        return Err(ConfigError(
            "payload-bytes 0 is invalid; expected 1–114".into(),
        ));
    }
    if args.payload_bytes > 114 {
        return Err(ConfigError(format!(
            "payload-bytes {} is invalid; expected 1–114",
            args.payload_bytes
        )));
    }
    if 14_usize
        .checked_add(args.payload_bytes)
        .is_none_or(|required| required > args.record_slots)
    {
        return Err(ConfigError(format!(
            "14 + payload-bytes {} exceeds record-slots {}",
            args.payload_bytes, args.record_slots
        )));
    }
    if args.trials == 0 {
        return Err(ConfigError(
            "trials 0 is invalid; expected a positive value".into(),
        ));
    }

    let text_bytes = args
        .num_records
        .checked_mul(args.searchable_bytes)
        .and_then(|value| value.checked_add(args.num_records - 1))
        .ok_or_else(|| ConfigError("searchable text length overflow".into()))?;
    let transform_len = text_bytes
        .checked_add(1)
        .and_then(usize::checked_next_power_of_two)
        .ok_or_else(|| ConfigError("Search transform length overflow".into()))?;
    let ckks_ring_dimension = CKKS_RINGS
        .iter()
        .copied()
        .find(|ring| ring / 2 >= transform_len)
        .ok_or_else(|| {
            ConfigError(format!(
                "Search transform length {transform_len} exceeds maximum {} complex slots",
                CKKS_RINGS.last().copied().unwrap_or_default() / 2
            ))
        })?;
    let required_fetch_slots = args
        .num_records
        .checked_mul(args.record_slots)
        .ok_or_else(|| ConfigError("Fetch slot count overflow".into()))?;
    let bgv_ring_dimension = BGV_RINGS
        .iter()
        .copied()
        .find(|ring| *ring >= required_fetch_slots)
        .ok_or_else(|| {
            ConfigError(format!(
                "Fetch requires {required_fetch_slots} slots but maximum supported BGV ring is {}",
                BGV_RINGS.last().copied().unwrap_or_default()
            ))
        })?;

    Ok(CliConfig {
        num_records: args.num_records,
        searchable_bytes: args.searchable_bytes,
        payload_bytes: args.payload_bytes,
        record_slots: args.record_slots,
        capacity: args.capacity,
        pattern_length: args.pattern_length,
        search_mode: args.search_mode,
        warmups: args.warmups,
        trials: args.trials,
        text_bytes,
        transform_len,
        ckks_ring_dimension,
        bgv_ring_dimension,
    })
}
