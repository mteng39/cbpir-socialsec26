use std::collections::HashSet;

use cbpir_bgv::BgvCpirConfig;
use cbpir_fetch::{
    FetchError, FetchPlanner, P3BackendError, P3BgvBackend, RecordId, SUPPORTED_PUBLIC_K,
};

const RING_DIMENSION: usize = 8192;
const RECORD_SLOTS: usize = 128;
const PHYSICAL_CAPACITY: usize = RING_DIMENSION / RECORD_SLOTS;

fn record(id: usize) -> Vec<u8> {
    let mut bytes = (0..RECORD_SLOTS)
        .map(|offset| {
            u8::try_from((id * 131 + offset * 17 + offset / 7) % 251)
                .expect("value is reduced modulo 251")
        })
        .collect::<Vec<_>>();
    let id_byte = u8::try_from(id).expect("test IDs fit one byte");
    bytes[0] = id_byte;
    bytes[1] = 0;
    bytes[RECORD_SLOTS - 1] = id_byte ^ 0xa5;
    bytes
}

fn records(real_record_count: usize) -> Vec<Vec<u8>> {
    let mut records = (0..real_record_count).map(record).collect::<Vec<_>>();
    records[0] = vec![0; RECORD_SLOTS];
    records
}

fn assert_fetched(
    result: &cbpir_fetch::P3FetchResult,
    expected_ids: &[RecordId],
    source: &[Vec<u8>],
) {
    assert_eq!(result.records.len(), expected_ids.len());
    for (fetched, &expected_id) in result.records.iter().zip(expected_ids) {
        assert_eq!(fetched.id(), expected_id);
        let expected_index = usize::try_from(expected_id.0).expect("test ID fits usize");
        assert_eq!(fetched.bytes(), source[expected_index]);
    }
}

fn assert_slot_isolation(
    result: &cbpir_fetch::P3FetchResult,
    plan: &cbpir_fetch::FetchPlan,
    source: &[Vec<u8>],
) {
    assert_eq!(result.decrypted_slots.len(), RING_DIMENSION);
    let selected = plan
        .physical_ids()
        .iter()
        .map(|id| usize::try_from(id.0).expect("test ID fits usize"))
        .collect::<HashSet<_>>();
    for physical_id in 0..PHYSICAL_CAPACITY {
        let start = physical_id * RECORD_SLOTS;
        let window = &result.decrypted_slots[start..start + RECORD_SLOTS];
        if selected.contains(&physical_id) && physical_id < source.len() {
            assert_eq!(
                window,
                source[physical_id]
                    .iter()
                    .copied()
                    .map(u64::from)
                    .collect::<Vec<_>>()
            );
        } else {
            assert!(window.iter().all(|&slot| slot == 0));
        }
    }
}

#[test]
fn all_supported_k_fetch_zero_fewer_exact_and_truncated_candidates() {
    for public_k in SUPPORTED_PUBLIC_K {
        let real_record_count = PHYSICAL_CAPACITY - public_k;
        let source = records(real_record_count);
        let (mut backend, setup) =
            P3BgvBackend::new(&source, public_k, BgvCpirConfig::prior_mini())
                .expect("declared P3 configuration");
        assert!(setup.context_generation_ns > 0);
        assert!(setup.key_generation_ns > 0);
        assert!(setup.database_assembly_and_encode_ns > 0);

        let empty_plan = backend.planner().plan(&[]).expect("all-dummy plan");
        let empty_result = backend.fetch(&empty_plan).expect("all-dummy fetch");
        assert!(empty_result.records.is_empty());
        assert_eq!(empty_plan.physical_ids().len(), public_k);
        assert_eq!(
            empty_plan.physical_ids()[public_k - 1],
            RecordId((PHYSICAL_CAPACITY - 1) as u64)
        );
        assert_slot_isolation(&empty_result, &empty_plan, &source);

        let fewer_count = public_k.saturating_sub(1).max(1).min(real_record_count);
        let fewer_ids = (0..fewer_count)
            .map(|index| RecordId(index as u64))
            .collect::<Vec<_>>();
        let fewer_plan = backend.planner().plan(&fewer_ids).expect("padded plan");
        let fewer_result = backend.fetch(&fewer_plan).expect("padded fetch");
        assert_fetched(&fewer_result, &fewer_ids, &source);
        assert_slot_isolation(&fewer_result, &fewer_plan, &source);

        let exact_ids = (0..public_k)
            .map(|offset| {
                let id = if offset == 0 {
                    real_record_count - 1
                } else {
                    offset - 1
                };
                RecordId(id as u64)
            })
            .collect::<Vec<_>>();
        let exact_plan = backend.planner().plan(&exact_ids).expect("exact plan");
        let exact_result = backend.fetch(&exact_plan).expect("exact fetch");
        assert_fetched(&exact_result, &exact_ids, &source);
        assert_slot_isolation(&exact_result, &exact_plan, &source);

        let mut over_ids = exact_ids.clone();
        over_ids.push(RecordId(public_k as u64));
        over_ids.push(exact_ids[0]);
        let over_plan = backend.planner().plan(&over_ids).expect("truncated plan");
        assert_eq!(over_plan.truncated_unique_count(), 1);
        let over_result = backend.fetch(&over_plan).expect("truncated fetch");
        assert_fetched(&over_result, &exact_ids, &source);
    }
}

#[test]
fn adjacent_and_non_adjacent_windows_reconstruct_in_requested_order() {
    let source = records(PHYSICAL_CAPACITY - 4);
    let (mut backend, _) =
        P3BgvBackend::new(&source, 4, BgvCpirConfig::prior_mini()).expect("valid backend");

    for ids in [
        vec![RecordId(0), RecordId(1), RecordId(2), RecordId(3)],
        vec![RecordId(59), RecordId(0), RecordId(30), RecordId(7)],
    ] {
        let plan = backend.planner().plan(&ids).expect("valid plan");
        let result = backend.fetch(&plan).expect("valid fetch");
        assert_fetched(&result, &ids, &source);
        assert_slot_isolation(&result, &plan, &source);
    }
}

#[test]
fn constructor_and_fetch_reject_malformed_width_capacity_and_layout() {
    let empty = P3BgvBackend::new(&[], 4, BgvCpirConfig::prior_mini());
    assert!(matches!(
        empty,
        Err(P3BackendError::Fetch(FetchError::EmptyRealDatabase))
    ));

    let mut wrong_width = records(2);
    wrong_width[1].pop();
    let wrong_width_result = P3BgvBackend::new(&wrong_width, 4, BgvCpirConfig::prior_mini());
    assert!(matches!(
        wrong_width_result,
        Err(P3BackendError::RecordWidth {
            index: 1,
            actual: 127,
            expected: 128,
        })
    ));

    let too_many = records(PHYSICAL_CAPACITY - 3);
    let too_many_result = P3BgvBackend::new(&too_many, 4, BgvCpirConfig::prior_mini());
    assert!(matches!(
        too_many_result,
        Err(P3BackendError::Fetch(
            FetchError::InsufficientPhysicalCapacity { .. }
        ))
    ));

    let source = records(10);
    let (mut backend, _) =
        P3BgvBackend::new(&source, 4, BgvCpirConfig::prior_mini()).expect("valid backend");
    let foreign_plan = FetchPlanner::new(9, PHYSICAL_CAPACITY, 4)
        .expect("different valid layout")
        .plan(&[RecordId(0)])
        .expect("valid foreign plan");
    assert!(matches!(
        backend.fetch(&foreign_plan),
        Err(P3BackendError::PlanLayoutMismatch { .. })
    ));
}

#[test]
fn serialized_multi_window_response_round_trip_preserves_ordered_records() {
    let source = records(PHYSICAL_CAPACITY - 4);
    let (mut backend, _) =
        P3BgvBackend::new(&source, 4, BgvCpirConfig::prior_mini()).expect("valid backend");
    let ids = [RecordId(59), RecordId(0), RecordId(30)];
    let plan = backend.planner().plan(&ids).expect("padded plan");
    let directory = tempfile::tempdir().expect("temporary artifact directory");

    let (result, bundle) = backend
        .fetch_and_serialize(&plan, directory.path())
        .expect("one serializable P3 exchange");
    assert!(bundle.sizes.context_bytes > 0);
    assert!(bundle.sizes.public_key_bytes > 0);
    assert!(bundle.sizes.private_key_bytes > 0);
    assert!(bundle.sizes.query_bytes > 0);
    assert!(bundle.sizes.response_bytes > 0);
    assert_eq!(bundle.sizes.evaluation_key_bytes, None);

    let round_trip_slots = cbpir_bgv::BgvCpir::decrypt_serialized_response(&bundle.paths)
        .expect("fresh objects decrypt serialized response");
    assert_eq!(round_trip_slots, result.decrypted_slots);
    let round_trip_records = backend
        .decode_plan_slots(&plan, &round_trip_slots)
        .expect("decode serialized P3 response");
    assert_eq!(round_trip_records, result.records);
    assert_fetched(&result, &ids, &source);
}

#[test]
fn fixed_seed_p3_response_equals_k_repeated_p0a_responses() {
    let mut state = 0x7a5b_31d9_c4e2_806f_u64;

    for public_k in SUPPORTED_PUBLIC_K {
        let real_record_count = PHYSICAL_CAPACITY - public_k;
        let source = records(real_record_count);
        let (mut backend, _) = P3BgvBackend::new(&source, public_k, BgvCpirConfig::prior_mini())
            .expect("valid backend");

        for _ in 0..8 {
            let candidate_count = public_k + 3;
            let candidates = (0..candidate_count)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    RecordId(state % u64::try_from(real_record_count).unwrap())
                })
                .collect::<Vec<_>>();
            let plan = backend
                .planner()
                .plan(&candidates)
                .expect("valid candidates");
            let p3 = backend.fetch(&plan).expect("one-query P3 response");

            let mut expected = vec![0_u64; backend.kernel().actual_ring_dimension()];
            for id in plan.physical_ids() {
                let index = usize::try_from(id.0).expect("physical ID fits usize");
                let query = backend
                    .kernel()
                    .encrypt_query(index)
                    .expect("valid repeated P0-A index");
                let response = backend.answer_p0a_query(&query);
                let slots = backend
                    .kernel()
                    .decrypt_slots(&response)
                    .expect("valid repeated P0-A response");
                for (expected_slot, slot) in expected.iter_mut().zip(slots) {
                    *expected_slot += slot;
                }
            }

            assert_eq!(p3.decrypted_slots, expected);
        }
    }
}
