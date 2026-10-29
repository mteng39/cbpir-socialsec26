use std::env;
use std::process::ExitCode;

use cbpir_search::{
    CkksError, DecisionCause, FpsProfile, RfpsProfile, dft, feer_fps_scores, feer_rfps_scores,
    fps_time_layout, idft, naive_dft_reference, naive_idft_reference, rfps_time_layout,
    right_rotation_factors, server_text_ct_pt_fps_with_profile,
    server_text_ct_pt_rfps_with_profile,
};
use num_complex::Complex64;
use serde_json::{Value, json};

fn max_complex_error(left: &[Complex64], right: &[Complex64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(left, right)| (*left - *right).norm())
        .fold(0.0, f64::max)
}

fn max_real_error(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, f64::max)
}

fn naive_real_dft(values: &[f64]) -> Vec<Complex64> {
    naive_dft_reference(
        &values
            .iter()
            .map(|value| Complex64::new(*value, 0.0))
            .collect::<Vec<_>>(),
    )
}

fn naive_fps_scores(text: &[u8], pattern: &[u8]) -> Vec<f64> {
    let layout = fps_time_layout(text, pattern).expect("valid audit FPS layout");
    let text_spectrum = naive_real_dft(&layout.text);
    let prefix_spectrum = naive_real_dft(&layout.prefix_squares);
    let pattern_spectrum = naive_real_dft(&layout.reversed_pattern);
    let pattern_square_spectrum =
        naive_real_dft(&vec![layout.pattern_square_sum; layout.transform_len]);
    let rotation_factors = right_rotation_factors(layout.transform_len, pattern.len());
    let score_spectrum = (0..layout.transform_len)
        .map(|index| {
            prefix_spectrum[index] - prefix_spectrum[index] * rotation_factors[index]
                + pattern_square_spectrum[index]
                - 2.0 * text_spectrum[index] * pattern_spectrum[index]
        })
        .collect::<Vec<_>>();
    let scores = naive_idft_reference(&score_spectrum);
    layout
        .valid_coefficients
        .map(|index| scores[index].re)
        .collect()
}

fn naive_rfps_scores(text: &[u8], pattern: &[u8], randomizer: &[u8]) -> Vec<f64> {
    let layout = rfps_time_layout(text, pattern, randomizer).expect("valid audit RFPS layout");
    let text_spectrum = naive_real_dft(&layout.text);
    let randomizer_spectrum = naive_real_dft(&layout.reversed_randomizer);
    let pattern_randomizer_spectrum = naive_real_dft(&vec![
        layout.pattern_randomizer_dot_product;
        layout.transform_len
    ]);
    let score_spectrum = text_spectrum
        .iter()
        .zip(randomizer_spectrum)
        .zip(pattern_randomizer_spectrum)
        .map(|((text_value, randomizer_value), pattern_value)| {
            text_value * randomizer_value - pattern_value
        })
        .collect::<Vec<_>>();
    let scores = naive_idft_reference(&score_spectrum);
    layout
        .valid_coefficients
        .map(|index| scores[index].re)
        .collect()
}

fn deterministic_fixture(text_len: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut state = 0x5A17_3C91_u32;
    let mut text = (0..text_len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((state >> 24) | 1) as u8
        })
        .collect::<Vec<_>>();
    let pattern_len = 16.min(text_len);
    let pattern = (0..pattern_len)
        .map(|index| 128_u8.wrapping_add((index * 7) as u8))
        .collect::<Vec<_>>();
    let position = text_len.saturating_sub(pattern_len) / 2;
    text[position..position + pattern_len].copy_from_slice(&pattern);
    let randomizer = (0..pattern_len)
        .map(|index| 3 + (index % 30) as u8)
        .collect();
    (text, pattern, randomizer)
}

fn equivalence_report() -> Value {
    let rows = [64, 256, 1_024, 2_048]
        .into_iter()
        .map(|length| {
            let values = (0..length)
                .map(|index| {
                    Complex64::new(
                        ((index * 17 + 3) % 251) as f64 - 125.0,
                        ((index * 29 + 7) % 127) as f64 - 63.0,
                    )
                })
                .collect::<Vec<_>>();
            let naive_forward = naive_dft_reference(&values);
            let fft_forward = dft(&values);
            let naive_inverse = naive_idft_reference(&naive_forward);
            let fft_inverse = idft(&fft_forward);

            let (text, pattern, randomizer) = deterministic_fixture(length - 1);
            let fps_naive = naive_fps_scores(&text, &pattern);
            let fps_fft = feer_fps_scores(&text, &pattern).expect("valid FPS fixture");
            let rfps_naive = naive_rfps_scores(&text, &pattern, &randomizer);
            let rfps_fft =
                feer_rfps_scores(&text, &pattern, &randomizer).expect("valid RFPS fixture");
            let fps_decision_mismatches = fps_naive
                .iter()
                .zip(&fps_fft)
                .filter(|(naive, fft)| (naive.abs() <= 0.5) != (fft.abs() <= 0.5))
                .count();
            let rfps_decision_mismatches = rfps_naive
                .iter()
                .zip(&rfps_fft)
                .filter(|(naive, fft)| (naive.abs() <= 4.0) != (fft.abs() <= 4.0))
                .count();
            json!({
                "kind": "transform_equivalence",
                "transform_len": length,
                "max_abs_dft_error": max_complex_error(&naive_forward, &fft_forward),
                "max_abs_idft_error": max_complex_error(&naive_inverse, &fft_inverse),
                "max_abs_fps_score_difference": max_real_error(&fps_naive, &fps_fft),
                "max_abs_rfps_score_difference": max_real_error(&rfps_naive, &rfps_fft),
                "fps_decision_mismatches": fps_decision_mismatches,
                "rfps_decision_mismatches": rfps_decision_mismatches,
                "status": if fps_decision_mismatches == 0 && rfps_decision_mismatches == 0 {
                    "PASS"
                } else {
                    "FAIL"
                }
            })
        })
        .collect::<Vec<_>>();
    json!({"schema":"p0c-capacity-audit-equivalence-v1","rows":rows})
}

fn profile_for_fps(log_n: u32) -> Option<FpsProfile> {
    match log_n {
        13 => Some(FpsProfile::Fps13),
        14 => Some(FpsProfile::Fps14),
        15 => Some(FpsProfile::Fps15),
        _ => None,
    }
}

fn profile_for_rfps(log_n: u32) -> Option<RfpsProfile> {
    match log_n {
        13 => Some(RfpsProfile::Rfps13),
        14 => Some(RfpsProfile::Rfps14),
        15 => Some(RfpsProfile::Rfps15),
        _ => None,
    }
}

fn case_text_len(case: &str, ring_dimension: usize) -> Option<usize> {
    match case {
        "small" => Some(31),
        "phase1-fixture" => Some(1_024),
        "full" => Some(ring_dimension / 2 - 1),
        "over" => Some(ring_dimension / 2),
        _ => None,
    }
}

fn error_text(error: &CkksError) -> String {
    format!("{error:?}")
}

fn fps_report(log_n: u32, case: &str) -> Result<Value, String> {
    let profile =
        profile_for_fps(log_n).ok_or_else(|| "FPS logN must be 13, 14, or 15".to_owned())?;
    let ring_dimension = profile.ring_dimension() as usize;
    let text_len = case_text_len(case, ring_dimension).ok_or_else(|| "unknown case".to_owned())?;
    let (text, pattern, _) = deterministic_fixture(text_len);
    let transform_len = fps_time_layout(&text, &pattern)
        .map_err(|error| format!("layout failed: {error:?}"))?
        .transform_len;

    if case == "over" {
        let error = profile.config_for_transform_len(transform_len).unwrap_err();
        let expected = CkksError::TransformExceedsProfileCapacity {
            transform_len: ring_dimension,
            available_slots: ring_dimension / 2,
        };
        return Ok(json!({
            "kind":"capacity","mode":"FPS","profile":profile.label(),"log_n":log_n,
            "ring_dimension":ring_dimension,"available_slots":ring_dimension/2,"case":case,
            "text_len":text_len,"pattern_len":pattern.len(),"transform_len":transform_len,
            "context_generation_success":false,"encryption_success":false,"evaluation_success":false,
            "decryption_success":false,"max_abs_error":Value::Null,"mean_abs_error":Value::Null,
            "exact_accept_count":Value::Null,"recovered_accept_count":Value::Null,
            "decision_mismatches":Value::Null,"clean_rejection":error == expected,
            "error":error_text(&error),"status":if error == expected {"PASS"} else {"FAIL"}
        }));
    }

    let result = server_text_ct_pt_fps_with_profile(&text, &pattern, 0.5, profile)
        .map_err(|error| format!("FPS execution failed: {error:?}"))?;
    let errors = result
        .scores
        .iter()
        .zip(&result.exact_scores)
        .map(|(approximate, exact)| (approximate - exact).abs())
        .collect::<Vec<_>>();
    let mismatches = result
        .scores
        .iter()
        .zip(&result.exact_scores)
        .filter(|(approximate, exact)| (approximate.abs() <= 0.5) != (exact.abs() <= 0.5))
        .count();
    let exact_accept_count = result
        .exact_scores
        .iter()
        .filter(|score| score.abs() <= 0.5)
        .count();
    let recovered_accept_count = result
        .scores
        .iter()
        .filter(|score| score.abs() <= 0.5)
        .count();
    Ok(json!({
        "kind":"capacity","mode":"FPS","profile":profile.label(),"log_n":log_n,
        "ring_dimension":result.metrics.ring_dimension,"available_slots":ring_dimension/2,"case":case,
        "text_len":text_len,"pattern_len":pattern.len(),"transform_len":transform_len,
        "context_generation_success":result.metrics.timings.context_generation_ns > 0,
        "encryption_success":result.metrics.timings.client_query_encryption_ns > 0,
        "evaluation_success":result.metrics.timings.server_homomorphic_evaluation_ns > 0,
        "decryption_success":result.metrics.timings.client_decrypt_ns > 0,
        "max_abs_error":result.max_abs_error,
        "mean_abs_error":errors.iter().sum::<f64>() / errors.len() as f64,
        "exact_accept_count":exact_accept_count,"recovered_accept_count":recovered_accept_count,
        "decision_mismatches":mismatches,"clean_rejection":Value::Null,"error":Value::Null,
        "security_level":"HEStd_128_classic","first_modulus_bits":result.metrics.first_modulus_bits,
        "scaling_modulus_bits":result.metrics.scaling_modulus_bits,
        "multiplicative_depth":result.metrics.configured_multiplicative_depth,
        "scaling_technique":result.metrics.scaling_technique,
        "key_switch_technique":result.metrics.key_switch_technique,
        "status":if mismatches == 0 {"PASS"} else {"FAIL"}
    }))
}

fn rfps_report(log_n: u32, case: &str) -> Result<Value, String> {
    let profile =
        profile_for_rfps(log_n).ok_or_else(|| "RFPS logN must be 13, 14, or 15".to_owned())?;
    let ring_dimension = profile.ring_dimension() as usize;
    let text_len = case_text_len(case, ring_dimension).ok_or_else(|| "unknown case".to_owned())?;
    let (text, pattern, randomizer) = deterministic_fixture(text_len);
    let transform_len = rfps_time_layout(&text, &pattern, &randomizer)
        .map_err(|error| format!("layout failed: {error:?}"))?
        .transform_len;

    if case == "over" {
        let error = profile.config_for_transform_len(transform_len).unwrap_err();
        let expected = CkksError::TransformExceedsProfileCapacity {
            transform_len: ring_dimension,
            available_slots: ring_dimension / 2,
        };
        return Ok(json!({
            "kind":"capacity","mode":"RFPS","profile":profile.label(),"log_n":log_n,
            "ring_dimension":ring_dimension,"available_slots":ring_dimension/2,"case":case,
            "text_len":text_len,"pattern_len":pattern.len(),"transform_len":transform_len,
            "context_generation_success":false,"encryption_success":false,"evaluation_success":false,
            "decryption_success":false,"max_abs_error":Value::Null,"mean_abs_error":Value::Null,
            "exact_accept_count":Value::Null,"recovered_accept_count":Value::Null,
            "decision_mismatches":Value::Null,"clean_rejection":error == expected,
            "error":error_text(&error),"status":if error == expected {"PASS"} else {"FAIL"}
        }));
    }

    let result = server_text_ct_pt_rfps_with_profile(&text, &pattern, &randomizer, 4.0, profile)
        .map_err(|error| format!("RFPS execution failed: {error:?}"))?;
    let errors = result
        .scores
        .iter()
        .zip(&result.exact_scores)
        .map(|(approximate, exact)| (approximate - *exact as f64).abs())
        .collect::<Vec<_>>();
    let mismatches = result
        .scores
        .iter()
        .zip(&result.exact_scores)
        .filter(|(approximate, exact)| {
            (approximate.abs() <= 4.0) != ((**exact as f64).abs() <= 4.0)
        })
        .count();
    let exact_accept_count = result
        .exact_scores
        .iter()
        .filter(|score| score.abs() <= 4)
        .count();
    let recovered_accept_count = result
        .scores
        .iter()
        .filter(|score| score.abs() <= 4.0)
        .count();
    let cancellation_count = result
        .decisions
        .iter()
        .filter(|decision| decision.cause == DecisionCause::IntrinsicCancellation)
        .count();
    let threshold_only_count = result
        .decisions
        .iter()
        .filter(|decision| decision.cause == DecisionCause::ThresholdOnly)
        .count();
    Ok(json!({
        "kind":"capacity","mode":"RFPS","profile":profile.label(),"log_n":log_n,
        "ring_dimension":result.metrics.ring_dimension,"available_slots":ring_dimension/2,"case":case,
        "text_len":text_len,"pattern_len":pattern.len(),"transform_len":transform_len,
        "context_generation_success":result.metrics.timings.context_generation_ns > 0,
        "encryption_success":result.metrics.timings.client_query_encryption_ns > 0,
        "evaluation_success":result.metrics.timings.server_homomorphic_evaluation_ns > 0,
        "decryption_success":result.metrics.timings.client_decrypt_ns > 0,
        "max_abs_error":result.max_abs_error,
        "mean_abs_error":errors.iter().sum::<f64>() / errors.len() as f64,
        "exact_accept_count":exact_accept_count,"recovered_accept_count":recovered_accept_count,
        "decision_mismatches":mismatches,"rfps_intrinsic_cancellation_count":cancellation_count,
        "rfps_threshold_only_count":threshold_only_count,"clean_rejection":Value::Null,"error":Value::Null,
        "security_level":"HEStd_128_classic","first_modulus_bits":result.metrics.first_modulus_bits,
        "scaling_modulus_bits":result.metrics.scaling_modulus_bits,
        "multiplicative_depth":result.metrics.configured_multiplicative_depth,
        "scaling_technique":result.metrics.scaling_technique,
        "key_switch_technique":result.metrics.key_switch_technique,
        "status":if mismatches == 0 {"PASS"} else {"FAIL"}
    }))
}

fn main() -> ExitCode {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let result = match args.as_slice() {
        [mode] if mode == "equivalence" => Ok(equivalence_report()),
        [mode, log_n, case] if mode == "fps" => log_n
            .parse::<u32>()
            .map_err(|_| "invalid logN".to_owned())
            .and_then(|log_n| fps_report(log_n, case)),
        [mode, log_n, case] if mode == "rfps" => log_n
            .parse::<u32>()
            .map_err(|_| "invalid logN".to_owned())
            .and_then(|log_n| rfps_report(log_n, case)),
        _ => Err("usage: p0c_capacity_audit equivalence | \
             <fps|rfps> <13|14|15> <small|phase1-fixture|full|over>"
            .to_owned()),
    };
    match result {
        Ok(report) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("serialize report")
            );
            if report["status"] == "FAIL"
                || report["rows"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|row| row["status"] == "FAIL"))
            {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
