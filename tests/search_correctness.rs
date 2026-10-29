use cbpir::{
    RecordId, formal_rfps_seed_schedule, fresh_rfps_weights_for_seed, record_set_metrics,
    rfps_record_attribution, validate_search_correctness_pattern_lengths,
};

fn ids(values: &[u64]) -> Vec<RecordId> {
    values
        .iter()
        .map(|value| RecordId::new(*value).unwrap())
        .collect()
}

#[test]
fn formal_seed_schedule_flattens_the_old_four_k_groups() {
    let schedule = formal_rfps_seed_schedule(8192).unwrap();

    assert_eq!(schedule.len(), 80);
    assert_eq!(
        (
            schedule[0].source_public_k,
            schedule[0].source_run,
            schedule[0].invocation
        ),
        (1, 0, 3)
    );
    assert_eq!(
        (
            schedule[20].source_public_k,
            schedule[20].source_run,
            schedule[20].invocation
        ),
        (4, 0, 26)
    );
    assert_eq!(
        (
            schedule[79].source_public_k,
            schedule[79].source_run,
            schedule[79].invocation
        ),
        (16, 19, 91)
    );
    assert_eq!(fresh_rfps_weights_for_seed(schedule[0].seed, 1), vec![10]);
}

#[test]
fn record_metrics_use_unique_sorted_record_ids() {
    let metrics = record_set_metrics(&ids(&[3, 1, 3, 2]), &ids(&[4, 3, 3, 1]));

    assert_eq!(metrics.true_positive_ids, ids(&[1, 3]));
    assert_eq!(metrics.false_negative_ids, ids(&[2]));
    assert_eq!(metrics.false_positive_ids, ids(&[4]));
    assert_eq!((metrics.tp, metrics.fn_count, metrics.fp_count), (2, 1, 1));
    assert_eq!(metrics.recall, 2.0 / 3.0);
    assert_eq!(metrics.precision, Some(2.0 / 3.0));
}

#[test]
fn precision_is_absent_when_no_candidate_record_exists() {
    let metrics = record_set_metrics(&ids(&[1, 2]), &[]);

    assert_eq!((metrics.tp, metrics.fn_count, metrics.fp_count), (0, 2, 0));
    assert_eq!(metrics.recall, 0.0);
    assert_eq!(metrics.precision, None);
}

#[test]
fn rfps_attribution_separates_semantics_from_ckks_approximation() {
    let attribution = rfps_record_attribution(&ids(&[1, 2]), &ids(&[1, 2, 3]), &ids(&[1, 3, 4]));

    assert_eq!(attribution.exact_semantic_false_positive_ids, ids(&[3]));
    assert_eq!(attribution.ckks_false_positive_ids, ids(&[4]));
    assert_eq!(attribution.ckks_false_negative_ids, ids(&[2]));
}

#[test]
fn multipattern_correctness_requires_the_approved_lengths_once_each() {
    assert!(validate_search_correctness_pattern_lengths(&[1, 4, 8, 16]));
    assert!(!validate_search_correctness_pattern_lengths(&[1, 4, 8]));
    assert!(!validate_search_correctness_pattern_lengths(&[1, 4, 8, 8]));
    assert!(!validate_search_correctness_pattern_lengths(&[16, 8, 4, 1]));
}

#[test]
fn longer_randomizers_extend_the_same_seeded_draw() {
    let seed = formal_rfps_seed_schedule(8192).unwrap()[0].seed;

    assert_eq!(
        fresh_rfps_weights_for_seed(seed, 1),
        fresh_rfps_weights_for_seed(seed, 16)[..1]
    );
}
