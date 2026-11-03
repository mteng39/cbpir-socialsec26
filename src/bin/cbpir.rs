use clap::Parser;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = cbpir::Cli::parse();
    match cli.command {
        Some(cbpir::Command::Run(args)) => {
            if cli.quick || cli.match_backend != "ct_ct" {
                return Err("legacy smoke flags cannot be combined with run".into());
            }
            run_interactive(args)
        }
        None => run_smoke(cli.quick, cbpir::MatchBackend::parse(&cli.match_backend)?),
    }
}

fn run_interactive(args: cbpir::RunArgs) -> Result<(), Box<dyn std::error::Error>> {
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
    std::io::stdout().lock().write_all(&bytes)?;
    Ok(())
}

fn run_smoke(quick: bool, backend: cbpir::MatchBackend) -> Result<(), Box<dyn std::error::Error>> {
    let report = cbpir::report::experiment_with_match_backend(quick, backend);
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report["bounded_fixture_gate_passed"] != true {
        return Err("bounded fixture gate failed; report retained on stdout".into());
    }
    Ok(())
}
