#![cfg(feature = "native")]

use cbpir::{
    Catalog, LogicalRecord, MatchMode, MatchRequest, NoDummyFetch, Phase2Profile, RecordId,
    locate_many, run_phase2_match,
};
use cbpir_bgv::BgvProfile;

fn records(count: usize) -> Vec<LogicalRecord> {
    (0..count)
        .map(|index| LogicalRecord {
            id: RecordId::new(10_000 + index as u64).unwrap(),
            searchable: format!("search-{index:03}").into_bytes(),
            payload: format!("payload-{index:03}").into_bytes(),
        })
        .collect()
}

#[test]
fn no_dummy_fetch_has_one_constant_size_query_and_response_for_every_cardinality() {
    let records = records(64);
    let mut fetch = NoDummyFetch::prepare(&records, BgvProfile::Bgv13).unwrap();
    let selections = [
        vec![],
        vec![0],
        vec![0, 7, 63],
        vec![0, 7, 31, 63],
        vec![0, 7, 15, 31, 63],
    ];
    let directory = tempfile::tempdir().unwrap();
    let mut query_sizes = Vec::new();
    let mut response_sizes = Vec::new();

    for (case_index, selected) in selections.iter().enumerate() {
        let case_directory = directory.path().join(case_index.to_string());
        let trace = fetch.run(selected, &case_directory).unwrap();
        assert_eq!(trace.query_ciphertext_count, 1);
        assert_eq!(trace.response_ciphertext_count, 1);
        assert!(trace.selected_payloads_exact);
        assert!(trace.unselected_windows_zero);
        assert_eq!(trace.empty_response_all_zero, selected.is_empty());
        assert_eq!(trace.outputs.len(), selected.len());
        assert_eq!(
            trace
                .outputs
                .iter()
                .map(|record| record.id)
                .collect::<Vec<_>>(),
            selected
                .iter()
                .map(|&index| records[index].id)
                .collect::<Vec<_>>()
        );
        query_sizes.push(trace.query_bytes);
        response_sizes.push(trace.response_bytes);
    }

    assert!(query_sizes.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(response_sizes.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(fetch.preparation().physical_dummy_window_count, 0);
    assert_eq!(fetch.preparation().real_record_count, 64);
    assert_eq!(fetch.preparation().physical_capacity, 64);
}

#[test]
fn no_dummy_fetch_enforces_profile_capacity_instead_of_catalog_constant() {
    let error = NoDummyFetch::prepare(&records(65), BgvProfile::Bgv13)
        .err()
        .expect("65 records exceed BGV-13 capacity");
    assert!(error.to_string().contains("capacity is 64"));
}

#[test]
fn no_dummy_fetch_rejects_unencoded_physical_window() {
    let records = records(1);
    let mut fetch = NoDummyFetch::prepare(&records, BgvProfile::Bgv13).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let error = fetch
        .run(&[1], directory.path())
        .expect_err("physical slot 1 has no real record");
    assert!(error.to_string().contains("outside real-record count 1"));
}

#[test]
fn explicit_phase2_match_profiles_feed_locate_without_changing_semantics() {
    let catalog = Catalog::new(vec![
        LogicalRecord {
            id: RecordId::new(20).unwrap(),
            searchable: b"beta common".to_vec(),
            payload: b"payload-20".to_vec(),
        },
        LogicalRecord {
            id: RecordId::new(10).unwrap(),
            searchable: b"alpha common".to_vec(),
            payload: b"payload-10".to_vec(),
        },
    ])
    .unwrap();
    let profile = Phase2Profile::for_ring_dimension(8192).unwrap();
    let fps = MatchRequest {
        pattern: b"common".to_vec(),
        threshold: 0.5,
        mode: MatchMode::Fps,
    };
    let fps_match = run_phase2_match(&catalog, &fps, profile).unwrap();
    assert_eq!(fps_match.backend, "server_text_ct_pt");
    assert_eq!(fps_match.profile, "FPS-13");
    assert_eq!(fps_match.decision_mismatches, 0);
    let located = locate_many(
        &catalog,
        &fps,
        &fps_match.scores,
        &fps_match.matching_positions,
        1,
    )
    .unwrap();
    assert_eq!(
        located.selected_candidates[0].id,
        RecordId::new(10).unwrap()
    );

    let rfps = MatchRequest {
        pattern: vec![1, 1],
        threshold: 0.5,
        mode: MatchMode::Rfps {
            randomizer: vec![1, 1],
        },
    };
    let cancellation_catalog = Catalog::new(vec![LogicalRecord {
        id: RecordId::new(30).unwrap(),
        searchable: vec![2, 0],
        payload: b"cancel".to_vec(),
    }])
    .unwrap();
    let rfps_match = run_phase2_match(&cancellation_catalog, &rfps, profile).unwrap();
    assert_eq!(rfps_match.profile, "RFPS-13");
    assert!(rfps_match.intrinsic_cancellation_count > 0);
}
