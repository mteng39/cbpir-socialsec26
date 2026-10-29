//! Frozen BGL K=1 runner. It calls the existing Pipeline without changing either HE lane.
use cbpir::{MatchBackend, Pipeline, realistic};
use serde_json::json;
use std::fs::{self, File};
use std::io::{BufWriter, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() != 4 && arguments.len() != 6 {
        return Err("usage: p0d_bgl_k1 CATALOG_JSON CASES_JSONL NEW_OUTPUT_JSONL [--match-backend ct_ct|server_text_ct_pt]".into());
    }
    let backend = if arguments.len() == 6 {
        if arguments[4] != "--match-backend" {
            return Err("expected --match-backend after output path".into());
        }
        MatchBackend::parse(
            arguments[5]
                .to_str()
                .ok_or("Match backend must be valid UTF-8")?,
        )?
    } else {
        MatchBackend::CtCt
    };
    let catalog_raw = fs::read_to_string(&arguments[1])?;
    let cases_raw = fs::read_to_string(&arguments[2])?;
    let catalog = realistic::parse_catalog(&catalog_raw)?;
    let cases = realistic::parse_cases(&cases_raw)?;
    realistic::validate_fixed_catalog(&catalog)?;
    realistic::validate_fixed_cases(&cases)?;

    let mut pipeline =
        Pipeline::prepare_with_match_backend(realistic::logical_records(catalog)?, backend)?;
    let serialization = pipeline.serialization_probe()?;
    let mut output = BufWriter::new(File::create_new(&arguments[3])?);
    serde_json::to_writer(
        &mut output,
        &json!({
            "schema": "p0d-bgl-k1-native-metadata-v1",
            "match_backend": backend,
            "attempts": cases.len(),
            "preparation": &pipeline.preparation,
            "serialization": serialization,
        }),
    )?;
    writeln!(output)?;
    output.flush()?;

    let mut errors = 0;
    for (index, case) in cases.iter().enumerate() {
        let request = realistic::request(case)?;
        let result = pipeline.run(&request).map_err(|error| error.to_string());
        let row = match result {
            Ok(run) => json!({
                "schema": realistic::RESULT_SCHEMA,
                "case": case,
                "run": run,
                "error": null,
            }),
            Err(error) => {
                errors += 1;
                json!({
                    "schema": realistic::RESULT_SCHEMA,
                    "case": case,
                    "run": null,
                    "error": error,
                })
            }
        };
        serde_json::to_writer(&mut output, &row)?;
        writeln!(output)?;
        output.flush()?;
        if (index + 1) % 25 == 0 {
            eprintln!(
                "P0-D BGL K=1: {} / {} attempts, {errors} errors",
                index + 1,
                cases.len()
            );
        }
    }
    eprintln!(
        "P0-D BGL K=1 finished: {} attempts, {errors} errors",
        cases.len()
    );
    if errors > 0 {
        return Err("one or more pipeline attempts failed; inspect retained rows".into());
    }
    Ok(())
}
