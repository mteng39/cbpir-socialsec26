//! Prototype P0-C: CKKS reproduction of DFT-based FPS and RFPS.

mod ckks;
mod classification;
mod layout;
mod serialization;
mod server_text_ct_pt;
mod server_text_ct_pt_report;
mod staged_server_text_ct_pt;
mod transform;
mod workloads;

pub use ckks::{
    CiphertextState, CkksConfig, CkksError, CkksRunMetrics, CkksVectorResult, EncryptedFpsResult,
    EncryptedPackedRfpsResult, EncryptedRfpsResult, FpsCkksParameters, FpsKeySwitchTechnique,
    FpsProfile, FpsScalingTechnique, MAX_CKKS_BATCH_SIZE, ProtocolPhaseTimings,
    RFPS_SCALING_MODULUS_BITS, RfpsPackedTrialInput, RfpsPackedTrialResult, RfpsProfile,
    ckks_componentwise_product, ckks_fps, ckks_rfps, ckks_rfps_packed_trials,
    ckks_rfps_with_scaling_modulus_bits,
};
pub use classification::{DecisionCause, ScoreDecision, classify_score};
pub use layout::{
    FpsTimeLayout, MAX_PATTERN_LEN, PHASE1_FIXTURE_TEXT_LEN, PHASE1_FIXTURE_TRANSFORM_LEN,
    RfpsTimeLayout, SearchError, feer_fps_scores, feer_rfps_scores, fps_time_layout,
    rfps_time_layout, right_rotation_factors,
};
pub use serialization::{
    CkksSerializationResult, SerializedArtifactSizes, ckks_fps_serialization_roundtrip,
    ckks_rfps_serialization_roundtrip,
};
pub use server_text_ct_pt::{
    SERVER_TEXT_CT_PT_BACKEND, ServerTextCtPtArtifactSizes, ServerTextCtPtFpsResult,
    ServerTextCtPtKeyRequirements, ServerTextCtPtMetrics, ServerTextCtPtOperationCounts,
    ServerTextCtPtPackedRfpsResult, ServerTextCtPtRfpsResult, ServerTextCtPtStateSizes,
    ServerTextCtPtTimings, server_text_ct_pt_fps, server_text_ct_pt_fps_artifact_sizes,
    server_text_ct_pt_fps_artifact_sizes_with_profile, server_text_ct_pt_fps_context_json,
    server_text_ct_pt_fps_context_json_with_parameters, server_text_ct_pt_fps_with_parameters,
    server_text_ct_pt_fps_with_profile, server_text_ct_pt_rfps,
    server_text_ct_pt_rfps_artifact_sizes, server_text_ct_pt_rfps_artifact_sizes_with_profile,
    server_text_ct_pt_rfps_context_json, server_text_ct_pt_rfps_packed_artifact_sizes,
    server_text_ct_pt_rfps_packed_trials, server_text_ct_pt_rfps_with_profile,
};
pub use server_text_ct_pt_report::server_text_ct_pt_focused_report;
pub use staged_server_text_ct_pt::{
    ServerTextCtPtFpsQuery, ServerTextCtPtFpsServerState, ServerTextCtPtFpsStaged,
    ServerTextCtPtMessageSizes, ServerTextCtPtRecoveredScores, ServerTextCtPtResponse,
    ServerTextCtPtRfpsQuery, ServerTextCtPtRfpsServerState, ServerTextCtPtRfpsStaged,
    ServerTextCtPtRuntimeProfile,
};
pub use transform::{dft, idft, naive_dft_reference, naive_idft_reference};
pub use workloads::{
    AuthorityMetadata, BglCase, BglSource, BglWorkload, GeneratorMetadata, ProposalWorkloads,
    RfpsProbabilityModel, RfpsProbabilityWorkload, RfpsTrial, RfpsTrueMatchControl, SyntheticCase,
    WorkloadDomain, WorkloadError, fixture_sha256, proposal_workloads,
};
