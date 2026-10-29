use std::collections::HashMap;

use cbpir_search::{
    CkksError, DecisionCause, RfpsPackedTrialInput, server_text_ct_pt_rfps_packed_trials,
};

fn trial(id: &str, text: &[u8], pattern: &[u8], randomizer: &[u8]) -> RfpsPackedTrialInput {
    RfpsPackedTrialInput {
        id: id.to_owned(),
        text: text.to_vec(),
        pattern: pattern.to_vec(),
        randomizer: randomizer.to_vec(),
    }
}

#[test]
fn server_text_ct_pt_packed_reuses_baseline_shape_validation() {
    assert_eq!(
        server_text_ct_pt_rfps_packed_trials(&[], 0.5),
        Err(CkksError::EmptyPackedTrials)
    );
    let duplicate = trial("same", &[1], &[1], &[3]);
    assert_eq!(
        server_text_ct_pt_rfps_packed_trials(&[duplicate.clone(), duplicate], 0.5),
        Err(CkksError::DuplicatePackedTrialId("same".to_owned()))
    );
}

#[test]
#[allow(
    clippy::cast_precision_loss,
    reason = "declared RFPS exact-score domain is below 2^53"
)]
fn server_text_ct_pt_packed_rfps_preserves_trial_isolation() {
    let inputs = vec![
        trial("left", &[0, 2, 1], &[2, 0, 1], &[3, 5, 7]),
        trial("match", &[2, 1, 0], &[2, 1, 0], &[11, 13, 17]),
        trial("right", &[2, 0, 1], &[0, 1, 2], &[19, 23, 29]),
    ];
    let forward = server_text_ct_pt_rfps_packed_trials(&inputs, 0.5).unwrap();
    let reversed_inputs = inputs.iter().cloned().rev().collect::<Vec<_>>();
    let reversed = server_text_ct_pt_rfps_packed_trials(&reversed_inputs, 0.5).unwrap();
    let reversed_by_id = reversed
        .trials
        .iter()
        .map(|result| (result.id.as_str(), result))
        .collect::<HashMap<_, _>>();

    assert_eq!(forward.block_transform_len, 4);
    assert_eq!(forward.encoded_slot_count, 12);
    assert_eq!(forward.padded_slot_count, 4096);
    assert_eq!(forward.metrics.operations.ciphertext_multiplications, 0);
    assert_eq!(
        forward
            .metrics
            .operations
            .ciphertext_plaintext_multiplications,
        1
    );
    assert_eq!(forward.metrics.operations.relinearizations, 0);
    assert!(!forward.metrics.keys.evaluation_multiplication_key);
    for result in &forward.trials {
        let reordered = reversed_by_id[result.id.as_str()];
        assert_eq!(result.exact_score, reordered.exact_score);
        assert!((result.approximate_score - result.exact_score as f64).abs() <= 2.0);
        assert!((reordered.approximate_score - reordered.exact_score as f64).abs() <= 2.0);
    }
    let true_match = forward
        .trials
        .iter()
        .find(|result| result.id == "match")
        .unwrap();
    assert_eq!(true_match.decision.cause, DecisionCause::TrueMatchAccepted);
}
