use std::collections::BTreeMap;

use cbpir::{realistic, realistic_kgt1};

const CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/fixtures/bgl-k1-study/catalog.json"
));
const CASES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/fixtures/bgl-k1-study/cases.jsonl"
));

#[test]
fn frozen_bgl_oracle_has_expected_k4_workload_shape() {
    let catalog = realistic::parse_catalog(CATALOG).unwrap();
    realistic::validate_fixed_catalog(&catalog).unwrap();
    let cases = realistic::parse_cases(CASES).unwrap();
    realistic::validate_fixed_cases(&cases).unwrap();

    let valid_ids = catalog
        .records
        .iter()
        .map(|record| record.id)
        .collect::<Vec<_>>();
    let mut distribution = BTreeMap::new();
    let mut literal_multi_requests = 0;
    let mut over_k_requests = 0;
    let mut boundary_requests = 0;
    let mut dedup_requests = 0;

    for case in &cases {
        let expected = realistic_kgt1::expected(case, 4).unwrap();
        *distribution
            .entry(expected.candidate_ids.len())
            .or_insert(0) += 1;
        literal_multi_requests += usize::from(expected.literal_ids.len() > 1);
        over_k_requests += usize::from(expected.candidate_ids.len() > 4);
        boundary_requests += usize::from(!expected.boundary_positions.is_empty());
        dedup_requests += usize::from(expected.duplicate_candidate_offsets > 0);
        assert!(
            expected
                .candidate_ids
                .iter()
                .all(|id| valid_ids.contains(id))
        );
        assert_eq!(
            expected.selected_ids,
            expected
                .candidate_ids
                .iter()
                .copied()
                .take(4)
                .collect::<Vec<_>>()
        );
    }

    assert_eq!(
        distribution,
        BTreeMap::from([
            (0, 257),
            (1, 225),
            (2, 149),
            (3, 7),
            (4, 3),
            (5, 1),
            (10, 1)
        ])
    );
    assert_eq!(literal_multi_requests, 147);
    assert_eq!(over_k_requests, 2);
    assert_eq!(boundary_requests, 93);
    assert_eq!(dedup_requests, 54);
}
