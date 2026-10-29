use crate::{Catalog, MatchBackend, MatchMode, MatchRequest, P0dError};
use cbpir_search::{
    CkksRunMetrics, ServerTextCtPtMetrics, ckks_fps, ckks_rfps, server_text_ct_pt_fps,
    server_text_ct_pt_rfps,
};
use serde_json::{Value, json};

pub(crate) struct ClientMatch {
    pub scores: Vec<f64>,
    pub positions: Vec<usize>,
    pub metrics: MatchMetrics,
}

pub(crate) enum MatchMetrics {
    CtCt(CkksRunMetrics),
    ServerTextCtPt(ServerTextCtPtMetrics),
}

impl MatchMetrics {
    pub(crate) fn openfhe_version(&self) -> &str {
        match self {
            Self::CtCt(metrics) => &metrics.openfhe_version,
            Self::ServerTextCtPt(metrics) => &metrics.openfhe_version,
        }
    }

    pub(crate) fn derived_online_match_ns(&self) -> Result<u64, P0dError> {
        match self {
            Self::CtCt(metrics) => {
                let timings = metrics
                    .phase_timings
                    .ok_or(P0dError::MatchContract("missing phase timings"))?;
                Ok(timings.client_query_ns
                    + timings.server_evaluation_ns
                    + timings.client_decrypt_idft_ns)
            }
            Self::ServerTextCtPt(metrics) => Ok(metrics.timings.derived_online_match_ns()),
        }
    }
}

pub(crate) fn run_with_backend(
    c: &Catalog,
    q: &MatchRequest,
    backend: MatchBackend,
) -> Result<ClientMatch, P0dError> {
    c.validate(q)?;
    let result = match (backend, &q.mode) {
        (MatchBackend::CtCt, MatchMode::Fps) => {
            ckks_fps(c.text(), &q.pattern, q.threshold).map(|r| ClientMatch {
                scores: r.scores,
                positions: r.matching_positions,
                metrics: MatchMetrics::CtCt(r.metrics),
            })
        }
        (MatchBackend::CtCt, MatchMode::Rfps { randomizer }) => {
            ckks_rfps(c.text(), &q.pattern, randomizer, q.threshold).map(|r| ClientMatch {
                scores: r.scores,
                positions: r.matching_positions,
                metrics: MatchMetrics::CtCt(r.metrics),
            })
        }
        (MatchBackend::ServerTextCtPt, MatchMode::Fps) => {
            server_text_ct_pt_fps(c.text(), &q.pattern, q.threshold).map(|r| ClientMatch {
                scores: r.scores,
                positions: r.matching_positions,
                metrics: MatchMetrics::ServerTextCtPt(r.metrics),
            })
        }
        (MatchBackend::ServerTextCtPt, MatchMode::Rfps { randomizer }) => {
            server_text_ct_pt_rfps(c.text(), &q.pattern, randomizer, q.threshold).map(|r| {
                ClientMatch {
                    scores: r.scores,
                    positions: r.matching_positions,
                    metrics: MatchMetrics::ServerTextCtPt(r.metrics),
                }
            })
        }
    }
    .map_err(|e| P0dError::MatchComponent(format!("{e:?}")))?;
    if result.metrics.openfhe_version() != "1.5.1" {
        return Err(P0dError::Runtime(
            result.metrics.openfhe_version().to_owned(),
        ));
    }
    Ok(result)
}

fn ct_ct_metrics(m: &CkksRunMetrics) -> Value {
    let state = |s: cbpir_search::CiphertextState| json!({"level":s.level,"noise_scale_degree":s.noise_scale_degree,"scaling_factor":s.scaling_factor,"component_count":s.component_count});
    let phases=m.phase_timings.map(|t|json!({"context_and_key_generation_ns":t.context_and_key_generation_ns,"client_setup_ns":t.client_setup_ns,"client_query_ns":t.client_query_ns,"server_evaluation_ns":t.server_evaluation_ns,"client_decrypt_idft_ns":t.client_decrypt_idft_ns,"client_phase_note":"includes exact reference scoring, error and decision instrumentation"}));
    json!({"backend":"ct_ct","scheme":"CKKS","openfhe_version":m.openfhe_version,"ring_dimension":m.ring_dimension,"batch_size":m.batch_size,
        "multiplicative_depth":m.multiplicative_depth,"scaling_modulus_bits":m.scaling_modulus_bits,"first_modulus_bits":m.first_modulus_bits,
        "scaling_technique":m.scaling_technique,"security_setting":"HEStd_128_classic requested; no independent estimate",
        "input_state":state(m.input_state),"output_state":state(m.output_state),"reported_log_precision_bits":m.reported_log_precision_bits,
        "phase_timings":phases,"operations":{"ciphertext_multiplications":m.ciphertext_multiplications,"ciphertext_plaintext_multiplications":m.ciphertext_plaintext_multiplications,
        "ciphertext_constant_multiplications":m.ciphertext_constant_multiplications,"additions":m.additions,"subtractions":m.subtractions,"relinearizations":m.relinearizations,"explicit_rescaling_operations":m.explicit_rescaling_operations}})
}

fn server_text_ct_pt_metrics(m: &ServerTextCtPtMetrics) -> Value {
    let state = |s: cbpir_search::CiphertextState| json!({"level":s.level,"noise_scale_degree":s.noise_scale_degree,"scaling_factor":s.scaling_factor,"component_count":s.component_count});
    json!({
        "backend":m.backend,"scheme":"CKKS","openfhe_version":m.openfhe_version,
        "ring_dimension":m.ring_dimension,"batch_size":m.batch_size,
        "configured_multiplicative_depth":m.configured_multiplicative_depth,
        "scaling_modulus_bits":m.scaling_modulus_bits,"first_modulus_bits":m.first_modulus_bits,
        "scaling_technique":m.scaling_technique,"security_setting":"HEStd_128_classic requested; no independent estimate",
        "input_state":state(m.input_state),"output_state":state(m.output_state),
        "phase_timings":{
            "context_generation_ns":m.timings.context_generation_ns,"key_generation_ns":m.timings.key_generation_ns,
            "server_text_dft_preparation_ns":m.timings.server_text_dft_preparation_ns,
            "server_plaintext_encoding_ns":m.timings.server_plaintext_encoding_ns,
            "server_query_plaintext_preparation_ns":m.timings.server_query_plaintext_preparation_ns,
            "client_query_dft_preparation_ns":m.timings.client_query_dft_preparation_ns,
            "client_query_encryption_ns":m.timings.client_query_encryption_ns,
            "server_homomorphic_evaluation_ns":m.timings.server_homomorphic_evaluation_ns,
            "client_decrypt_ns":m.timings.client_decrypt_ns,"client_idft_ns":m.timings.client_idft_ns,
            "oracle_error_instrumentation_ns":m.timings.oracle_error_instrumentation_ns,
            "derived_online_match_ns":m.timings.derived_online_match_ns(),
            "cold_end_to_end_ns":m.timings.cold_end_to_end_ns(),
            "online_formula":"client query DFT + query encryption + server query plaintext prep + server HE evaluation + client decrypt + client IDFT"
        },
        "operations":{
            "ciphertext_multiplications":m.operations.ciphertext_multiplications,
            "ciphertext_plaintext_multiplications":m.operations.ciphertext_plaintext_multiplications,
            "ciphertext_constant_multiplications":m.operations.ciphertext_constant_multiplications,
            "additions":m.operations.additions,"subtractions":m.operations.subtractions,
            "relinearizations":m.operations.relinearizations,
            "explicit_rescaling_operations":m.operations.explicit_rescaling_operations,
            "evaluated_multiplicative_depth":m.operations.evaluated_multiplicative_depth
        },
        "required_keys":{
            "public_key":m.keys.public_key,"secret_key":m.keys.secret_key,
            "evaluation_multiplication_key":m.keys.evaluation_multiplication_key,
            "relinearization_key":m.keys.relinearization_key,"rotation_keys":m.keys.rotation_keys
        },
        "server_plaintext_state":{
            "numerical_dft_bytes":m.state_sizes.plaintext_search_state_bytes,
            "encoded_plaintext_count":m.state_sizes.encoded_server_plaintext_count,
            "note":"numerical size counts two f64 values per complex slot; encoded OpenFHE plaintext is context-specific"
        },
        "encrypted_query_ciphertext_count":m.state_sizes.encrypted_query_ciphertext_count,
        "encrypted_response_ciphertext_count":m.state_sizes.encrypted_response_ciphertext_count
    })
}

pub(crate) fn metrics(metrics: &MatchMetrics) -> Value {
    match metrics {
        MatchMetrics::CtCt(metrics) => ct_ct_metrics(metrics),
        MatchMetrics::ServerTextCtPt(metrics) => server_text_ct_pt_metrics(metrics),
    }
}
