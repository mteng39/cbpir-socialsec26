use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "cbpir", disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[arg(long, global = true)]
    pub quick: bool,
    #[arg(long, global = true, default_value = "ct_ct")]
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
