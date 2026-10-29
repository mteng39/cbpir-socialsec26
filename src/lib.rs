//! Prototype P0-D: bounded P0-C Match, client Locate, and BGV Fetch composition.
// Public fallible functions use stage-labelled errors described by their types.
#![allow(clippy::missing_errors_doc, clippy::must_use_candidate)]
mod frame;
mod layout;
mod locate;
mod model;
mod multi_fixtures;
mod multi_locate;
mod oracle;
pub mod realistic;
pub mod realistic_kgt1;
mod search_correctness;
pub use frame::*;
pub use layout::*;
pub use locate::*;
pub use model::*;
pub use multi_fixtures::*;
pub use multi_locate::*;
pub use oracle::*;
pub use search_correctness::*;
mod fixtures;
pub use fixtures::*;
#[cfg(feature = "native")]
mod configurable_fixed_fetch;
#[cfg(feature = "native")]
mod fetch_stage;
#[cfg(feature = "native")]
mod formal_benchmark;
#[cfg(feature = "native")]
mod match_stage;
#[cfg(feature = "native")]
mod multi_fetch_stage;
#[cfg(feature = "native")]
mod multi_pipeline;
#[cfg(feature = "native")]
mod no_dummy_fetch;
#[cfg(feature = "native")]
mod phase2_match;
#[cfg(feature = "native")]
mod pipeline;
#[cfg(feature = "native")]
pub use configurable_fixed_fetch::*;
#[cfg(feature = "native")]
pub use formal_benchmark::*;
#[cfg(feature = "native")]
pub use multi_fetch_stage::{MultiFetchPreparation, MultiFetchTimings, MultiFetchTrace};
#[cfg(feature = "native")]
pub use multi_pipeline::*;
#[cfg(feature = "native")]
pub use no_dummy_fetch::*;
#[cfg(feature = "native")]
pub use phase2_match::*;
#[cfg(feature = "native")]
pub mod multi_report;
#[cfg(feature = "native")]
pub use pipeline::*;
#[cfg(feature = "native")]
pub mod report;
