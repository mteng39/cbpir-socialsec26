//! Canonical P3 single-query multi-record windowed CPIR.
//!
//! This crate owns proposal-facing planning and orchestration. Cryptographic
//! operations remain in the P0-A `OpenFHE` backend.

mod backend;
mod fetch_plan;
mod scaled_backend;
mod scaled_layout;

pub use backend::{P3BackendError, P3BgvBackend, P3FetchResult, P3FetchTimings, P3SetupTimings};
pub use fetch_plan::{
    FetchError, FetchPlan, FetchPlanner, FetchedRecord, RecordId, SUPPORTED_PUBLIC_K,
    discard_dummy_responses,
};
pub use scaled_backend::{
    ScaledFetchCounts, ScaledFetchResult, ScaledFetchTimings, ScaledSetupTimings,
    ShardedP3BgvBackend,
};
pub use scaled_layout::{
    BASE_WINDOW_BYTES, PHYSICAL_WINDOWS_PER_SHARD, SUPPORTED_DATABASE_RECORDS,
    SUPPORTED_PAYLOAD_BYTES, ScaledFetchPlan, ScaledLayout, ScaledP3Error, ShardAddress,
};
