use cbpir::{MatchMode, realistic};

#[test]
fn realistic_inputs_map_to_existing_pipeline_types() {
    let catalog = realistic::parse_catalog(
        r#"{"schema":"p0d-bgl-k1-study-v1","records":[
        {"id":10,"searchable":[97,98],"payload":[1,0,255]}]}"#,
    )
    .unwrap();
    let records = realistic::logical_records(catalog).unwrap();
    assert_eq!(records[0].id.get(), 10);
    assert_eq!(records[0].searchable, b"ab");

    let fps = realistic::parse_case(
        r#"{"schema":"p0d-bgl-k1-study-v1","id":"fps","pattern_id":"p",
        "pattern_kind":"unique","evidence_class":"natural","pattern":[97],
        "mode":"fps","weight_policy":"none","weight_draw":null,"weights":[],
        "threshold":0.5,"oracle":{}}"#,
    )
    .unwrap();
    assert!(matches!(
        realistic::request(&fps).unwrap().mode,
        MatchMode::Fps
    ));

    let rfps = realistic::parse_case(
        r#"{"schema":"p0d-bgl-k1-study-v1","id":"rfps","pattern_id":"p",
        "pattern_kind":"one_change","evidence_class":"controlled","pattern":[97,98],
        "mode":"rfps","weight_policy":"uniform_3_32","weight_draw":0,
        "weights":[3,32],"threshold":1.0,"oracle":{}}"#,
    )
    .unwrap();
    assert!(matches!(
        realistic::request(&rfps).unwrap().mode,
        MatchMode::Rfps { randomizer } if randomizer == [3, 32]
    ));
}

#[test]
fn realistic_input_validation_rejects_semantic_drift() {
    let bad_schema = realistic::parse_catalog(
        r#"{"schema":"wrong","records":[{"id":10,"searchable":[1],"payload":[1]}]}"#,
    )
    .unwrap();
    assert!(realistic::logical_records(bad_schema).is_err());

    for bad_case in [
        r#"{"schema":"p0d-bgl-k1-study-v1","id":"x","pattern_id":"p",
        "pattern_kind":"unique","evidence_class":"natural","pattern":[1],
        "mode":"fps","weight_policy":"uniform_3_32","weight_draw":0,"weights":[3],
        "threshold":0.5,"oracle":{}}"#,
        r#"{"schema":"p0d-bgl-k1-study-v1","id":"x","pattern_id":"p",
        "pattern_kind":"unique","evidence_class":"natural","pattern":[1,2],
        "mode":"rfps","weight_policy":"uniform_3_32","weight_draw":0,"weights":[3],
        "threshold":0.5,"oracle":{}}"#,
        r#"{"schema":"p0d-bgl-k1-study-v1","id":"x","pattern_id":"p",
        "pattern_kind":"unique","evidence_class":"natural","pattern":[1],
        "mode":"rfps","weight_policy":"uniform_3_32","weight_draw":0,"weights":[33],
        "threshold":0.5,"oracle":{}}"#,
    ] {
        let parsed = realistic::parse_case(bad_case).unwrap();
        assert!(realistic::request(&parsed).is_err());
    }
}
