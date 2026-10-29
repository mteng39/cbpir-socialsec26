fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let repetitions = match arguments.as_slice() {
        [] => 3,
        [flag, value] if flag == "--repetitions" => value.parse()?,
        _ => return Err("usage: p0c_server_text_ct_pt [--repetitions N]".into()),
    };
    let report = cbpir_search::server_text_ct_pt_focused_report(repetitions)
        .map_err(|error| format!("server-text ct-pt focused report failed: {error:?}"))?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["gate_passed"] != true {
        return Err("focused validation gate failed; report retained on stdout".into());
    }
    Ok(())
}
