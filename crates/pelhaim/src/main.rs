use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use pelhaim::cli::{Cli, Cmd};
use pelhaim::env::ProcessEnv;
use pelhaim::{config, doctor, run};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let env = ProcessEnv;
    if cli.headless && matches!(cli.command, Some(Cmd::Doctor | Cmd::Decisions { .. })) {
        Cli::command()
            .error(
                clap::error::ErrorKind::ArgumentConflict,
                "--headless applies to observe only",
            )
            .exit();
    }
    let loaded = config::load(cli.config.as_deref(), &env);
    if cli.command == Some(Cmd::Doctor) {
        return match &loaded {
            Ok(l) => {
                print!("{}", doctor::doctor(l, &env));
                ExitCode::SUCCESS
            }
            Err(e) => {
                print!("{}", doctor::doctor_config_error(e, &env));
                ExitCode::FAILURE
            }
        };
    }
    let loaded = match loaded {
        Ok(l) => l,
        Err(e) => {
            eprintln!("pelhaim: config: {e}");
            return ExitCode::from(2);
        }
    };
    let result = match cli.command {
        Some(Cmd::Decisions { open }) => run::decisions(&loaded, &env, open).map(|s| print!("{s}")),
        Some(Cmd::Doctor) => unreachable!("handled above"),
        None | Some(Cmd::Observe) => run::init_tracing(&env).and_then(|()| match &cli.dump_scene {
            Some(out) => run::dump_scene(&loaded, &env, out),
            None => run::observe(&loaded, &env),
        }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pelhaim: {e:#}");
            ExitCode::FAILURE
        }
    }
}
