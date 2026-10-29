use cbpir_search::{ckks_fps_serialization_roundtrip, ckks_rfps_serialization_roundtrip};
use serde_json::{Value, json};

fn bytes(v: &Value) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    v.as_array()
        .ok_or("expected byte array")?
        .iter()
        .map(|v| Ok(u8::try_from(v.as_u64().ok_or("expected byte")?)?))
        .collect()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let report_path = std::env::args().nth(1).ok_or("pass P0-D report path")?;
    let report: Value = serde_json::from_slice(&std::fs::read(report_path)?)?;
    let version = openfhe::ffi::GetOpenFHEVersion()
        .to_string_lossy()
        .into_owned();
    if version != "1.5.1" {
        return Err("wrong native runtime".into());
    }
    let mut out = Vec::new();
    for (name, protocol) in [("exact_first", "fps"), ("rfps_exact", "rfps")] {
        let case = report["cases"]
            .as_array()
            .ok_or("missing cases")?
            .iter()
            .find(|c| c["fixture"]["name"] == name)
            .ok_or("missing probe fixture")?;
        let text = bytes(&case["catalog"]["text"])?;
        let pattern = bytes(&case["fixture"]["request"]["pattern"])?;
        let q = &case["fixture"]["request"];
        let threshold = q["threshold"].as_f64().ok_or("missing threshold")?;
        let result = if protocol == "fps" {
            ckks_fps_serialization_roundtrip(&text, &pattern, threshold)
        } else {
            ckks_rfps_serialization_roundtrip(
                &text,
                &pattern,
                &bytes(&q["mode"]["Rfps"]["randomizer"])?,
                threshold,
            )
        }
        .map_err(|e| format!("probe failed: {e:?}"))?;
        let s = result.sizes;
        let positions = serde_json::to_value(&result.matching_positions)?;
        if positions != case["run"]["locate"]["accepted_positions"] {
            return Err("probe changed accepted positions on fixed fixture".into());
        }
        out.push(json!({"protocol":protocol,"fixture":name,"text":text,"pattern":pattern,"request":q,"openfhe_version":version,
            "scores":result.scores,"matching_positions":result.matching_positions,"positions_agree_with_timing_run":true,
            "context_bytes":s.context_bytes,"public_key_bytes":s.public_key_bytes,"private_key_bytes":s.private_key_bytes,
            "evaluation_key_bytes":s.eval_mult_key_bytes,"setup_ciphertext_bytes":s.setup_ciphertext_bytes,
            "query_ciphertext_bytes":s.query_ciphertext_bytes,"response_ciphertext_bytes":s.response_ciphertext_bytes}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":"p0d-ckks-size-probe-v1","execution":"unchanged source snapshot; separate randomized serialization executions","artifacts_retained":false,"probes":out})
        )?
    );
    Ok(())
}
