//! Frozen BGL K=4 runner over the existing realistic input files.

use cbpir::{MatchBackend, MultiPipeline, realistic, realistic_kgt1};
use serde_json::json;
use std::fs::{self, File};
use std::io::{BufWriter, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    if arguments.len() != 4 && arguments.len() != 6 {
        return Err("usage: p0d_bgl_kgt1 CATALOG_JSON CASES_JSONL NEW_OUTPUT_JSONL [--match-backend ct_ct|server_text_ct_pt]".into());
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
    let catalog_input = realistic::parse_catalog(&catalog_raw)?;
    let cases = realistic::parse_cases(&cases_raw)?;
    realistic::validate_fixed_catalog(&catalog_input)?;
    realistic::validate_fixed_cases(&cases)?;

    let mut pipeline = MultiPipeline::prepare_with_match_backend(
        realistic::logical_records(catalog_input)?,
        4,
        backend,
    )?;
    let mut output = BufWriter::new(File::create_new(&arguments[3])?);
    serde_json::to_writer(
        &mut output,
        &json!({
            "schema": realistic_kgt1::METADATA_SCHEMA,
            "match_backend": backend,
            "attempts": cases.len(),
            "public_k": 4,
            "preparation": &pipeline.preparation,
            "message_sizes": "joined from separately frozen P0-C and P3 serialization evidence",
        }),
    )?;
    writeln!(output)?;
    output.flush()?;

    let mut errors = 0;
    for (index, case) in cases.iter().enumerate() {
        let row = realistic_kgt1::run_case(&mut pipeline, case);
        errors += usize::from(!row["error"].is_null());
        serde_json::to_writer(&mut output, &row)?;
        writeln!(output)?;
        output.flush()?;
        if (index + 1) % 25 == 0 {
            eprintln!(
                "P0-D BGL K=4: {} / {} attempts, {errors} errors",
                index + 1,
                cases.len()
            );
        }
    }
    eprintln!(
        "P0-D BGL K=4 finished: {} attempts, {errors} errors",
        cases.len()
    );
    if errors > 0 {
        return Err("one or more K>1 pipeline attempts failed; inspect retained rows".into());
    }
    Ok(())
}
