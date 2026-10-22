use std::process::ExitCode;

mod cli;
mod commands;
mod config;
mod context;
mod convention_file;
mod error;
mod git;
mod output;
mod providers;
mod repos;
mod settings;

use clap::Parser;

use crate::cli::{Cli, Command};
use crate::context::Context;

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `init` creates a layout and `repos` lists them, so neither can require
    // one to exist first.
    if matches!(&cli.command, Command::Init(_) | Command::Repos(_)) {
        let result = match cli.command {
            Command::Init(args) => commands::init::run(args, cli.json, cli.no_color),
            Command::Repos(args) => commands::repos::run(&args, cli.json),
            _ => unreachable!("guarded above"),
        };
        return finish(result);
    }

    let ctx = match Context::load(&cli) {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("gw: {err}");
            return ExitCode::from(external_exit_code(&err));
        }
    };

    let result = match cli.command {
        Command::Status(args) => commands::status::run(&ctx, args),
        Command::Add(args) => commands::add::run(&ctx, args),
        Command::Clean(args) => commands::clean::run(&ctx, args),
        Command::Convention(args) => commands::convention::run(&ctx, args.action),
        Command::Init(_) | Command::Repos(_) => unreachable!("handled above"),
    };
    finish(result)
}

fn finish(result: Result<crate::output::Outcome, error::Error>) -> ExitCode {
    match result {
        Ok(outcome) => ExitCode::from(outcome.code()),
        Err(err) => {
            eprintln!("gw: {err}");
            ExitCode::from(external_exit_code(&err))
        }
    }
}

/// Map error kinds to the documented exit codes:
/// 0 ok, 1 operational error, 2 usage, 3 partial success.
fn external_exit_code(err: &error::Error) -> u8 {
    match err {
        error::Error::Usage(_) => 2,
        _ => 1,
    }
}
