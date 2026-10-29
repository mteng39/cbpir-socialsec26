use std::collections::HashMap;

use cbpir_search::{
    CkksConfig, CkksError, DecisionCause, MAX_CKKS_BATCH_SIZE, RFPS_SCALING_MODULUS_BITS,
    RfpsPackedTrialInput, SearchError, ckks_rfps, ckks_rfps_packed_trials, dft, idft,
    proposal_workloads, rfps_time_layout,
};
use num_complex::Complex64;

const PACKED_RFPS_TEST_ERROR_ENVELOPE: f64 = 2.0;

fn input(id: &str, text: &[u8], pattern: &[u8], randomizer: &[u8]) -> RfpsPackedTrialInput {
    RfpsPackedTrialInput {
        id: id.to_owned(),
        text: text.to_vec(),
        pattern: pattern.to_vec(),
        randomizer: randomizer.to_vec(),
    }
}

#[test]
fn packed_config_is_distinct_from_single_search_transform_limit() {
    assert_eq!(MAX_CKKS_BATCH_SIZE, 4096);
    let packed = CkksConfig::for_packed_rfps_batch_size(MAX_CKKS_BATCH_SIZE).unwrap();
    assert_eq!(packed.batch_size, 4096);
    assert_eq!(packed.multiplicative_depth, 1);
    assert_eq!(packed.scaling_modulus_bits, RFPS_SCALING_MODULUS_BITS);
    assert_eq!(packed.first_modulus_bits, 60);
    assert_eq!(
        CkksConfig::for_packed_rfps_batch_size(8192),
        Err(CkksError::UnsupportedBatchSize(8192))
    );
}

#[test]
#[allow(clippy::cast_precision_loss)]
fn packed_trials_match_scalar_ckks_and_independent_exact_scores() {
    let fixture = proposal_workloads().unwrap();
    for k in 1..=4 {
        let inputs = fixture
            .rfps_probability
            .trials
            .iter()
            .filter(|trial| trial.k == k)
            .take(2)
            .map(|trial| input(&trial.id, &trial.text, &trial.pattern, &trial.randomizer))
            .collect::<Vec<_>>();
        let packed = ckks_rfps_packed_trials(&inputs, 0.5).unwrap();
        assert_eq!(packed.trials.len(), inputs.len());
        assert_eq!(
            packed.encoded_slot_count,
            inputs.len() * packed.block_transform_len
        );
        assert_eq!(packed.padded_slot_count, 4096);
        assert_eq!(packed.metrics.batch_size, 4096);
        assert_eq!(packed.metrics.ciphertext_multiplications, 1);
        assert_eq!(packed.metrics.ciphertext_constant_multiplications, 1);
        assert_eq!(packed.metrics.subtractions, 1);

        for (packed_trial, trial) in packed.trials.iter().zip(&inputs) {
            let scalar = ckks_rfps(&trial.text, &trial.pattern, &trial.randomizer, 0.5).unwrap();
            assert_eq!(packed_trial.id, trial.id);
            assert_eq!(packed_trial.exact_score, scalar.exact_scores[0]);
            assert!(
                (packed_trial.approximate_score - packed_trial.exact_score as f64).abs()
                    <= PACKED_RFPS_TEST_ERROR_ENVELOPE
            );
            assert!(
                (scalar.scores[0] - scalar.exact_scores[0] as f64).abs()
                    <= PACKED_RFPS_TEST_ERROR_ENVELOPE
            );
        }
    }
}

#[test]
#[allow(clippy::cast_precision_loss)]
fn packed_true_match_is_preserved_and_neighbor_reordering_is_isolated() {
    let inputs = vec![
        input("left", &[0, 2, 1], &[2, 0, 1], &[3, 5, 7]),
        input("match", &[2, 1, 0], &[2, 1, 0], &[11, 13, 17]),
        input("right", &[2, 0, 1], &[0, 1, 2], &[19, 23, 29]),
    ];
    let forward = ckks_rfps_packed_trials(&inputs, 0.5).unwrap();
    let reversed_inputs = inputs.iter().cloned().rev().collect::<Vec<_>>();
    let reversed = ckks_rfps_packed_trials(&reversed_inputs, 0.5).unwrap();
    let reversed_by_id = reversed
        .trials
        .iter()
        .map(|trial| (trial.id.as_str(), trial))
        .collect::<HashMap<_, _>>();

    for trial in &forward.trials {
        let reordered = reversed_by_id[trial.id.as_str()];
        assert_eq!(trial.exact_score, reordered.exact_score);
        assert!(
            (trial.approximate_score - trial.exact_score as f64).abs()
                <= PACKED_RFPS_TEST_ERROR_ENVELOPE
        );
        assert!(
            (reordered.approximate_score - reordered.exact_score as f64).abs()
                <= PACKED_RFPS_TEST_ERROR_ENVELOPE
        );
    }
    let true_match = forward
        .trials
        .iter()
        .find(|trial| trial.id == "match")
        .unwrap();
    assert_eq!(true_match.exact_score, 0);
    assert_eq!(true_match.decision.cause, DecisionCause::TrueMatchAccepted);
}

#[test]
fn plaintext_concatenated_blocks_have_exactly_independent_idfts() {
    let trials = [
        input("a", &[0, 2, 1], &[2, 0, 1], &[3, 5, 7]),
        input("b", &[2, 1, 0], &[2, 1, 0], &[11, 13, 17]),
        input("c", &[2, 0, 1], &[0, 1, 2], &[19, 23, 29]),
    ];
    let block_len = 4;
    let mut packed_scores = vec![Complex64::new(0.0, 0.0); 4096];
    for (trial_index, trial) in trials.iter().enumerate() {
        let layout = rfps_time_layout(&trial.text, &trial.pattern, &trial.randomizer).unwrap();
        let text = dft(&layout
            .text
            .iter()
            .map(|value| Complex64::new(*value, 0.0))
            .collect::<Vec<_>>());
        let randomizer = dft(&layout
            .reversed_randomizer
            .iter()
            .map(|value| Complex64::new(*value, 0.0))
            .collect::<Vec<_>>());
        let repeated_dot = dft(&vec![
            Complex64::new(
                layout.pattern_randomizer_dot_product,
                0.0
            );
            block_len
        ]);
        let offset = trial_index * block_len;
        for index in 0..block_len {
            packed_scores[offset + index] = text[index] * randomizer[index] - repeated_dot[index];
        }
    }
    for (trial_index, trial) in trials.iter().enumerate() {
        let offset = trial_index * block_len;
        let decoded = idft(&packed_scores[offset..offset + block_len]);
        let exact = trial
            .text
            .iter()
            .zip(&trial.pattern)
            .zip(&trial.randomizer)
            .map(|((text, pattern), randomizer)| {
                (f64::from(*text) - f64::from(*pattern)) * f64::from(*randomizer)
            })
            .sum::<f64>();
        assert!((decoded[trial.text.len() - 1].re - exact).abs() <= 1e-10);
    }
}

#[test]
fn packed_validation_rejects_bad_shapes_domains_and_capacity() {
    assert_eq!(
        ckks_rfps_packed_trials(&[], 0.5),
        Err(CkksError::EmptyPackedTrials)
    );
    assert_eq!(
        ckks_rfps_packed_trials(&[input("bad-threshold", &[1], &[1], &[3])], f64::NAN),
        Err(CkksError::InvalidDecisionThreshold)
    );

    let duplicate = input("duplicate", &[1], &[0], &[3]);
    assert_eq!(
        ckks_rfps_packed_trials(&[duplicate.clone(), duplicate], 0.5),
        Err(CkksError::DuplicatePackedTrialId("duplicate".to_owned()))
    );

    let unequal = input("unequal", &[1, 2], &[1], &[3]);
    assert_eq!(
        ckks_rfps_packed_trials(&[unequal], 0.5),
        Err(CkksError::PackedTrialLengthMismatch {
            trial_index: 0,
            text: 2,
            pattern: 1,
            randomizer: 1,
        })
    );

    let invalid_randomizer = input("bad-r", &[1], &[0], &[0]);
    assert_eq!(
        ckks_rfps_packed_trials(&[invalid_randomizer], 0.5),
        Err(CkksError::Search(
            SearchError::RandomizerOutsideSupportedDomain { index: 0, value: 0 }
        ))
    );

    let mixed = vec![
        input("k1", &[0], &[1], &[3]),
        input("k2", &[0, 1], &[1, 0], &[3, 4]),
    ];
    assert_eq!(
        ckks_rfps_packed_trials(&mixed, 0.5),
        Err(CkksError::MixedPackedTransformLengths {
            expected: 2,
            actual: 4,
            trial_index: 1,
        })
    );

    let too_many = (0..513)
        .map(|index| input(&format!("trial-{index}"), &[0; 4], &[1; 4], &[3; 4]))
        .collect::<Vec<_>>();
    assert_eq!(
        ckks_rfps_packed_trials(&too_many, 0.5),
        Err(CkksError::PackedSlotCapacityExceeded {
            required: 4104,
            maximum: 4096,
        })
    );
}
