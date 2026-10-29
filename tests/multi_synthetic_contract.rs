use cbpir::{Catalog, MatchMode, MatchRequest, locate_many, multi_fixtures};

fn exact_full_text_scores(text: &[u8], request: &MatchRequest) -> Vec<f64> {
    text.windows(request.pattern.len())
        .map(|window| {
            window
                .iter()
                .zip(&request.pattern)
                .enumerate()
                .map(|(index, (&actual, &expected))| {
                    let difference = i32::from(actual) - i32::from(expected);
                    match &request.mode {
                        MatchMode::Fps => difference * difference,
                        MatchMode::Rfps { randomizer } => difference * i32::from(randomizer[index]),
                    }
                })
                .sum::<i32>()
        })
        .map(f64::from)
        .collect()
}

#[test]
fn frozen_k4_fixture_matrix_has_expected_client_decisions() {
    let fixtures = multi_fixtures();
    assert_eq!(fixtures.len(), 14);
    assert_eq!(
        fixtures
            .iter()
            .map(|fixture| fixture.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "zero_matches",
            "one_record",
            "fewer_than_k",
            "exactly_k",
            "more_than_k",
            "one_record_many_occurrences",
            "duplicates_across_records",
            "adjacent_records",
            "nonadjacent_records",
            "first_and_last_records",
            "cross_record",
            "rfps_false_only_cancellation",
            "rfps_false_competes",
            "rfps_threshold_only",
        ]
    );

    for fixture in fixtures {
        let catalog = Catalog::new(fixture.records).unwrap();
        let scores = exact_full_text_scores(catalog.text(), &fixture.request);
        let positions = scores
            .iter()
            .enumerate()
            .filter_map(|(position, score)| {
                (score.abs() <= fixture.request.threshold).then_some(position)
            })
            .collect::<Vec<_>>();
        let trace = locate_many(
            &catalog,
            &fixture.request,
            &scores,
            &positions,
            fixture.public_k,
        )
        .unwrap();

        assert_eq!(
            trace
                .candidates
                .iter()
                .map(|candidate| candidate.id.get())
                .collect::<Vec<_>>(),
            fixture.expected_candidate_ids,
            "{} candidates",
            fixture.name
        );
        assert_eq!(
            trace
                .selected_candidates
                .iter()
                .map(|candidate| candidate.id.get())
                .collect::<Vec<_>>(),
            fixture.expected_selected_ids,
            "{} selection",
            fixture.name
        );
        assert_eq!(
            trace.boundary_rejected_positions.len(),
            fixture.expected_boundary_rejections,
            "{} boundary rejections",
            fixture.name
        );
        assert_eq!(
            trace.duplicate_count, fixture.expected_duplicate_count,
            "{} duplicates",
            fixture.name
        );
        assert_eq!(
            trace.truncated_count, fixture.expected_truncated_count,
            "{} truncation",
            fixture.name
        );
    }
}

#[test]
fn locate_many_propagates_an_injected_match_decision_without_calling_it_fetch_failure() {
    let fixture = multi_fixtures()
        .into_iter()
        .find(|fixture| fixture.name == "zero_matches")
        .unwrap();
    let catalog = Catalog::new(fixture.records).unwrap();
    let expected = catalog.text().len() - fixture.request.pattern.len() + 1;
    let mut injected_scores = vec![10.0; expected];
    injected_scores[0] = 0.0;

    let trace = locate_many(
        &catalog,
        &fixture.request,
        &injected_scores,
        &[0],
        fixture.public_k,
    )
    .unwrap();

    assert_eq!(trace.selected_candidates.len(), 1);
    assert_eq!(trace.truncated_count, 0);
}
