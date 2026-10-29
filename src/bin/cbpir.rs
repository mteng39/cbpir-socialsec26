fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut quick = false;
    let mut backend = cbpir::MatchBackend::CtCt;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--quick" => quick = true,
            "--match-backend" if index + 1 < args.len() => {
                index += 1;
                backend = cbpir::MatchBackend::parse(&args[index])?;
            }
            _ => {
                return Err(
                    "usage: cbpir [--quick] [--match-backend ct_ct|server_text_ct_pt]".into(),
                );
            }
        }
        index += 1;
    }
    let report = cbpir::report::experiment_with_match_backend(quick, backend);
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["bounded_fixture_gate_passed"] != true {
        return Err("bounded fixture gate failed; report retained on stdout".into());
    }
    Ok(())
}
