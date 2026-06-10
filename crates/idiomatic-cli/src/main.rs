mod cascade;
mod check;
mod hook;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "idiomatic", version, about = "Idiom enforcement for agents and humans")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lint files against the resolved idiom cascade.
    Check {
        /// Apply autofix rewrites in place.
        #[arg(long)]
        fix: bool,
        /// Files to check.
        paths: Vec<PathBuf>,
    },
    /// Claude Code PostToolUse gate: autofix the touched file, instruct on the rest.
    Hook,
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { fix, paths } => {
            let outcome = check::run(&paths, fix)?;
            Ok(if outcome.had_error_severity {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Hook => hook::run(),
    }
}
