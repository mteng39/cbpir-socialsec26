#![cfg(feature = "native")]

use cbpir::FormalFetchContext;
use cbpir_bgv::BgvCpirConfig;
use cbpir_search::{
    CkksConfig, FpsCkksParameters, FpsKeySwitchTechnique, FpsScalingTechnique,
    ServerTextCtPtFpsStaged, ServerTextCtPtRfpsStaged,
};

#[test]
#[ignore = "constructs large native HE contexts; exercised by scalability-v1"]
fn explicit_constructors_accept_extended_profiles() {
    let transform_len = 32_768;
    let fps = FpsCkksParameters::fixed_manual(
        65_536,
        transform_len,
        60,
        50,
        FpsKeySwitchTechnique::Hybrid,
    )
    .unwrap();
    let _fps_context = ServerTextCtPtFpsStaged::from_parameters(fps, transform_len, 0.5).unwrap();

    let rfps = CkksConfig {
        ring_dimension: Some(65_536),
        batch_size: transform_len as u32,
        multiplicative_depth: 1,
        scaling_modulus_bits: 24,
        first_modulus_bits: 60,
        decision_threshold: 0.5,
        scaling_technique: FpsScalingTechnique::FixedManual,
        key_switch_technique: FpsKeySwitchTechnique::Hybrid,
    };
    let _rfps_context = ServerTextCtPtRfpsStaged::from_config(rfps, transform_len, 0.5).unwrap();

    let bgv = BgvCpirConfig {
        ring_dimension: 65_536,
        plaintext_modulus: 786_433,
        record_slots: 64,
        multiplicative_depth: 1,
    };
    let _fetch_context = FormalFetchContext::from_config("BGV-16-extended", bgv).unwrap();
}
