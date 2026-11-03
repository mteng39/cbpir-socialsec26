fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg == "run" || arg == "--help")
    {
        use clap::Parser;
        let cli = cbpir::Cli::parse();
        if let Some(cbpir::Command::Run(args)) = cli.command {
            if cli.quick || cli.match_backend != "ct_ct" {
                return Err("legacy smoke flags cannot be combined with run".into());
            }
            let config = cbpir::validate_run_args(&args)?;
            let workload = cbpir::build_bgl_workload(&config)?;
            let report = cbpir::run_interactive(config, workload)?;
            let mut bytes = serde_json::to_vec_pretty(&report)?;
            bytes.push(b'\n');
            if let Some(path) = args.output {
                if let Some(parent) = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, &bytes)?;
            }
            use std::io::Write;
            std::io::stdout().lock().write_all(&bytes)?;
            return Ok(());
        }
    }
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
