//! Measurement-only runner. The imported Pipeline and both mechanisms are unchanged.
use cbpir::{
    LogicalRecord, MatchMode, MatchRequest, Pipeline, RecordId,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct Trial {
    id: String,
    pattern: Vec<u8>,
    weights: Vec<u8>,
}
#[derive(Deserialize)]
struct InputRecord {
    id: u64,
    searchable: Vec<u8>,
    payload: Vec<u8>,
}
#[derive(Deserialize)]
struct Input {
    schema: String,
    trial: Trial,
    records: Vec<InputRecord>,
    thresholds: Vec<f64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().len() != 1 {
        return Err("p0d_rates reads JSON lines on stdin; no arguments".into());
    }
    let mut output = io::BufWriter::new(io::stdout().lock());
    let mut seen = HashSet::new();
    let mut errors = 0;
    let mut attempts = 0;
    for line in io::stdin().lock().lines() {
        let raw: Value = serde_json::from_str(&line?)?;
        let input: Input = serde_json::from_value(raw.clone())?;
        if input.schema != "p0d-rfps-native-input-v1"
            || input.thresholds.is_empty()
            || !seen.insert(input.trial.id.clone())
        {
            return Err("wrong schema, empty thresholds, or duplicate trial ID".into());
        }
        let mut prepared = input
            .records
            .into_iter()
            .map(|r| {
                Ok(LogicalRecord {
                    id: RecordId::new(r.id)?,
                    searchable: r.searchable,
                    payload: r.payload,
                })
            })
            .collect::<Result<Vec<_>, cbpir::P0dError>>()
            .and_then(Pipeline::prepare)
            .map_err(|e| e.to_string());
        for threshold in input.thresholds {
            let request = MatchRequest {
                pattern: input.trial.pattern.clone(),
                threshold,
                mode: MatchMode::Rfps {
                    randomizer: input.trial.weights.clone(),
                },
            };
            let result = match &mut prepared {
                Ok(pipeline) => pipeline.run(&request).map_err(|e| e.to_string()),
                Err(error) => Err(error.clone()),
            };
            let preparation = prepared.as_ref().ok().map(|p| &p.preparation);
            let row = match result {
                Ok(run) => json!({"schema":"p0d-rfps-native-result-v1", "input":raw,
                    "trial_id":input.trial.id, "threshold":threshold, "preparation":preparation,
                    "run":run, "error":null}),
                Err(error) => {
                    errors += 1;
                    json!({"schema":"p0d-rfps-native-result-v1", "input":raw,
                        "trial_id":input.trial.id, "threshold":threshold, "preparation":preparation,
                        "run":null, "error":error})
                }
            };
            serde_json::to_writer(&mut output, &row)?;
            writeln!(output)?;
            output.flush()?;
            attempts += 1;
            if attempts % 30 == 0 {
                eprintln!("P0-D rate study: {attempts} attempts, {errors} errors");
            }
        }
    }
    eprintln!("P0-D rate study finished: {attempts} attempts, {errors} errors");
    if attempts == 0 || errors > 0 {
        return Err("empty input or recorded execution errors; inspect retained rows".into());
    }
    Ok(())
}
