fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut repetitions = 3;
    let mut backend = cbpir::MatchBackend::CtCt;
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--repetitions" if index + 1 < args.len() => {
                index += 1;
                repetitions = args[index].parse()?;
            }
            "--match-backend" if index + 1 < args.len() => {
                index += 1;
                backend = cbpir::MatchBackend::parse(&args[index])?;
            }
            _ => {
                return Err("usage: p0d_kgt1_synthetic [--repetitions N] [--match-backend ct_ct|server_text_ct_pt]".into());
            }
        }
        index += 1;
    }
    let report = cbpir::multi_report::experiment_with_match_backend(
        repetitions,
        backend,
    );
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["bounded_fixture_gate_passed"] != true {
        return Err("bounded K>1 synthetic gate failed; report retained on stdout".into());
    }
    Ok(())
}
