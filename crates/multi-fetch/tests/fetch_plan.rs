use cbpir_fetch::{
    FetchError, FetchPlanner, RecordId, SUPPORTED_PUBLIC_K, discard_dummy_responses,
};

const PHYSICAL_CAPACITY: usize = 64;

fn planner() -> FetchPlanner {
    FetchPlanner::new(10, PHYSICAL_CAPACITY, 4).expect("valid bounded layout")
}

#[test]
fn zero_candidates_selects_k_distinct_valid_dummy_windows() {
    let plan = planner().plan(&[]).expect("valid empty candidate plan");

    assert_eq!(
        plan.physical_ids(),
        &[RecordId(10), RecordId(11), RecordId(12), RecordId(13)]
    );
    assert_eq!(plan.is_dummy(), &[true, true, true, true]);
    assert_eq!(plan.selected_real_count(), 0);
    assert_eq!(plan.truncated_unique_count(), 0);
    assert_eq!(plan.public_k(), 4);
}

#[test]
fn fewer_than_k_candidates_keep_order_and_use_distinct_dummies() {
    let plan = planner()
        .plan(&[RecordId(9), RecordId(0)])
        .expect("valid plan");

    assert_eq!(
        plan.physical_ids(),
        &[RecordId(9), RecordId(0), RecordId(10), RecordId(11)]
    );
    assert_eq!(plan.is_dummy(), &[false, false, true, true]);
    assert_eq!(plan.selected_real_count(), 2);
}

#[test]
fn exactly_k_candidates_need_no_dummy_selection() {
    let plan = planner()
        .plan(&[RecordId(0), RecordId(1), RecordId(8), RecordId(9)])
        .expect("valid plan");

    assert_eq!(
        plan.physical_ids(),
        &[RecordId(0), RecordId(1), RecordId(8), RecordId(9)]
    );
    assert_eq!(plan.is_dummy(), &[false, false, false, false]);
    assert_eq!(plan.truncated_unique_count(), 0);
}

#[test]
fn stable_deduplication_precedes_truncation() {
    let plan = planner()
        .plan(&[
            RecordId(3),
            RecordId(1),
            RecordId(3),
            RecordId(7),
            RecordId(2),
            RecordId(9),
        ])
        .expect("valid plan");

    assert_eq!(
        plan.physical_ids(),
        &[RecordId(3), RecordId(1), RecordId(7), RecordId(2)]
    );
    assert_eq!(plan.selected_real_count(), 4);
    assert_eq!(plan.truncated_unique_count(), 1);
}

#[test]
fn invalid_id_is_rejected_even_after_selected_prefix() {
    let error = planner()
        .plan(&[
            RecordId(0),
            RecordId(1),
            RecordId(2),
            RecordId(3),
            RecordId(10),
        ])
        .expect_err("trailing invalid ID must fail before truncation");

    assert_eq!(
        error,
        FetchError::CandidateOutOfRange {
            id: RecordId(10),
            real_record_count: 10,
        }
    );
}

#[test]
fn supported_k_values_reserve_exact_distinct_dummy_capacity() {
    assert_eq!(SUPPORTED_PUBLIC_K, [1, 4, 8, 16]);

    for public_k in SUPPORTED_PUBLIC_K {
        let real_record_count = PHYSICAL_CAPACITY - public_k;
        let planner = FetchPlanner::new(real_record_count, PHYSICAL_CAPACITY, public_k)
            .expect("declared layout fits");
        let plan = planner.plan(&[]).expect("all-dummy plan");

        assert_eq!(plan.physical_ids().len(), public_k);
        assert_eq!(plan.physical_ids()[0], RecordId(real_record_count as u64));
        assert_eq!(
            plan.physical_ids()[public_k - 1],
            RecordId((PHYSICAL_CAPACITY - 1) as u64)
        );
        assert!(plan.is_dummy().iter().all(|&value| value));
    }
}

#[test]
fn malformed_layouts_and_unsupported_k_are_rejected() {
    assert_eq!(
        FetchPlanner::new(0, PHYSICAL_CAPACITY, 4),
        Err(FetchError::EmptyRealDatabase)
    );
    for public_k in [0, 2, 3, 17] {
        assert_eq!(
            FetchPlanner::new(1, PHYSICAL_CAPACITY, public_k),
            Err(FetchError::UnsupportedPublicK { public_k })
        );
    }
    assert_eq!(
        FetchPlanner::new(61, PHYSICAL_CAPACITY, 4),
        Err(FetchError::InsufficientPhysicalCapacity {
            real_record_count: 61,
            public_k: 4,
            physical_capacity: PHYSICAL_CAPACITY,
        })
    );
}

#[test]
fn dummy_responses_are_removed_without_confusing_zero_real_records() {
    let plan = planner()
        .plan(&[RecordId(9), RecordId(0)])
        .expect("valid plan");
    let responses = vec![vec![9, 0], vec![0, 0], vec![0, 0], vec![0, 0]];
    let fetched = discard_dummy_responses(&plan, &responses).expect("matching response count");

    assert_eq!(fetched.len(), 2);
    assert_eq!(fetched[0].id(), RecordId(9));
    assert_eq!(fetched[0].bytes(), &[9, 0]);
    assert_eq!(fetched[1].id(), RecordId(0));
    assert_eq!(fetched[1].bytes(), &[0, 0]);
}

#[test]
fn malformed_response_count_is_rejected() {
    let plan = planner().plan(&[]).expect("valid plan");

    assert_eq!(
        discard_dummy_responses(&plan, &vec![vec![0]; 3]),
        Err(FetchError::ResponseCount {
            expected: 4,
            actual: 3,
        })
    );
}
