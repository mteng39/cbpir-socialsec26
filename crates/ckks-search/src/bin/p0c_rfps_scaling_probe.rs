use cbpir_search::{EncryptedRfpsResult, ckks_rfps_with_scaling_modulus_bits, proposal_workloads};
use serde_json::{Value, json};

const SCALES: [u32; 7] = [20, 21, 22, 23, 24, 25, 30];
const REPETITIONS: usize = 4;
const THRESHOLD: f64 = 0.5;

struct ProbeCase {
    id: String,
    text: Vec<u8>,
    pattern: Vec<u8>,
    randomizer: Vec<u8>,
    expected_positions: Vec<usize>,
}

fn observation(
    case: &ProbeCase,
    scaling_modulus_bits: u32,
    repetition: usize,
    result: &EncryptedRfpsResult,
) -> Value {
    let rejected_positions = case
        .expected_positions
        .iter()
        .copied()
        .filter(|position| !result.matching_positions.contains(position))
        .collect::<Vec<_>>();
    let max_true_match_abs_score = case
        .expected_positions
        .iter()
        .map(|position| result.scores[*position].abs())
        .fold(0.0_f64, f64::max);
    json!({
        "case_id": case.id,
        "scaling_modulus_bits": scaling_modulus_bits,
        "repetition": repetition,
        "text_len": case.text.len(),
        "pattern_len": case.pattern.len(),
        "expected_true_match_count": case.expected_positions.len(),
        "rejected_true_match_count": rejected_positions.len(),
        "rejected_positions": rejected_positions,
        "max_true_match_abs_score": max_true_match_abs_score,
        "all_exact_true_match_scores_zero": case.expected_positions.iter().all(|position| result.exact_scores[*position] == 0),
        "ring_dimension": result.metrics.ring_dimension,
        "batch_size": result.metrics.batch_size
    })
}

fn main() {
    let fixture = proposal_workloads().expect("canonical proposal workload fixture");
    let selected_ids = [
        "synthetic-n0064-m32-exact",
        "synthetic-n0064-m64-exact",
        "synthetic-n1024-m64-exact",
        "bgl_2k_line_4",
    ];
    let mut cases = fixture
        .synthetic_cases
        .iter()
        .map(|case| ProbeCase {
            id: case.id.clone(),
            text: case.text.clone(),
            pattern: case.pattern.clone(),
            randomizer: case.randomizer.clone(),
            expected_positions: case.expected_match_positions.clone(),
        })
        .chain(fixture.bgl.cases.iter().map(|case| ProbeCase {
            id: case.id.clone(),
            text: case.text.clone(),
            pattern: case.pattern.clone(),
            randomizer: case.randomizer.clone(),
            expected_positions: case.expected_match_positions.clone(),
        }))
        .filter(|case| selected_ids.contains(&case.id.as_str()))
        .collect::<Vec<_>>();
    cases.sort_by(|left, right| left.id.cmp(&right.id));
    assert_eq!(cases.len(), selected_ids.len());
    cases.push(ProbeCase {
        id: "maximum-domain-n1024-m64-all-255-r32".to_owned(),
        text: vec![u8::MAX; 1024],
        pattern: vec![u8::MAX; 64],
        randomizer: vec![32; 64],
        expected_positions: (0..=1024 - 64).collect(),
    });

    let mut observations = Vec::new();
    for scaling_modulus_bits in SCALES {
        for case in &cases {
            for repetition in 1..=REPETITIONS {
                let result = ckks_rfps_with_scaling_modulus_bits(
                    &case.text,
                    &case.pattern,
                    &case.randomizer,
                    THRESHOLD,
                    scaling_modulus_bits,
                )
                .expect("valid RFPS scale probe");
                observations.push(observation(case, scaling_modulus_bits, repetition, &result));
            }
        }
    }

    let report = json!({
        "schema": "p0c-rfps-scaling-probe-v1",
        "purpose": "one-variable empirical RFPS scaling-modulus calibration after retained 20-bit mapping crossed diagnostic threshold 0.5 on true matches",
        "threshold": THRESHOLD,
        "scaling_modulus_bits": SCALES,
        "repetitions_per_case_and_scale": REPETITIONS,
        "case_count": cases.len(),
        "observations": observations,
        "limitations": [
            "Finite randomized CKKS observations are not a uniform error proof.",
            "This probe changes only ScalingModSize; it does not establish a causal analytical error bound."
        ]
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
