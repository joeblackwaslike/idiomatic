//! `idiomatic hook` — Claude Code PostToolUse gate.
//!
//! Reads the hook payload from stdin, lints the touched file, autofixes in place,
//! and either surfaces an "applied N fixes" systemMessage (exit 0) or feeds
//! `warn-and-instruct` diagnostics back to Claude on stderr (exit 2).
use crate::cascade::{ext_lang, load_cascade};
use anyhow::Result;
use idiomatic_core::engine::{autofix_source, lint_source, support_lang, CompiledIdiom};
use idiomatic_core::pack::FixPolicy;
use idiomatic_core::render::render_diagnostic;
use idiomatic_core::telemetry::{append_trip, TripEntry};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(serde::Deserialize, Default)]
struct HookInput {
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: ToolInput,
}

#[derive(serde::Deserialize, Default)]
struct ToolInput {
    #[serde(default)]
    file_path: Option<String>,
}

pub fn run() -> Result<ExitCode> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    let input: HookInput = serde_json::from_str(&buf).unwrap_or_default();

    // Only react to file mutations on a supported, existing source file.
    if !matches!(input.tool_name.as_str(), "Write" | "Edit" | "MultiEdit") {
        return Ok(ExitCode::SUCCESS);
    }
    let Some(file_path) = input.tool_input.file_path else {
        return Ok(ExitCode::SUCCESS);
    };
    let path = PathBuf::from(&file_path);
    let Some(lang) = ext_lang(&path) else {
        return Ok(ExitCode::SUCCESS);
    };
    if !path.is_file() {
        return Ok(ExitCode::SUCCESS);
    }

    let set = load_cascade()?;
    let compiled: Vec<CompiledIdiom> = set
        .iter()
        .filter(|i| support_lang(&i.language) == Some(lang))
        .filter_map(|i| CompiledIdiom::compile(i).ok())
        .collect();

    let source = fs::read_to_string(&path)?;

    // Every match against the original is a "trip" → telemetry.
    for hit in lint_source(&compiled, lang, &source) {
        if let Some(idiom) = set.get(&hit.id) {
            record_trip(&file_path, &idiom.id, idiom.fix_policy);
        }
    }

    let (fixed, n) = autofix_source(&compiled, lang, &source);
    if n > 0 {
        fs::write(&path, &fixed)?;
    }

    // Re-lint the fixed text; surviving warn-and-instruct idioms go back to Claude.
    let mut seen = HashSet::new();
    let mut instructions = Vec::new();
    for hit in lint_source(&compiled, lang, &fixed) {
        let Some(idiom) = set.get(&hit.id) else { continue };
        if idiom.fix_policy != FixPolicy::WarnAndInstruct {
            continue;
        }
        if seen.insert(idiom.id.clone()) {
            instructions.push(render_diagnostic(idiom));
        }
    }

    let applied = (n > 0).then(|| format!("applied {n} idiom fixes to {file_path}"));

    if !instructions.is_empty() {
        // Exit 2: stderr is fed back to Claude as actionable feedback.
        if let Some(applied) = &applied {
            eprintln!("{applied}");
        }
        eprintln!("{}", instructions.join("\n"));
        return Ok(ExitCode::from(2));
    }
    if let Some(applied) = applied {
        // Exit 0 + systemMessage: tells Claude the file changed under it.
        let out = serde_json::json!({ "systemMessage": applied });
        println!("{out}");
    }
    Ok(ExitCode::SUCCESS)
}

/// Best-effort telemetry: resolve the path from env, append, ignore failures.
fn record_trip(file: &str, idiom_id: &str, policy: FixPolicy) {
    let Some(path) = telemetry_path() else { return };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entry = TripEntry { idiom_id, file, fix_policy: policy_str(policy), ts };
    let _ = append_trip(&path, &entry); // best-effort; never breaks the hook
}

fn telemetry_path() -> Option<PathBuf> {
    if std::env::var_os("IDIOMATIC_NO_TELEMETRY").is_some() {
        return None;
    }
    if let Some(p) = std::env::var_os("IDIOMATIC_TELEMETRY") {
        return Some(PathBuf::from(p));
    }
    dirs::home_dir().map(|h| h.join(".idiomatic").join("telemetry.jsonl"))
}

fn policy_str(p: FixPolicy) -> &'static str {
    match p {
        FixPolicy::Autofix => "autofix",
        FixPolicy::WarnAndInstruct => "warn-and-instruct",
        FixPolicy::SkillOnly => "skill-only",
    }
}
