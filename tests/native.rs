#![cfg(feature = "native")]
use cbpir::*;

#[test]
fn native_composition_matches_declared_fixture_outcomes() {
    for case in fixtures() {
        let mut pipeline = Pipeline::prepare(case.records.clone()).unwrap();
        let run = pipeline.run(&case.request).unwrap();
        assert_eq!(
            run.output.as_ref().map(|r| r.id.get()),
            case.expected_id,
            "{}",
            case.name
        );
        assert_eq!(run.fetch_count, 1);
        assert!(run.exact_selection_agreement, "{}", case.name);
        assert_eq!(
            run.semantic_selection_agreement, !case.semantic_negative,
            "{}",
            case.name
        );
        assert_eq!(run.ckks_metrics["openfhe_version"], "1.5.1");
        if let Some(r) = &run.output {
            assert_eq!(
                &r.payload,
                &case
                    .records
                    .iter()
                    .find(|input| input.id == r.id)
                    .unwrap()
                    .payload
            );
        }
        if case.name == "cross_record" {
            assert!(!run.locate.boundary_rejected_positions.is_empty());
        }
        if case.name == "duplicates" {
            assert_eq!(run.locate.duplicate_count, 2);
        }
        assert!(
            run.timings.request_ns
                >= run.timings.match_ns + run.timings.locate_ns + run.timings.fetch_ns
        );
    }
}

#[test]
fn bgv_serialization_roundtrip() {
    let case = fixtures().remove(0);
    let mut p = Pipeline::prepare(case.records).unwrap();
    let sizes = p.serialization_probe().unwrap();
    assert!(sizes["query_bytes"].as_u64().unwrap() > 0);
    assert!(sizes["response_bytes"].as_u64().unwrap() > 0);
    assert!(sizes["evaluation_key_bytes"].is_null());
    assert_eq!(sizes["roundtrip_exact"], true);
}

#[test]
fn native_last_physical_real_window_preserves_full_binary_payload() {
    let mut records: Vec<_> = (1..=63)
        .map(|id| LogicalRecord {
            id: RecordId::new(id).unwrap(),
            searchable: vec![1],
            payload: vec![0; 114],
        })
        .collect();
    records[62].searchable = vec![2];
    records[62].payload = (0_u8..114).collect();
    records[62].payload[113] = 0;
    let mut p = Pipeline::prepare(records.clone()).unwrap();
    let q = MatchRequest {
        pattern: vec![2],
        threshold: 0.5,
        mode: MatchMode::Fps,
    };
    let r = p.run(&q).unwrap();
    assert_eq!(
        r.locate.target,
        FetchTarget::Real {
            id: RecordId::new(63).unwrap(),
            physical_index: 62
        }
    );
    assert_eq!(r.output.unwrap().payload, records[62].payload);
    let q = MatchRequest {
        pattern: vec![3],
        threshold: 0.5,
        mode: MatchMode::Fps,
    };
    let r = p.run(&q).unwrap();
    assert_eq!(r.locate.target, FetchTarget::Dummy { physical_index: 63 });
    assert!(r.output.is_none());
}
