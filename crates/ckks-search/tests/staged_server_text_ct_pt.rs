use cbpir_search::{FpsProfile, RfpsProfile, ServerTextCtPtFpsStaged, ServerTextCtPtRfpsStaged};
use std::fs;

fn unique_artifact_directory(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("p0c-staged-{label}-{}", std::process::id()))
}

#[test]
fn staged_fps_reuses_server_state_and_serializes_actual_messages() {
    let text = b"ababa";
    let pattern = b"aba";
    let threshold = 0.5;
    let transform_len = (text.len() + 1).next_power_of_two();
    let staged = ServerTextCtPtFpsStaged::new(FpsProfile::Fps13, transform_len, threshold)
        .expect("create staged FPS");
    let mut server = staged
        .prepare_server(text)
        .expect("prepare FPS server state");

    let query = staged
        .match_query(text.len(), pattern)
        .expect("create FPS query");
    let response = staged
        .match_response(&mut server, &query)
        .expect("evaluate FPS response");
    let recovered = staged
        .recover_scores(&response, text.len(), pattern.len(), threshold)
        .expect("recover FPS scores");
    assert_eq!(recovered.matching_positions, vec![0, 2]);

    let second_response = staged
        .match_response(&mut server, &query)
        .expect("reuse FPS server state");
    let second = staged
        .recover_scores(&second_response, text.len(), pattern.len(), threshold)
        .expect("recover repeated FPS scores");
    assert_eq!(second.matching_positions, recovered.matching_positions);
    assert!(
        second
            .scores
            .iter()
            .zip(&recovered.scores)
            .all(|(left, right)| (left - right).abs() < 1.0)
    );

    let directory = unique_artifact_directory("fps");
    let sizes = staged
        .serialize_messages(&directory, &query, &response)
        .expect("serialize actual FPS messages");
    assert_eq!(sizes.encrypted_query_ciphertext_bytes.len(), 2);
    assert!(
        sizes
            .encrypted_query_ciphertext_bytes
            .iter()
            .all(|size| *size > 0)
    );
    assert!(sizes.encrypted_response_ciphertext_bytes > 0);
    fs::remove_dir_all(directory).expect("remove targeted test artifacts");
}

#[test]
fn staged_rfps_reuses_server_state_and_preserves_decisions() {
    let text = b"ababa";
    let pattern = b"aba";
    let randomizer = [3, 5, 7];
    let threshold = 0.5;
    let transform_len = (text.len() + 1).next_power_of_two();
    let staged = ServerTextCtPtRfpsStaged::new(RfpsProfile::Rfps13, transform_len, threshold)
        .expect("create staged RFPS");
    let mut server = staged
        .prepare_server(text)
        .expect("prepare RFPS server state");
    let query = staged
        .match_query(text.len(), pattern, &randomizer)
        .expect("create RFPS query");
    let response = staged
        .match_response(&mut server, &query)
        .expect("evaluate RFPS response");
    let recovered = staged
        .recover_scores(&response, text.len(), pattern.len(), threshold)
        .expect("recover RFPS scores");
    assert_eq!(recovered.matching_positions, vec![0, 2]);

    let second_response = staged
        .match_response(&mut server, &query)
        .expect("reuse RFPS server state");
    let second = staged
        .recover_scores(&second_response, text.len(), pattern.len(), threshold)
        .expect("recover repeated RFPS scores");
    assert_eq!(second.matching_positions, recovered.matching_positions);
}
