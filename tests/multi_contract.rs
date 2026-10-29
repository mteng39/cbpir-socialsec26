use cbpir::{Catalog, LogicalRecord, MatchMode, MatchRequest, RecordId, locate_many};

fn record(id: u64, searchable: &[u8]) -> LogicalRecord {
    LogicalRecord {
        id: RecordId::new(id).expect("nonzero test ID"),
        searchable: searchable.to_vec(),
        payload: format!("payload-{id}").into_bytes(),
    }
}

fn fps(pattern: &[u8]) -> MatchRequest {
    MatchRequest {
        pattern: pattern.to_vec(),
        threshold: 0.5,
        mode: MatchMode::Fps,
    }
}

fn scores_with_accepts(expected: usize, positions: &[usize]) -> Vec<f64> {
    let mut scores = vec![9.0; expected];
    for &position in positions {
        scores[position] = 0.0;
    }
    scores
}

#[test]
fn locate_many_orders_stable_ids_deduplicates_and_truncates() {
    let catalog = Catalog::new(vec![
        record(90, b"aa"),
        record(10, b"aa"),
        record(70, b"aa"),
        record(30, b"aa"),
        record(50, b"aa"),
    ])
    .unwrap();
    let request = fps(b"a");
    let positions: Vec<_> = catalog
        .spans()
        .iter()
        .flat_map(|span| [span.start, span.start + 1])
        .collect();
    let scores = scores_with_accepts(catalog.text().len(), &positions);

    let trace = locate_many(&catalog, &request, &scores, &positions, 4).unwrap();

    assert_eq!(trace.duplicate_count, 5);
    assert_eq!(trace.truncated_count, 1);
    assert_eq!(
        trace
            .candidates
            .iter()
            .map(|candidate| candidate.id.get())
            .collect::<Vec<_>>(),
        vec![10, 30, 50, 70, 90]
    );
    assert_eq!(
        trace
            .selected_candidates
            .iter()
            .map(|candidate| candidate.id.get())
            .collect::<Vec<_>>(),
        vec![10, 30, 50, 70]
    );
    assert!(
        trace
            .candidates
            .iter()
            .all(|candidate| candidate.local_offset == 0)
    );
}

#[test]
fn locate_many_allows_empty_real_selection_and_rejects_boundaries() {
    let catalog = Catalog::new(vec![record(8, b"ab"), record(3, b"cd")]).unwrap();
    let request = fps(b"bc");
    let crossing_position = catalog.spans()[0].end - 1;
    let expected = catalog.text().len() - request.pattern.len() + 1;
    let positions = vec![crossing_position];
    let scores = scores_with_accepts(expected, &positions);

    let trace = locate_many(&catalog, &request, &scores, &positions, 4).unwrap();

    assert!(trace.candidates.is_empty());
    assert!(trace.selected_candidates.is_empty());
    assert_eq!(trace.boundary_rejected_positions, positions);
    assert_eq!(trace.truncated_count, 0);
}

#[test]
fn locate_many_rejects_non_multi_capacity() {
    let catalog = Catalog::new(vec![record(1, b"abc")]).unwrap();
    let request = fps(b"z");
    let scores = vec![9.0; 3];

    let k1 = locate_many(&catalog, &request, &scores, &[], 1).unwrap();
    assert_eq!(k1.public_k, 1);
    assert!(k1.selected_candidates.is_empty());

    for unsupported in [0, 2, 3, 5, 17] {
        let error = locate_many(&catalog, &request, &scores, &[], unsupported).unwrap_err();
        assert_eq!(
            error.to_string(),
            "request: public K must be 1, 4, 8, or 16"
        );
    }
}
