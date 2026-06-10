mod cascade;
mod check;
mod hook;
mod install_hook;
mod skillgen;

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
    /// Install the PostToolUse hook into a Claude Code settings.json.
    InstallHook {
        #[arg(long, default_value = ".claude/settings.json")]
        settings: PathBuf,
    },
    /// Render the teaching skill (SKILL.md) for a language from the cascade.
    SkillGen {
        /// Target language, e.g. `python`.
        language: String,
        /// Write SKILL.md into this directory instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
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
        Command::InstallHook { settings } => {
            install_hook::run(&settings)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::SkillGen { language, out } => {
            skillgen::run(&language, out.as_deref())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
