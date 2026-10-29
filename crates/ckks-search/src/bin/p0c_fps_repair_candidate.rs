use std::env;
use std::time::Instant;

use cbpir_search::{
    FpsCkksParameters, FpsKeySwitchTechnique, PHASE1_FIXTURE_TEXT_LEN,
    server_text_ct_pt_fps_context_json_with_parameters, server_text_ct_pt_fps_with_parameters,
};
use serde_json::{Value, json};

const THRESHOLD: f64 = 0.5;

#[derive(Clone, Copy)]
enum Candidate {
    OriginalHybrid,
    OriginalFixedBv,
    Recommended,
    PrecisionFixedHybrid,
    RecommendedFixedBv,
}

impl Candidate {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "original-hybrid" => Some(Self::OriginalHybrid),
            "original-fixed-bv" => Some(Self::OriginalFixedBv),
            "recommended" => Some(Self::Recommended),
            "precision-fixed-hybrid" => Some(Self::PrecisionFixedHybrid),
            "recommended-fixed-bv" => Some(Self::RecommendedFixedBv),
            _ => None,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::OriginalHybrid => "original-hybrid",
            Self::OriginalFixedBv => "original-fixed-bv",
            Self::Recommended => "openfhe-recommended",
            Self::PrecisionFixedHybrid => "precision-fixed-hybrid",
            Self::RecommendedFixedBv => "recommended-fixed-bv",
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::OriginalHybrid => "retained FPS-14 tuple",
            Self::OriginalFixedBv => {
                "retained FPS-14 modulus sizes; BV selected because FPS generates no key-switch keys"
            }
            Self::Recommended => "OpenFHE 1.5.1 CKKSRNS_SCHEME_DEFAULTS",
            Self::PrecisionFixedHybrid => {
                "OpenFHE default modulus sizes with retained FIXEDMANUAL/HYBRID"
            }
            Self::RecommendedFixedBv => {
                "OpenFHE default modulus sizes; FIXEDMANUAL retained; BV selected because FPS generates no key-switch keys"
            }
        }
    }

    fn parameters(self, ring_dimension: u32, transform_len: usize) -> FpsCkksParameters {
        match self {
            Self::OriginalHybrid => FpsCkksParameters::fixed_manual(
                ring_dimension,
                transform_len,
                60,
                40,
                FpsKeySwitchTechnique::Hybrid,
            ),
            Self::OriginalFixedBv => FpsCkksParameters::fixed_manual(
                ring_dimension,
                transform_len,
                60,
                40,
                FpsKeySwitchTechnique::Bv,
            ),
            Self::Recommended => {
                FpsCkksParameters::openfhe_recommended(ring_dimension, transform_len)
            }
            Self::PrecisionFixedHybrid => FpsCkksParameters::fixed_manual(
                ring_dimension,
                transform_len,
                60,
                50,
                FpsKeySwitchTechnique::Hybrid,
            ),
            Self::RecommendedFixedBv => FpsCkksParameters::fixed_manual(
                ring_dimension,
                transform_len,
                60,
                50,
                FpsKeySwitchTechnique::Bv,
            ),
        }
        .expect("candidate layout must be valid")
    }
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn cm_value(value: &Value) -> Option<u64> {
    match value {
        Value::Object(values) => {
            if let Some(Value::Object(cm)) = values.get("cm") {
                if let Some(result) = cm.get("v").and_then(Value::as_u64) {
                    return Some(result);
                }
            }
            values.values().find_map(cm_value)
        }
        Value::Array(values) => values.iter().find_map(cm_value),
        _ => None,
    }
}

fn collect_prime_arrays(value: &Value, candidates: &mut Vec<Vec<u64>>) {
    match value {
        Value::Object(values) => {
            for (key, child) in values {
                if key == "p" {
                    if let Value::Array(entries) = child {
                        let chain = entries.iter().filter_map(cm_value).collect::<Vec<_>>();
                        if !chain.is_empty() {
                            candidates.push(chain);
                        }
                    }
                }
                collect_prime_arrays(child, candidates);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_prime_arrays(child, candidates);
            }
        }
        _ => {}
    }
}

fn modulus_chain(context_json: &str) -> Vec<u64> {
    let root: Value = serde_json::from_str(context_json).expect("context JSON must parse");
    let mut candidates = Vec::new();
    collect_prime_arrays(&root, &mut candidates);
    candidates
        .into_iter()
        .max_by_key(Vec::len)
        .unwrap_or_default()
}

fn run_case(
    id: &str,
    text: &[u8],
    pattern: &[u8],
    ring_dimension: u32,
    candidate: Candidate,
) -> Value {
    let transform_len = (text.len() + 1).next_power_of_two();
    let parameters = candidate.parameters(ring_dimension, transform_len);
    let wall_start = Instant::now();
    let result = server_text_ct_pt_fps_with_parameters(text, pattern, THRESHOLD, parameters)
        .expect("retained FPS execution must succeed");
    let wall_ns = elapsed_ns(wall_start);
    let errors = result
        .scores
        .iter()
        .zip(&result.exact_scores)
        .map(|(approximate, exact)| (approximate - exact).abs())
        .collect::<Vec<_>>();
    let exact_accept_count = result
        .exact_scores
        .iter()
        .filter(|score| score.abs() <= THRESHOLD)
        .count();
    let recovered_accept_count = result
        .scores
        .iter()
        .filter(|score| score.abs() <= THRESHOLD)
        .count();
    let decision_mismatches = result
        .scores
        .iter()
        .zip(&result.exact_scores)
        .filter(|(approximate, exact)| {
            (approximate.abs() <= THRESHOLD) != (exact.abs() <= THRESHOLD)
        })
        .count();
    let timings = result.metrics.timings;
    json!({
        "id": id,
        "text_length": text.len(),
        "pattern_length": pattern.len(),
        "dft_length": transform_len,
        "max_abs_error": errors.iter().copied().fold(0.0_f64, f64::max),
        "mean_abs_error": errors.iter().sum::<f64>() / errors.len() as f64,
        "accept_count_exact": exact_accept_count,
        "accept_count_recovered": recovered_accept_count,
        "decision_mismatches": decision_mismatches,
        "wall_ns": wall_ns,
        "timings_ns": {
            "context_generation": timings.context_generation_ns,
            "key_generation": timings.key_generation_ns,
            "server_text_dft_preparation": timings.server_text_dft_preparation_ns,
            "server_plaintext_encoding": timings.server_plaintext_encoding_ns,
            "client_query_dft_preparation": timings.client_query_dft_preparation_ns,
            "client_query_encryption": timings.client_query_encryption_ns,
            "server_query_plaintext_preparation": timings.server_query_plaintext_preparation_ns,
            "server_homomorphic_evaluation": timings.server_homomorphic_evaluation_ns,
            "client_decrypt": timings.client_decrypt_ns,
            "client_idft": timings.client_idft_ns,
            "oracle_error_instrumentation": timings.oracle_error_instrumentation_ns,
            "cold_end_to_end": timings.cold_end_to_end_ns(),
        },
        "actual": {
            "ring_dimension": result.metrics.ring_dimension,
            "batch_size": result.metrics.batch_size,
            "first_modulus_bits": result.metrics.first_modulus_bits,
            "scaling_modulus_bits": result.metrics.scaling_modulus_bits,
            "scaling_technique": result.metrics.scaling_technique,
            "key_switch_technique": result.metrics.key_switch_technique,
            "openfhe_version": result.metrics.openfhe_version,
        },
        "operations": result.metrics.operations,
    })
}

fn main() {
    let mut args = env::args().skip(1);
    let ring_dimension = args
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| matches!(*value, 8_192 | 16_384 | 32_768))
        .expect("usage: p0c_fps_repair_candidate <8192|16384|32768> <candidate>");
    let candidate = args
        .next()
        .as_deref()
        .and_then(Candidate::parse)
        .expect("candidate: original-hybrid | original-fixed-bv | recommended | precision-fixed-hybrid | recommended-fixed-bv");
    assert!(args.next().is_none(), "unexpected extra argument");

    let boundary_parameters = candidate.parameters(ring_dimension, 2_048);
    let context_json = server_text_ct_pt_fps_context_json_with_parameters(boundary_parameters)
        .expect("candidate context must serialize");
    let chain = modulus_chain(&context_json);
    let modulus_bits = chain
        .iter()
        .map(|value| u64::BITS - value.leading_zeros())
        .collect::<Vec<_>>();

    let mut cases = Vec::new();
    cases.push(run_case(
        "deterministic",
        &[1, 2, 3, 4, 1, 2, 3],
        &[2, 3, 4],
        ring_dimension,
        candidate,
    ));
    for trial in 0..5_usize {
        let text = (0..31_usize)
            .map(|index| (((index * 17 + trial * 29 + 11) % 251) + 1) as u8)
            .collect::<Vec<_>>();
        let position = (trial * 5) % 27;
        let mut pattern = text[position..position + 4].to_vec();
        if trial % 2 == 1 {
            pattern[0] = pattern[0].wrapping_add(1);
        }
        cases.push(run_case(
            &format!("random-{trial}"),
            &text,
            &pattern,
            ring_dimension,
            candidate,
        ));
    }
    let boundary_pattern = (128_u8..192).collect::<Vec<_>>();
    let mut boundary_text = vec![0_u8; PHASE1_FIXTURE_TEXT_LEN];
    let boundary_position = PHASE1_FIXTURE_TEXT_LEN - boundary_pattern.len();
    boundary_text[boundary_position..].copy_from_slice(&boundary_pattern);
    cases.push(run_case(
        "implementation-boundary",
        &boundary_text,
        &boundary_pattern,
        ring_dimension,
        candidate,
    ));

    let max_abs_error = cases
        .iter()
        .filter_map(|case| case["max_abs_error"].as_f64())
        .fold(0.0_f64, f64::max);
    let weighted_error_sum = cases
        .iter()
        .map(|case| {
            let score_count = case["text_length"].as_u64().unwrap()
                - case["pattern_length"].as_u64().unwrap()
                + 1;
            case["mean_abs_error"].as_f64().unwrap() * score_count as f64
        })
        .sum::<f64>();
    let score_count = cases
        .iter()
        .map(|case| {
            case["text_length"].as_u64().unwrap() - case["pattern_length"].as_u64().unwrap() + 1
        })
        .sum::<u64>();
    let exact_accept_count = cases
        .iter()
        .filter_map(|case| case["accept_count_exact"].as_u64())
        .sum::<u64>();
    let recovered_accept_count = cases
        .iter()
        .filter_map(|case| case["accept_count_recovered"].as_u64())
        .sum::<u64>();
    let decision_mismatches = cases
        .iter()
        .filter_map(|case| case["decision_mismatches"].as_u64())
        .sum::<u64>();
    let parameters = candidate.parameters(ring_dimension, 2_048);
    let status = if max_abs_error <= 1.0 && decision_mismatches == 0 {
        "PASS"
    } else {
        "FAIL_CORRECTNESS"
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": status,
            "profile": format!("FPS-{}", ring_dimension.ilog2()),
            "candidate_id": candidate.id(),
            "parameter_source": candidate.source(),
            "security": {
                "requested": "HEStd_128_classic",
                "validated_by": "OpenFHE 1.5.1 standards gate",
                "numeric_estimate": null,
            },
            "parameters": {
                "ring_dimension": ring_dimension,
                "multiplicative_depth": parameters.multiplicative_depth,
                "first_modulus_bits": parameters.first_modulus_bits,
                "scaling_modulus_bits": parameters.scaling_modulus_bits,
                "scaling_technique": parameters.scaling_technique.label(),
                "key_switch_technique": parameters.key_switch_technique.label(),
                "chi_s": "UNIFORM_TERNARY",
                "chi_e": "discrete Gaussian stddev=3.19",
            },
            "modulus_chain": chain,
            "modulus_bits": modulus_bits,
            "total_modulus_bits": modulus_bits.iter().sum::<u32>(),
            "correctness": {
                "score_count": score_count,
                "max_abs_error": max_abs_error,
                "mean_abs_error": weighted_error_sum / score_count as f64,
                "accept_count_exact": exact_accept_count,
                "accept_count_recovered": recovered_accept_count,
                "decision_mismatches": decision_mismatches,
                "contract_max_abs_error": 1.0,
                "fps_semantics": "M_a=0 iff T[a:a+m]=P",
            },
            "cases": cases,
        }))
        .unwrap()
    );
}
