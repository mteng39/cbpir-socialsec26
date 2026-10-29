use cbpir::*;

fn record(id: u64, text: &[u8], payload: &[u8]) -> LogicalRecord {
    LogicalRecord {
        id: RecordId::new(id).unwrap(),
        searchable: text.to_vec(),
        payload: payload.to_vec(),
    }
}
fn request(pattern: &[u8]) -> MatchRequest {
    MatchRequest {
        pattern: pattern.to_vec(),
        threshold: 0.5,
        mode: MatchMode::Fps,
    }
}

#[test]
fn sorted_layout_maps_first_last_and_rejects_guards() {
    let c = Catalog::new(vec![
        record(30, b"BB", b"last"),
        record(10, b"AA", b"first"),
    ])
    .unwrap();
    assert_eq!(c.text(), b"AA\0BB");
    assert_eq!(c.dummy_index(), 2);
    let trace = locate(&c, &request(b"A"), &[0.0; 5], &[0, 1, 2, 3, 4]).unwrap();
    assert_eq!(trace.boundary_rejected_positions, vec![2]);
    assert_eq!(
        trace
            .candidates
            .iter()
            .map(|x| x.id.get())
            .collect::<Vec<_>>(),
        vec![10, 30]
    );
    assert_eq!(trace.candidates[1].local_offset, 0);
    assert_eq!(trace.duplicate_count, 2);
    assert_eq!(trace.truncated_count, 1);
    assert_eq!(
        trace.target,
        FetchTarget::Real {
            id: RecordId::new(10).unwrap(),
            physical_index: 0
        }
    );
    let crossing = locate(&c, &request(b"A\0B"), &[9.0, 0.0, 9.0], &[1]).unwrap();
    assert_eq!(crossing.boundary_rejected_positions, vec![1]);
    assert_eq!(crossing.target, FetchTarget::Dummy { physical_index: 2 });
}

#[test]
fn acceptance_is_inclusive_and_not_score_ranked() {
    let c = Catalog::new(vec![record(8, b"A", b"a"), record(9, b"A", b"b")]).unwrap();
    let t = locate(&c, &request(b"A"), &[-0.5, 99.0, 0.0], &[0, 2]).unwrap();
    assert_eq!(t.candidates[0].id.get(), 8);
    assert_eq!(PUBLIC_K, 1);
}

#[test]
fn injected_crossings_change_selection_in_both_directions() {
    let c = Catalog::new(vec![record(10, b"b", b"false"), record(20, b"a", b"true")]).unwrap();
    let q = request(b"a");
    let exact = oracle(&c, &q).unwrap();
    assert_eq!(exact.exact_ids[0].get(), 20);
    let fp = locate(&c, &q, &[0.49, 9409.0, 0.0], &[0, 2]).unwrap();
    assert_eq!(fp.candidates[0].id.get(), 10);
    let missed = locate(&c, &q, &[1.0, 9409.0, 0.51], &[]).unwrap();
    assert_eq!(missed.target, FetchTarget::Dummy { physical_index: 2 });
}

#[test]
fn boundary_capacity_and_long_pattern_without_in_record_windows() {
    let c = Catalog::new((1..=63).map(|id| record(id, b"x", &[0; 114])).collect()).unwrap();
    assert_eq!(c.dummy_index(), 63);
    let trace = locate(
        &c,
        &request(b"x"),
        &vec![0.0; c.text().len()],
        &(0..c.text().len()).collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(trace.candidates.len(), 63);
    assert_eq!(trace.candidates.last().unwrap().physical_index, 62);
    let q = request(b"x\0x");
    let o = oracle(&c, &q).unwrap();
    assert!(o.windows.is_empty());
    let max = Catalog::new(vec![record(1, &[255; 1024], b"")]).unwrap();
    assert!(max.validate(&request(&[255; 64])).is_ok());
    assert_eq!(
        oracle(&max, &request(&[0; 64])).unwrap().windows[0].exact_score,
        4_161_600
    );
}

#[test]
fn frame_preserves_zero_bytes_and_rejects_corruption() {
    let r = record(42, b"text", b"A\0B\0");
    let f = encode_frame(&r).unwrap();
    assert_eq!(f.len(), 128);
    assert_eq!(&f[..4], b"P0D1");
    assert_eq!(&f[4..12], &42_u64.to_le_bytes());
    assert_eq!(&f[12..14], &4_u16.to_le_bytes());
    assert_eq!(decode_frame(&f, r.id).unwrap().payload, r.payload);
    for index in [0, 4, 12, 127] {
        let mut bad = f;
        bad[index] = 255;
        assert!(decode_frame(&bad, r.id).is_err());
    }
    let mut zero = f;
    zero[4..12].fill(0);
    assert!(decode_frame(&zero, r.id).is_err());
    assert!(decode_frame(&f[..127], r.id).is_err());
    for payload in [vec![], vec![255; 114]] {
        let r = record(1, b"x", &payload);
        assert_eq!(
            decode_frame(&encode_frame(&r).unwrap(), r.id)
                .unwrap()
                .payload,
            payload
        );
    }
}

#[test]
fn malformed_catalog_and_request_inputs_fail() {
    assert!(RecordId::new(0).is_err());
    assert!(Catalog::new(vec![]).is_err());
    assert!(Catalog::new(vec![record(1, b"x", b""), record(1, b"y", b"")]).is_err());
    assert!(Catalog::new(vec![record(1, b"", b"")]).is_err());
    assert!(Catalog::new(vec![record(1, b"x", &[0; 115])]).is_err());
    assert!(Catalog::new(vec![record(1, &[1; 1025], b"")]).is_ok());
    assert!(Catalog::new((1..=256).map(|i| record(i, b"x", b"")).collect()).is_ok());
    let c = Catalog::new(vec![record(1, b"abc", b"")]).unwrap();
    for pattern in [vec![], vec![0; 4], vec![0; 65]] {
        assert!(c.validate(&request(&pattern)).is_err());
    }
    for threshold in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut q = request(b"a");
        q.threshold = threshold;
        assert!(c.validate(&q).is_err());
    }
    for randomizer in [vec![], vec![0], vec![33], vec![1, 1]] {
        let mut q = request(b"a");
        q.mode = MatchMode::Rfps { randomizer };
        assert!(c.validate(&q).is_err());
    }
}

#[test]
fn malformed_match_contract_and_targets_fail() {
    let c = Catalog::new(vec![record(10, b"ab", b"")]).unwrap();
    let q = request(b"a");
    for (scores, positions) in [
        (vec![0.0], vec![0]),
        (vec![f64::NAN, 0.0], vec![1]),
        (vec![f64::INFINITY, 0.0], vec![1]),
        (vec![0.0, 0.0], vec![1, 0]),
        (vec![0.0, 0.0], vec![0, 0]),
        (vec![0.0, 0.0], vec![0, 2]),
        (vec![0.0, 0.0], vec![0]),
    ] {
        assert!(locate(&c, &q, &scores, &positions).is_err());
    }
    for target in [
        FetchTarget::Dummy { physical_index: 0 },
        FetchTarget::Dummy { physical_index: 2 },
        FetchTarget::Real {
            id: RecordId::new(99).unwrap(),
            physical_index: 0,
        },
        FetchTarget::Real {
            id: RecordId::new(10).unwrap(),
            physical_index: 1,
        },
    ] {
        assert!(c.validate_target(target).is_err());
    }
}

#[test]
fn oracle_distinguishes_semantic_matches_cancellation_threshold_and_ckks_errors() {
    let c = Catalog::new(vec![record(1, &[1, 2, 3], b"payload")]).unwrap();
    let o = oracle(&c, &request(&[2, 3])).unwrap();
    assert_eq!(
        o.windows.iter().map(|w| w.exact_score).collect::<Vec<_>>(),
        vec![2, 0]
    );
    assert_eq!(o.semantic_ids, vec![RecordId::new(1).unwrap()]);
    let c = Catalog::new(vec![record(1, &[2, 0], b"cancel")]).unwrap();
    let q = MatchRequest {
        pattern: vec![1, 1],
        threshold: 0.5,
        mode: MatchMode::Rfps {
            randomizer: vec![1, 1],
        },
    };
    let o = oracle(&c, &q).unwrap();
    assert!(o.semantic_ids.is_empty());
    assert_eq!(o.exact_ids.len(), 1);
    assert!(compare(&o, &[0.0], q.threshold).unwrap()[0].intrinsic_cancellation);
    let c = Catalog::new(vec![record(1, &[2], b"near")]).unwrap();
    let mut q = request(&[1]);
    let o = oracle(&c, &q).unwrap();
    assert!(compare(&o, &[0.49], 0.5).unwrap()[0].ckks_false_positive);
    q.threshold = 4.0;
    let o = oracle(&c, &q).unwrap();
    assert!(compare(&o, &[1.0], 4.0).unwrap()[0].threshold_only);
    let o = oracle(&c, &request(&[2])).unwrap();
    assert!(compare(&o, &[0.51], 0.5).unwrap()[0].ckks_false_negative);
}
