use cbpir_search::{ckks_fps_serialization_roundtrip, ckks_rfps_serialization_roundtrip};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};

fn bytes(value: &Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    value
        .as_array()
        .ok_or("expected byte array")?
        .iter()
        .map(|item| Ok(u8::try_from(item.as_u64().ok_or("expected byte")?)?))
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() != 4 {
        return Err("usage: p0d_bgl_ckks_sizes CATALOG_JSON CASES_JSONL NEW_OUTPUT_JSON".into());
    }
    let catalog: Value = serde_json::from_slice(&fs::read(&arguments[1])?)?;
    if catalog["schema"] != "p0d-bgl-k1-study-v1" {
        return Err("wrong catalog schema".into());
    }
    let records = catalog["records"].as_array().ok_or("missing records")?;
    let mut text = Vec::new();
    for (index, record) in records.iter().enumerate() {
        text.extend(bytes(&record["searchable"])?);
        if index + 1 < records.len() {
            text.push(0);
        }
    }
    if records.len() != 24 || text.len() != 618 {
        return Err("catalog dimensions drift".into());
    }

    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for line in BufReader::new(File::open(&arguments[2])?).lines() {
        let case: Value = serde_json::from_str(&line?)?;
        let mode = case["mode"].as_str().ok_or("missing mode")?;
        let length = case["pattern"].as_array().ok_or("missing pattern")?.len();
        let wanted = matches!(length, 5 | 8 | 16 | 32)
            && ((mode == "fps")
                || (mode == "rfps"
                    && case["weight_policy"] == "uniform_3_32"
                    && case["weight_draw"] == 0
                    && case["threshold"] == 0.5));
        if wanted && seen.insert((mode.to_owned(), length)) {
            selected.push(case);
        }
    }
    if selected.len() != 8 {
        return Err("expected one FPS and uniform RFPS probe at each of four lengths".into());
    }

    let version = openfhe::ffi::GetOpenFHEVersion()
        .to_string_lossy()
        .into_owned();
    if version != "1.5.1" {
        return Err("wrong OpenFHE runtime".into());
    }
    let mut probes = Vec::new();
    for case in selected {
        let mode = case["mode"].as_str().ok_or("missing mode")?;
        let pattern = bytes(&case["pattern"])?;
        let threshold = case["threshold"].as_f64().ok_or("missing threshold")?;
        let result = if mode == "fps" {
            ckks_fps_serialization_roundtrip(&text, &pattern, threshold)
        } else {
            ckks_rfps_serialization_roundtrip(&text, &pattern, &bytes(&case["weights"])?, threshold)
        }
        .map_err(|error| format!("P0-C serialization probe failed: {error:?}"))?;
        let sizes = result.sizes;
        probes.push(json!({
            "case_id": case["id"],
            "mode": mode,
            "pattern_length": pattern.len(),
            "threshold": threshold,
            "openfhe_version": version,
            "context_bytes": sizes.context_bytes,
            "public_key_bytes": sizes.public_key_bytes,
            "private_key_bytes": sizes.private_key_bytes,
            "evaluation_key_bytes": sizes.eval_mult_key_bytes,
            "setup_ciphertext_bytes": sizes.setup_ciphertext_bytes,
            "query_ciphertext_bytes": sizes.query_ciphertext_bytes,
            "response_ciphertext_bytes": sizes.response_ciphertext_bytes,
            "serialization_matching_positions": result.matching_positions,
        }));
    }
    probes.sort_by_key(|probe| {
        (
            probe["mode"].as_str().unwrap_or_default().to_owned(),
            probe["pattern_length"].as_u64().unwrap_or_default(),
        )
    });
    let report = json!({
        "schema": "p0d-bgl-k1-ckks-size-probe-v1",
        "execution": "separate randomized P0-C serialization roundtrips",
        "artifacts_retained": false,
        "text_bytes": text.len(),
        "probes": probes,
        "limitations": [
            "sizes come from separate serialization executions, not timed Match ciphertexts",
            "private keys are local metadata, not transmitted messages",
            "fresh ciphertext randomness can change serialized byte counts slightly"
        ]
    });
    let mut output = BufWriter::new(File::create_new(&arguments[3])?);
    serde_json::to_writer_pretty(&mut output, &report)?;
    writeln!(output)?;
    Ok(())
}
