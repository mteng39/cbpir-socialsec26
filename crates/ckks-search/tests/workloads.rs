use cbpir_search::{fixture_sha256, proposal_workloads};

const EXPECTED_FIXTURE_SHA256: &str =
    "5c3ac1b262acd67f9b64e3147e5e6ba9e2c54b7bcc484caedabde4a1d775bd77";

#[test]
fn loads_the_canonical_proposal_workload_fixture() {
    let fixture = proposal_workloads().expect("canonical fixture");

    assert_eq!(fixture.schema, "p0-search-workloads-v1");
    assert_eq!(fixture.synthetic_cases.len(), 24);
    assert_eq!(fixture.rfps_probability.trials.len(), 16_384);
    assert_eq!(fixture.rfps_probability.true_match_controls.len(), 1_024);
    assert_eq!(fixture.bgl.cases.len(), 4);
    assert_eq!(fixture_sha256(), EXPECTED_FIXTURE_SHA256);
}
