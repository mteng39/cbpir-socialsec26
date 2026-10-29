#![cfg(feature = "native")]
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn run(record_id: u64) -> std::process::Output {
    let input = json!({"schema":"p0d-rfps-native-input-v1", "trial":{"id":"test", "pattern":[1,2],"weights":[3,4]},
        "records":[{"id":record_id,"searchable":[1,2],"payload":[7,0,255,0]}],"thresholds":[0.5,1.0,4.0]});
    let mut child = Command::new(env!("CARGO_BIN_EXE_p0d_rates"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{input}").unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn rate_runner_calls_existing_pipeline_and_retains_errors() {
    let result = run(20);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let rows: Vec<Value> = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    for row in rows {
        assert_eq!(row["run"]["output"], json!({"id":20,"payload":[7,0,255,0]}));
        assert_eq!(row["run"]["fetch_count"], 1);
        assert!(row["error"].is_null());
    }
    let result = run(0);
    assert!(!result.status.success());
    let rows: Vec<Value> = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .all(|row| row["error"].is_string() && row["run"].is_null())
    );
}
