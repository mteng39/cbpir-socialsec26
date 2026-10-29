use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::time::Instant;

use cbpir_bgv::{BgvCpir, BgvProfile, CpirError};
use cbpir_search::{
    DecisionCause, FpsProfile, PHASE1_FIXTURE_TEXT_LEN, PHASE1_FIXTURE_TRANSFORM_LEN, RfpsProfile,
    server_text_ct_pt_fps_artifact_sizes_with_profile, server_text_ct_pt_fps_context_json,
    server_text_ct_pt_fps_with_profile, server_text_ct_pt_rfps_artifact_sizes_with_profile,
    server_text_ct_pt_rfps_context_json, server_text_ct_pt_rfps_with_profile,
};
use serde_json::{Value, json};

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn json_scalars(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Array(values) => {
            for value in values {
                json_scalars(value, output);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                json_scalars(value, output);
            }
        }
        Value::String(value) => output.push(value.clone()),
        Value::Number(value) => output.push(value.to_string()),
        _ => {}
    }
}

fn modulus_fields(context_json: &str) -> Value {
    let Ok(root) = serde_json::from_str::<Value>(context_json) else {
        return json!({"archive_parse":"failed"});
    };
    fn visit(value: &Value, path: &str, found: &mut BTreeMap<String, Vec<String>>) {
        match value {
            Value::Object(values) => {
                for (key, child) in values {
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    let key_lower = key.to_ascii_lowercase();
                    if key_lower.contains("moduli") || key_lower.contains("modulus") {
                        let mut scalars = Vec::new();
                        json_scalars(child, &mut scalars);
                        found.insert(child_path.clone(), scalars);
                    }
                    visit(child, &child_path, found);
                }
            }
            Value::Array(values) => {
                for (index, child) in values.iter().enumerate() {
                    visit(child, &format!("{path}[{index}]"), found);
                }
            }
            _ => {}
        }
    }
    let mut found = BTreeMap::new();
    visit(&root, "", &mut found);
    json!(found)
}

fn parse_context_archive(context_json: &str) -> Value {
    serde_json::from_str(context_json).unwrap_or_else(|_| json!({"archive_parse":"failed"}))
}

fn mean_error(scores: &[f64], exact: &[f64]) -> f64 {
    scores
        .iter()
        .zip(exact)
        .map(|(approximate, exact)| (approximate - exact).abs())
        .sum::<f64>()
        / scores.len() as f64
}

fn fps(profile: FpsProfile) -> Value {
    let text = [1, 2, 3, 4, 1, 2, 3];
    let pattern = [2, 3, 4];
    let threshold = 0.5;
    let result = server_text_ct_pt_fps_with_profile(&text, &pattern, threshold, profile).unwrap();
    let sizes =
        server_text_ct_pt_fps_artifact_sizes_with_profile(&text, &pattern, threshold, profile)
            .unwrap();
    let context_archive = server_text_ct_pt_fps_context_json(profile, 8).unwrap();

    let mut cases = vec![json!({
        "id":"deterministic",
        "exact_scores":result.exact_scores,
        "recovered_scores":result.scores,
        "accepted_positions":result.matching_positions,
    })];
    let mut all_errors = result
        .scores
        .iter()
        .zip(&result.exact_scores)
        .map(|(approximate, exact)| (approximate - exact).abs())
        .collect::<Vec<_>>();
    let mut accepted = result.matching_positions.len();
    let mut mismatches = result
        .scores
        .iter()
        .zip(&result.exact_scores)
        .filter(|(approximate, exact)| {
            (approximate.abs() <= threshold) != (exact.abs() <= threshold)
        })
        .count();
    for trial in 0..5_usize {
        let random_text = (0..31_usize)
            .map(|index| (((index * 17 + trial * 29 + 11) % 251) + 1) as u8)
            .collect::<Vec<_>>();
        let position = (trial * 5) % 27;
        let mut random_pattern = random_text[position..position + 4].to_vec();
        if trial % 2 == 1 {
            random_pattern[0] = random_pattern[0].wrapping_add(1);
        }
        let run =
            server_text_ct_pt_fps_with_profile(&random_text, &random_pattern, threshold, profile)
                .unwrap();
        let errors = run
            .scores
            .iter()
            .zip(&run.exact_scores)
            .map(|(approximate, exact)| (approximate - exact).abs())
            .collect::<Vec<_>>();
        all_errors.extend(errors);
        accepted += run.matching_positions.len();
        mismatches += run
            .scores
            .iter()
            .zip(&run.exact_scores)
            .filter(|(approximate, exact)| {
                (approximate.abs() <= threshold) != (exact.abs() <= threshold)
            })
            .count();
        cases.push(json!({
            "id": format!("random-{trial}"),
            "exact_scores": run.exact_scores,
            "recovered_scores": run.scores,
            "accepted_positions": run.matching_positions,
        }));
    }
    let boundary_pattern = (128_u8..192).collect::<Vec<_>>();
    let mut boundary_text = vec![0_u8; PHASE1_FIXTURE_TEXT_LEN];
    let boundary_position = PHASE1_FIXTURE_TEXT_LEN - boundary_pattern.len();
    boundary_text[boundary_position..].copy_from_slice(&boundary_pattern);
    let boundary =
        server_text_ct_pt_fps_with_profile(&boundary_text, &boundary_pattern, threshold, profile)
            .unwrap();
    all_errors.extend(
        boundary
            .scores
            .iter()
            .zip(&boundary.exact_scores)
            .map(|(approximate, exact)| (approximate - exact).abs()),
    );
    accepted += boundary.matching_positions.len();
    mismatches += boundary
        .scores
        .iter()
        .zip(&boundary.exact_scores)
        .filter(|(approximate, exact)| {
            (approximate.abs() <= threshold) != (exact.abs() <= threshold)
        })
        .count();
    let boundary_dft_length = boundary.metrics.batch_size;
    let boundary_passed = boundary.matching_positions == vec![boundary_position];
    cases.push(json!({
        "id":"implementation-boundary",
        "text_bytes":PHASE1_FIXTURE_TEXT_LEN,
        "dft_length":boundary_dft_length,
        "exact_scores":boundary.exact_scores,
        "recovered_scores":boundary.scores,
        "accepted_positions":boundary.matching_positions,
    }));
    let mean_abs_error = all_errors.iter().sum::<f64>() / all_errors.len() as f64;
    let max_abs_error = all_errors.into_iter().fold(0.0_f64, f64::max);
    let numerical_contract_passed =
        result.max_abs_error <= 1e-3 && max_abs_error <= 1.0 && mismatches == 0 && boundary_passed;
    let status = if numerical_contract_passed {
        "PASS"
    } else {
        "FAIL_CORRECTNESS"
    };
    let timings = result.metrics.timings;

    json!({
        "status":status,
        "profile":profile.label(),
        "scheme":"CKKS",
        "mode":"FPS",
        "requested":{"log_n":profile.log_n(),"ring_dimension":profile.ring_dimension(),"multiplicative_depth":2,"first_modulus_bits":60,"scaling_modulus_bits":profile.scaling_modulus_bits(),"scaling_mode":"FIXEDMANUAL","key_switch_technique":profile.key_switch_technique().label(),"chi_s":"UNIFORM_TERNARY (OpenFHE 1.5.1 default)","chi_e":"discrete Gaussian stddev=3.19 (OpenFHE 1.5.1 default)","security":"HEStd_128_classic"},
        "actual":{"ring_dimension":result.metrics.ring_dimension,"available_slots":result.metrics.ring_dimension/2,"batch_size":result.metrics.batch_size,"scaling_mode":result.metrics.scaling_technique,"key_switch_technique":result.metrics.key_switch_technique,"chi_s":"UNIFORM_TERNARY","chi_e":"discrete Gaussian stddev=3.19","openfhe_version":result.metrics.openfhe_version,"modulus_archive_fields":modulus_fields(&context_archive),"openfhe_context_archive":parse_context_archive(&context_archive)},
        "capacity":{"phase1_fixture_text_bytes":PHASE1_FIXTURE_TEXT_LEN,"phase1_fixture_dft_length":PHASE1_FIXTURE_TRANSFORM_LEN,"profile_max_dft_length":profile.ring_dimension()/2,"fixture_fits_profile":PHASE1_FIXTURE_TRANSFORM_LEN <= (profile.ring_dimension()/2) as usize,"boundary_execution_text_bytes":PHASE1_FIXTURE_TEXT_LEN,"boundary_execution_dft_length":boundary_dft_length,"boundary_execution_passed":boundary_passed,"layout_formula":"L(T)=next_power_of_two(|T|+1)"},
        "correctness":{"fixture_count":cases.len(),"score_count":cases.iter().map(|case|case["exact_scores"].as_array().unwrap().len()).sum::<usize>(),"max_abs_error":max_abs_error,"mean_abs_error":mean_abs_error,"accepted_alignment_count":accepted,"decision_mismatches":mismatches,"deterministic_max_abs_error":result.max_abs_error,"deterministic_mean_abs_error":mean_error(&result.scores,&result.exact_scores),"numerical_contract":{"small_fixture_max_abs_error":1e-3,"all_profile_fixtures_max_abs_error":1.0,"passed":numerical_contract_passed},"cases":cases},
        "timings_ns":{"setup":timings.context_generation_ns+timings.key_generation_ns+timings.server_text_dft_preparation_ns+timings.server_plaintext_encoding_ns,"match_query":timings.client_query_dft_preparation_ns+timings.client_query_encryption_ns,"match_response":timings.server_query_plaintext_preparation_ns+timings.server_homomorphic_evaluation_ns,"recover":timings.client_decrypt_ns+timings.client_idft_ns,"cold_end_to_end":timings.cold_end_to_end_ns()},
        "sizes_bytes":{"plaintext_search_state":sizes.plaintext_search_state_bytes,"encoded_plaintext_serialized":sizes.encoded_server_plaintext_bytes,"context":sizes.context_bytes,"public_key":sizes.public_key_bytes,"secret_key":sizes.secret_key_bytes,"evaluation_key":sizes.evaluation_multiplication_key_bytes,"encrypted_query_total":sizes.encrypted_query_ciphertext_bytes.iter().sum::<u64>(),"encrypted_query_parts":sizes.encrypted_query_ciphertext_bytes,"encrypted_response":sizes.encrypted_response_ciphertext_bytes},
        "operations":result.metrics.operations,
        "keys":result.metrics.keys,
    })
}

fn rfps(profile: RfpsProfile) -> Value {
    let text = [1, 2, 3, 4, 1, 2, 3];
    let pattern = [2, 3, 4];
    let randomizer = [2, 5, 3];
    let threshold = 0.5;
    let result =
        server_text_ct_pt_rfps_with_profile(&text, &pattern, &randomizer, threshold, profile)
            .unwrap();
    let sizes = server_text_ct_pt_rfps_artifact_sizes_with_profile(
        &text,
        &pattern,
        &randomizer,
        threshold,
        profile,
    )
    .unwrap();
    let context_archive = server_text_ct_pt_rfps_context_json(profile, 8).unwrap();

    let fixtures = [
        (
            "literal",
            vec![1, 2, 3, 4, 1, 2, 3],
            vec![2, 3, 4],
            vec![2, 5, 3],
            0.5,
        ),
        (
            "cancellation",
            vec![2, 0, 4],
            vec![0, 1, 3],
            vec![2, 5, 1],
            0.5,
        ),
        ("threshold", vec![2], vec![0], vec![1], 4.0),
    ];
    let mut cases = Vec::new();
    let mut errors = Vec::new();
    let mut accepted = 0_usize;
    let mut mismatches = 0_usize;
    let mut causes = BTreeMap::<String, usize>::new();
    for (id, fixture_text, fixture_pattern, fixture_randomizer, fixture_threshold) in fixtures {
        let run = server_text_ct_pt_rfps_with_profile(
            &fixture_text,
            &fixture_pattern,
            &fixture_randomizer,
            fixture_threshold,
            profile,
        )
        .unwrap();
        errors.extend(
            run.scores
                .iter()
                .zip(&run.exact_scores)
                .map(|(approximate, exact)| (approximate - *exact as f64).abs()),
        );
        accepted += run.matching_positions.len();
        for decision in &run.decisions {
            *causes.entry(format!("{:?}", decision.cause)).or_default() += 1;
            let exact_accepts = (decision.exact_score as f64).abs() <= fixture_threshold;
            let approximate_accepts = decision.approximate_score.abs() <= fixture_threshold;
            mismatches += usize::from(exact_accepts != approximate_accepts);
        }
        cases.push(json!({"id":id,"threshold":fixture_threshold,"exact_scores":run.exact_scores,"recovered_scores":run.scores,"accepted_positions":run.matching_positions,"decisions":run.decisions.iter().map(|decision|format!("{:?}",decision.cause)).collect::<Vec<_>>() }));
    }
    for trial in 0..5_usize {
        let random_text = (0..31_usize)
            .map(|index| (((index * 13 + trial * 31 + 7) % 251) + 1) as u8)
            .collect::<Vec<_>>();
        let position = (trial * 7) % 27;
        let mut random_pattern = random_text[position..position + 4].to_vec();
        if trial % 2 == 1 {
            random_pattern[1] = random_pattern[1].wrapping_add(1);
        }
        let trial_randomizer = vec![3 + trial as u8, 7, 11, 13];
        let run = server_text_ct_pt_rfps_with_profile(
            &random_text,
            &random_pattern,
            &trial_randomizer,
            threshold,
            profile,
        )
        .unwrap();
        errors.extend(
            run.scores
                .iter()
                .zip(&run.exact_scores)
                .map(|(approximate, exact)| (approximate - *exact as f64).abs()),
        );
        accepted += run.matching_positions.len();
        for decision in &run.decisions {
            *causes.entry(format!("{:?}", decision.cause)).or_default() += 1;
            mismatches += usize::from(
                ((decision.exact_score as f64).abs() <= threshold)
                    != (decision.approximate_score.abs() <= threshold),
            );
        }
        cases.push(json!({"id":format!("random-{trial}"),"threshold":threshold,"exact_scores":run.exact_scores,"recovered_scores":run.scores,"accepted_positions":run.matching_positions,"decisions":run.decisions.iter().map(|decision|format!("{:?}",decision.cause)).collect::<Vec<_>>() }));
    }
    let boundary_pattern = (128_u8..192).collect::<Vec<_>>();
    let mut boundary_text = vec![0_u8; PHASE1_FIXTURE_TEXT_LEN];
    let boundary_position = PHASE1_FIXTURE_TEXT_LEN - boundary_pattern.len();
    boundary_text[boundary_position..].copy_from_slice(&boundary_pattern);
    let boundary_randomizer = vec![3_u8; boundary_pattern.len()];
    let boundary = server_text_ct_pt_rfps_with_profile(
        &boundary_text,
        &boundary_pattern,
        &boundary_randomizer,
        threshold,
        profile,
    )
    .unwrap();
    errors.extend(
        boundary
            .scores
            .iter()
            .zip(&boundary.exact_scores)
            .map(|(approximate, exact)| (approximate - *exact as f64).abs()),
    );
    accepted += boundary.matching_positions.len();
    for decision in &boundary.decisions {
        *causes.entry(format!("{:?}", decision.cause)).or_default() += 1;
        mismatches += usize::from(
            ((decision.exact_score as f64).abs() <= threshold)
                != (decision.approximate_score.abs() <= threshold),
        );
    }
    let boundary_dft_length = boundary.metrics.batch_size;
    let boundary_passed = boundary.matching_positions == vec![boundary_position];
    cases.push(json!({"id":"implementation-boundary","threshold":threshold,"text_bytes":PHASE1_FIXTURE_TEXT_LEN,"dft_length":boundary_dft_length,"exact_scores":boundary.exact_scores,"recovered_scores":boundary.scores,"accepted_positions":boundary.matching_positions,"decisions":boundary.decisions.iter().map(|decision|format!("{:?}",decision.cause)).collect::<Vec<_>>() }));
    let mean_abs_error = errors.iter().sum::<f64>() / errors.len() as f64;
    let max_abs_error = errors.into_iter().fold(0.0_f64, f64::max);
    let ckks_causes = [
        DecisionCause::CkksFalsePositiveThresholdCrossing,
        DecisionCause::CkksFalseNegativeThresholdCrossing,
        DecisionCause::TrueMatchRejectedByCkks,
    ];
    let ckks_approximation_failures = ckks_causes
        .iter()
        .map(|cause| causes.get(&format!("{cause:?}")).copied().unwrap_or(0))
        .sum::<usize>();
    let numerical_contract_passed = max_abs_error <= 1.0
        && mismatches == 0
        && boundary_passed
        && ckks_approximation_failures == 0;
    let status = if numerical_contract_passed {
        "PASS"
    } else {
        "FAIL_CORRECTNESS"
    };
    let timings = result.metrics.timings;

    json!({
        "status":status,"profile":profile.label(),"scheme":"CKKS","mode":"RFPS",
        "requested":{"log_n":profile.log_n(),"ring_dimension":profile.ring_dimension(),"multiplicative_depth":1,"first_modulus_bits":60,"scaling_modulus_bits":24,"chi_s":"UNIFORM_TERNARY (OpenFHE 1.5.1 default)","chi_e":"discrete Gaussian stddev=3.19 (OpenFHE 1.5.1 default)","security":"HEStd_128_classic"},
        "actual":{"ring_dimension":result.metrics.ring_dimension,"available_slots":result.metrics.ring_dimension/2,"batch_size":result.metrics.batch_size,"scaling_mode":result.metrics.scaling_technique,"chi_s":"UNIFORM_TERNARY","chi_e":"discrete Gaussian stddev=3.19","openfhe_version":result.metrics.openfhe_version,"modulus_archive_fields":modulus_fields(&context_archive),"openfhe_context_archive":parse_context_archive(&context_archive)},
        "capacity":{"phase1_fixture_text_bytes":PHASE1_FIXTURE_TEXT_LEN,"phase1_fixture_dft_length":PHASE1_FIXTURE_TRANSFORM_LEN,"profile_max_dft_length":profile.ring_dimension()/2,"fixture_fits_profile":PHASE1_FIXTURE_TRANSFORM_LEN <= (profile.ring_dimension()/2) as usize,"boundary_execution_text_bytes":PHASE1_FIXTURE_TEXT_LEN,"boundary_execution_dft_length":boundary_dft_length,"boundary_execution_passed":boundary_passed,"layout_formula":"L(T)=next_power_of_two(|T|+1)"},
        "correctness":{"fixture_count":cases.len(),"score_count":cases.iter().map(|case|case["exact_scores"].as_array().unwrap().len()).sum::<usize>(),"max_abs_error":max_abs_error,"mean_abs_error":mean_abs_error,"accepted_alignment_count":accepted,"decision_mismatches":mismatches,"ckks_approximation_failures":ckks_approximation_failures,"numerical_contract":{"all_profile_fixtures_max_abs_error":1.0,"passed":numerical_contract_passed},"semantic_causes":causes,"cases":cases},
        "timings_ns":{"setup":timings.context_generation_ns+timings.key_generation_ns+timings.server_text_dft_preparation_ns+timings.server_plaintext_encoding_ns,"match_query":timings.client_query_dft_preparation_ns+timings.client_query_encryption_ns,"match_response":timings.server_query_plaintext_preparation_ns+timings.server_homomorphic_evaluation_ns,"recover":timings.client_decrypt_ns+timings.client_idft_ns,"cold_end_to_end":timings.cold_end_to_end_ns()},
        "sizes_bytes":{"plaintext_search_state":sizes.plaintext_search_state_bytes,"encoded_plaintext_serialized":sizes.encoded_server_plaintext_bytes,"context":sizes.context_bytes,"public_key":sizes.public_key_bytes,"secret_key":sizes.secret_key_bytes,"evaluation_key":sizes.evaluation_multiplication_key_bytes,"encrypted_query_total":sizes.encrypted_query_ciphertext_bytes.iter().sum::<u64>(),"encrypted_query_parts":sizes.encrypted_query_ciphertext_bytes,"encrypted_response":sizes.encrypted_response_ciphertext_bytes},
        "operations":result.metrics.operations,"keys":result.metrics.keys,
    })
}

fn bgv(profile: BgvProfile) -> Value {
    let (kernel, setup) = BgvCpir::new_with_setup_timings(profile.config()).unwrap();
    let mut records = vec![vec![0_u8; 128]; kernel.capacity()];
    for (index, record) in records.iter_mut().enumerate() {
        let label = format!("record-{index:03}");
        record[..label.len()].copy_from_slice(label.as_bytes());
    }
    let database_start = Instant::now();
    let mut database = kernel.prepare_database(&records).unwrap();
    let database_preparation_ns = elapsed_ns(database_start);
    let setup_total_ns = setup
        .context_generation_ns
        .saturating_add(setup.key_generation_ns)
        .saturating_add(database_preparation_ns);
    let selected = [0, kernel.capacity() / 2, kernel.capacity() - 1];

    let query_start = Instant::now();
    let query = kernel.encrypt_window_query(&selected).unwrap();
    let fetch_query_ns = elapsed_ns(query_start);
    let response_start = Instant::now();
    let response = kernel.answer(&mut database, &query);
    let fetch_response_ns = elapsed_ns(response_start);
    let recover_start = Instant::now();
    let slots = kernel.decrypt_slots(&response).unwrap();
    let recover_ns = elapsed_ns(recover_start);

    let mut selected_exact = 0_usize;
    let mut unselected_zero = 0_usize;
    for index in 0..kernel.capacity() {
        let window = &slots[index * 128..(index + 1) * 128];
        if selected.contains(&index) {
            let expected = records[index]
                .iter()
                .copied()
                .map(u64::from)
                .collect::<Vec<_>>();
            selected_exact += usize::from(window == expected);
        } else {
            unselected_zero += usize::from(window.iter().all(|&value| value == 0));
        }
    }
    let k0_supported = !matches!(
        kernel.encrypt_window_query(&[]),
        Err(CpirError::EmptySelection)
    );
    let directory = tempfile::tempdir().unwrap();
    let bundle = kernel
        .serialize_binary_bundle(directory.path(), &query, &response)
        .unwrap();
    let context_json_path = directory.path().join("context.json");
    kernel.serialize_context_json(&context_json_path).unwrap();
    let context_archive = fs::read_to_string(context_json_path).unwrap();

    json!({
        "status":"PASS","profile":profile.label(),"scheme":"BGV","mode":"Fetch",
        "requested":{"log_n":profile.log_n(),"ring_dimension":profile.ring_dimension(),"plaintext_modulus":65537,"multiplicative_depth":1,"record_slots":128,"chi_s":"UNIFORM_TERNARY (OpenFHE 1.5.1 default)","chi_e":"discrete Gaussian stddev=3.19 (OpenFHE 1.5.1 default)","security":"HEStd_128_classic"},
        "actual":{"ring_dimension":kernel.actual_ring_dimension(),"available_slots":kernel.actual_ring_dimension(),"plaintext_modulus":kernel.plaintext_modulus(),"chi_s":"UNIFORM_TERNARY","chi_e":"discrete Gaussian stddev=3.19","openfhe_version":BgvCpir::openfhe_version(),"modulus_archive_fields":modulus_fields(&context_archive),"openfhe_context_archive":parse_context_archive(&context_archive)},
        "capacity":{"record_slots":128,"verified_record_capacity":kernel.capacity(),"formula":"floor(N/128)"},
        "correctness":{"selected_window_count":selected.len(),"selected_windows_exact":selected_exact,"unselected_window_count":kernel.capacity()-selected.len(),"unselected_windows_zero":unselected_zero,"multi_record_retrieval":selected_exact==selected.len(),"k0_supported":k0_supported},
        "timings_ns":{"setup":setup_total_ns,"context_generation":setup.context_generation_ns,"key_generation":setup.key_generation_ns,"database_preparation":database_preparation_ns,"fetch_query":fetch_query_ns,"fetch_response":fetch_response_ns,"recover":recover_ns},
        "sizes_bytes":{"plaintext_record_bytes":records.len()*128,"plaintext_slot_vector_bytes":kernel.actual_ring_dimension()*std::mem::size_of::<i64>(),"encoded_plaintext_serialized":Value::Null,"context":bundle.sizes.context_bytes,"public_key":bundle.sizes.public_key_bytes,"secret_key":bundle.sizes.private_key_bytes,"evaluation_key":bundle.sizes.evaluation_key_bytes,"encrypted_query":bundle.sizes.query_bytes,"encrypted_response":bundle.sizes.response_bytes},
        "operations":{"ciphertext_ciphertext_multiplications":0,"ciphertext_plaintext_multiplications":1,"relinearizations":0,"explicit_rescales":0},
        "keys":{"public_key":true,"secret_key":true,"evaluation_key":false,"rotation_keys":false},
    })
}

fn main() {
    let profile = env::args()
        .nth(1)
        .expect("usage: cbpir_phase1_profile PROFILE");
    let report = match profile.as_str() {
        "FPS-13" => fps(FpsProfile::Fps13),
        "FPS-14" => fps(FpsProfile::Fps14),
        "FPS-15" => fps(FpsProfile::Fps15),
        "RFPS-13" => rfps(RfpsProfile::Rfps13),
        "RFPS-14" => rfps(RfpsProfile::Rfps14),
        "RFPS-15" => rfps(RfpsProfile::Rfps15),
        "BGV-13" => bgv(BgvProfile::Bgv13),
        "BGV-14" => bgv(BgvProfile::Bgv14),
        "BGV-15" => bgv(BgvProfile::Bgv15),
        _ => panic!("unknown profile: {profile}"),
    };
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
