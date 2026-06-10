# `idiomatic` Agent Loop Implementation Plan (Build Order §11, Steps 4–5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Python agent loop — a live `idiomatic hook` that autofixes/instructs after each file write, an `idiomatic skill-gen` that renders the idiom set as a teaching skill, plus the diagnostic renderer and minimal JSONL telemetry that wire them together.

**Architecture:** Two new renderers live in `idiomatic-core` (`render::render_diagnostic` for agent-facing fix-its, `render::render_skill` for the SKILL.md) plus a tiny `telemetry` JSONL appender. The CLI gains three subcommands (`hook`, `install-hook`, `skill-gen`) sharing one extracted cascade-discovery module. The hook binary speaks the PostToolUse JSON protocol directly: it reads `tool_input.file_path` from stdin, reuses the existing `autofix_source`/`lint_source` engine, rewrites in place, and returns `systemMessage` context (exit 0) or feeds `warn-and-instruct` text back to Claude (exit 2).

**Tech Stack:** Rust (existing workspace), `serde_json` (new — hook payload, telemetry, settings merge), the existing `idiomatic-core` engine/resolver, `clap`, `assert_cmd` + `predicates`.

**Scope:** Spec §11 steps 4 (PostToolUse hook) and 5 (skill-gen), plus the §7 diagnostic renderer and §9 telemetry foundation they depend on. Out of scope (follow-on): PyO3 binding (6), TypeScript pack (7), Node/CI (8), the Stop/pre-completion gate (deferred), and any telemetry *reader*/ranking UI (this phase only *writes* trip records).

---

## Context

Steps 1–3 (merged to `main`) gave us a resolved `IdiomSet` and a tested engine (`lint_source`, `autofix_source`) behind an `idiomatic check` CLI. That's the machinery; this phase delivers the two halves the framework exists for:

- **Enforce in-loop (step 4):** a PostToolUse hook that fires after Write/Edit, silently autofixes what it can, and only bounces back the rare `warn-and-instruct` violation — the explicit fix for "linter makes the agent rewrite tables" pain.
- **Teach up front (step 5):** `skill-gen` renders the resolved idioms as a Claude Code skill the agent reads before writing code, so the gate ideally has nothing to do.

Decisions settled before planning (via AskUserQuestion): scope = steps 4+5 together; telemetry = minimal JSONL append (`{idiom_id, file, fix_policy, ts}`) to power later trip-count ranking (spec §9); hook delivery = an `idiomatic hook` subcommand speaking the PostToolUse protocol directly, with an `idiomatic install-hook` helper that merges the settings entry.

**Hook protocol (confirmed against current Claude Code hook docs):**
- PostToolUse stdin JSON includes `tool_name` and `tool_input.file_path`.
- Output channels: **exit 0** with a `{"systemMessage": "..."}` JSON on stdout surfaces context to Claude (used for "applied N idiom fixes to <file>" so the agent's stale in-context copy is corrected); **exit 2** feeds **stderr** back to Claude as actionable feedback (used for `warn-and-instruct`). When both apply, exit 2 and put everything on stderr.

**Current code this phase builds on (verified):**
- `idiomatic_core::engine`: `lint_source(&[CompiledIdiom], SupportLang, &str) -> Vec<Hit>` where `Hit { id, start, end }`; `autofix_source(&[CompiledIdiom], SupportLang, &str) -> (String, usize)`; `CompiledIdiom::compile(&Idiom)`; `support_lang(&str) -> Option<SupportLang>`; `pub use SupportLang`.
- `idiomatic_core::resolve`: `resolve`, `IdiomSet` (`.iter()`, `.get(id)`, `.len()`), `Idiom { id, language, title, why, severity, fix_policy, rule, fix, skill_prose, examples, provenance }`.
- `idiomatic_core::pack`: `FixPolicy { Autofix, WarnAndInstruct, SkillOnly }`, `Severity`, `Examples { bad: Option<String>, good: Option<String> }`.
- `idiomatic_core::{builtin_packs, Layer}`.
- CLI `crates/idiomatic-cli/src/check.rs` currently holds private `load_cascade()`, `load_dir()`, `ext_lang()` — Task 1 extracts these for reuse.

---

## File Structure

```
crates/idiomatic-core/src/
  render.rs        # NEW: render_diagnostic(&Idiom) + render_skill(&IdiomSet, lang)
  telemetry.rs     # NEW: TripEntry + append_trip(path, entry) JSONL appender
  lib.rs           # MODIFY: declare `pub mod render; pub mod telemetry;`
crates/idiomatic-cli/src/
  cascade.rs       # NEW: shared load_cascade() + ext_lang() (moved from check.rs)
  check.rs         # MODIFY: use cascade::*, report via render_diagnostic
  hook.rs          # NEW: `idiomatic hook` — PostToolUse protocol
  install_hook.rs  # NEW: `idiomatic install-hook` — merge settings.json
  skillgen.rs      # NEW: `idiomatic skill-gen <language> [--out <dir>]`
  main.rs          # MODIFY: declare modules + Hook/InstallHook/SkillGen subcommands
crates/idiomatic-cli/tests/
  hook_cli.rs        # NEW
  install_hook_cli.rs# NEW
  skillgen_cli.rs    # NEW
```

Responsibility boundaries: `render.rs` is pure formatting over `&Idiom`/`&IdiomSet` (no IO). `telemetry.rs` is a pure JSONL appender (no env/path policy — the CLI owns that). `cascade.rs` owns filesystem discovery + extension→language. Each CLI subcommand is its own thin module.

---

### Task 1: Add `serde_json` + extract the shared cascade module

**Files:**
- Modify: `Cargo.toml` (workspace deps), `crates/idiomatic-cli/Cargo.toml`, `crates/idiomatic-core/Cargo.toml`
- Create: `crates/idiomatic-cli/src/cascade.rs`
- Modify: `crates/idiomatic-cli/src/check.rs`, `crates/idiomatic-cli/src/main.rs`

- [ ] **Step 1: Add `serde_json` to `[workspace.dependencies]` in root `Cargo.toml`**

Add this line under `[workspace.dependencies]`:

```toml
serde_json = "1"
```

- [ ] **Step 2: Add `serde_json` to both crate manifests**

In `crates/idiomatic-core/Cargo.toml` under `[dependencies]` add:

```toml
serde_json.workspace = true
```

In `crates/idiomatic-cli/Cargo.toml` under `[dependencies]` add:

```toml
serde_json.workspace = true
```

- [ ] **Step 3: Create `crates/idiomatic-cli/src/cascade.rs`** by moving the discovery helpers out of `check.rs` verbatim:

```rust
//! Cascade discovery shared by the `check`, `hook`, and `skill-gen` commands.
use anyhow::Result;
use idiomatic_core::engine::{support_lang, SupportLang};
use idiomatic_core::pack::LoadedPack;
use idiomatic_core::resolve::{resolve, IdiomSet};
use idiomatic_core::{builtin_packs, Layer};
use std::fs;
use std::path::Path;

/// Resolve the full `base → user → project` cascade.
pub fn load_cascade() -> Result<IdiomSet> {
    let mut packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base))
        .collect::<std::result::Result<_, _>>()?;

    if let Some(config) = dirs::config_dir() {
        load_dir(&config.join("idiomatic"), Layer::User, &mut packs)?;
    }
    load_dir(Path::new(".idiomatic"), Layer::Project, &mut packs)?;

    Ok(resolve(&packs)?)
}

fn load_dir(dir: &Path, layer: Layer, out: &mut Vec<LoadedPack>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let yaml = fs::read_to_string(&path)?;
            out.push(LoadedPack::from_yaml_str(&yaml, layer)?);
        }
    }
    Ok(())
}

/// Map a file path's extension to a supported language, or None.
pub fn ext_lang(path: &Path) -> Option<SupportLang> {
    match path.extension().and_then(|e| e.to_str())? {
        "py" => support_lang("python"),
        "ts" => support_lang("typescript"),
        _ => None,
    }
}
```

- [ ] **Step 4: Trim `check.rs`** — delete the now-moved `load_cascade`, `load_dir`, and `ext_lang` functions (lines defining them), and replace the imports so it uses the shared module. The top of `check.rs` becomes:

```rust
//! `idiomatic check [--fix] <paths...>`
use crate::cascade::{ext_lang, load_cascade};
use anyhow::Result;
use idiomatic_core::engine::{autofix_source, lint_source, support_lang, CompiledIdiom, Hit};
use idiomatic_core::pack::{FixPolicy, Severity};
use idiomatic_core::resolve::IdiomSet;
use std::fs;
use std::path::{Path, PathBuf};
```

Leave `run` and `report` as they are (they call `load_cascade()`/`ext_lang()` which now resolve to the imported ones). Remove the now-unused `LoadedPack`, `Layer`, `builtin_packs`, `resolve`, `SupportLang` imports from `check.rs` (they moved to `cascade.rs`). `SupportLang` is still referenced by `ext_lang`'s return — but that's in `cascade.rs` now; `check.rs` only needs what `run`/`report` use.

- [ ] **Step 5: Declare the module in `main.rs`** — add near the top, with the existing `mod check;`:

```rust
mod cascade;
mod check;
```

- [ ] **Step 6: Verify nothing regressed**

Run: `cargo test --workspace`
Expected: PASS — all 16 existing tests still green (the two `check_cli` integration tests prove the extraction is behavior-preserving).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean (fix any unused-import warnings from the move).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/idiomatic-core/Cargo.toml crates/idiomatic-cli/
git commit -m "refactor(cli): extract shared cascade module; add serde_json"
```

---

### Task 2: Diagnostic renderer

**Files:**
- Create: `crates/idiomatic-core/src/render.rs`
- Modify: `crates/idiomatic-core/src/lib.rs`, `crates/idiomatic-cli/src/check.rs`

- [ ] **Step 1: Declare the module** — add to `crates/idiomatic-core/src/lib.rs` (with the other `pub mod` lines):

```rust
pub mod render;
```

- [ ] **Step 2: Write the failing test** in `crates/idiomatic-core/src/render.rs`:

```rust
//! Renderers: idioms → agent-facing diagnostics and teaching skills.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{Examples, FixPolicy, Severity};
    use crate::resolve::Idiom;
    use std::collections::BTreeMap;

    fn warn_idiom() -> Idiom {
        Idiom {
            id: "print-debugging".into(),
            language: "python".into(),
            title: "Use logging instead of print".into(),
            why: "print can't be leveled or routed".into(),
            severity: Severity::Info,
            fix_policy: FixPolicy::WarnAndInstruct,
            rule: None,
            fix: None,
            skill_prose: None,
            examples: Some(Examples {
                bad: Some("print(x)".into()),
                good: Some("logger.debug(x)".into()),
            }),
            provenance: BTreeMap::new(),
        }
    }

    #[test]
    fn diagnostic_includes_id_title_why_and_shape() {
        let s = render_diagnostic(&warn_idiom());
        assert!(s.contains("[print-debugging]"));
        assert!(s.contains("Use logging instead of print"));
        assert!(s.contains("print can't be leveled or routed"));
        // shape falls back to examples.good when there's no `fix`
        assert!(s.contains("logger.debug(x)"));
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p idiomatic-core render::`
Expected: FAIL — `render_diagnostic` not defined.

- [ ] **Step 4: Implement `render_diagnostic`** (above the test module in `render.rs`):

```rust
use crate::resolve::Idiom;

/// Render a violation as an agent-facing fix-it instruction. The "idiomatic
/// shape" is the idiom's `fix` if present, else its `examples.good`.
pub fn render_diagnostic(idiom: &Idiom) -> String {
    let mut s = format!("[{}] {}\n  why: {}", idiom.id, idiom.title, idiom.why);
    let shape = idiom
        .fix
        .clone()
        .or_else(|| idiom.examples.as_ref().and_then(|e| e.good.clone()));
    if let Some(shape) = shape {
        // Indent multi-line shapes under the `prefer:` label.
        let indented = shape.replace('\n', "\n          ");
        s.push_str(&format!("\n  prefer: {indented}"));
    }
    s
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p idiomatic-core render::`
Expected: PASS.

- [ ] **Step 6: Route the `check` report through the renderer** — in `crates/idiomatic-cli/src/check.rs`, change the `println!` inside `report` from the inline format to the shared renderer. Replace:

```rust
        println!("{}: [{}] {} — {}", path.display(), hit.id, idiom.title, idiom.why);
```

with:

```rust
        println!("{}: {}", path.display(), idiomatic_core::render::render_diagnostic(idiom));
```

(`hit` is still used for `hit.id` in the `set.get(&hit.id)` lookup above; only the print line changes.)

- [ ] **Step 7: Verify the CLI still passes**

Run: `cargo test -p idiomatic-cli --test check_cli`
Expected: PASS — `check_reports_warn_and_instruct_without_fixing` still finds `print-debugging` in stdout (the renderer emits `[print-debugging]`).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/idiomatic-core/src/lib.rs crates/idiomatic-core/src/render.rs crates/idiomatic-cli/src/check.rs
git commit -m "feat(core): diagnostic renderer; route check report through it"
```

---

### Task 3: Telemetry JSONL appender

**Files:**
- Create: `crates/idiomatic-core/src/telemetry.rs`
- Modify: `crates/idiomatic-core/src/lib.rs`

- [ ] **Step 1: Declare the module** — add to `lib.rs`:

```rust
pub mod telemetry;
```

- [ ] **Step 2: Write the failing test** in `crates/idiomatic-core/src/telemetry.rs`:

```rust
//! Minimal JSONL telemetry: append one line per idiom trip. Powers the §9
//! feedback loop (trip-count ranking) without any reader/analysis yet.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_writes_one_json_line_per_call() {
        let dir = std::env::temp_dir().join(format!("idiomatic-telem-{}", std::process::id()));
        let path = dir.join("telemetry.jsonl");
        let _ = std::fs::remove_file(&path);

        let e1 = TripEntry { idiom_id: "compare-none", file: "a.py", fix_policy: "autofix", ts: 100 };
        let e2 = TripEntry { idiom_id: "print-debugging", file: "a.py", fix_policy: "warn-and-instruct", ts: 101 };
        append_trip(&path, &e1).unwrap();
        append_trip(&path, &e2).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        // each line is valid JSON carrying the idiom id
        let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["idiom_id"], "compare-none");
        assert_eq!(v["fix_policy"], "autofix");
        assert!(lines[1].contains("print-debugging"));
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p idiomatic-core telemetry::`
Expected: FAIL — `TripEntry`/`append_trip` not defined.

- [ ] **Step 4: Implement `telemetry.rs`** (above the test module):

```rust
use serde::Serialize;
use std::io::Write;
use std::path::Path;

/// One recorded idiom trip. Timestamps are supplied by the caller (the CLI uses
/// wall-clock seconds) so this module stays pure and testable.
#[derive(Debug, Serialize)]
pub struct TripEntry<'a> {
    pub idiom_id: &'a str,
    pub file: &'a str,
    pub fix_policy: &'a str,
    pub ts: u64,
}

/// Append one JSON line to the telemetry file, creating parent dirs as needed.
/// Best-effort by design: callers ignore the error so telemetry never breaks the
/// hot path. The `Result` is returned so tests can assert success.
pub fn append_trip(path: &Path, entry: &TripEntry) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")?;
    Ok(())
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p idiomatic-core telemetry::`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/idiomatic-core/src/lib.rs crates/idiomatic-core/src/telemetry.rs
git commit -m "feat(core): minimal JSONL telemetry appender"
```

---

### Task 4: `idiomatic hook` (PostToolUse gate)

**Files:**
- Create: `crates/idiomatic-cli/src/hook.rs`
- Modify: `crates/idiomatic-cli/src/main.rs`
- Test: `crates/idiomatic-cli/tests/hook_cli.rs`

- [ ] **Step 1: Write the failing integration test** `crates/idiomatic-cli/tests/hook_cli.rs`:

```rust
use assert_cmd::Command;
use std::fs;

fn tmp(name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("idiomatic-hook-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    base.join(name)
}

#[test]
fn hook_autofixes_and_reports_warn() {
    let file = tmp("a.py");
    fs::write(&file, "if x == None:\n    print(x)\n").unwrap();
    let payload = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        file.display()
    );

    Command::cargo_bin("idiomatic")
        .unwrap()
        .arg("hook")
        .env("IDIOMATIC_NO_TELEMETRY", "1")
        .write_stdin(payload)
        .assert()
        .code(2) // warn-and-instruct present → feed back to Claude
        .stderr(predicates::str::contains("print-debugging"))
        .stderr(predicates::str::contains("applied 1 idiom fixes"));

    // compare-none was autofixed in place; print left for the agent
    assert_eq!(fs::read_to_string(&file).unwrap(), "if x is None:\n    print(x)\n");
}

#[test]
fn hook_pure_autofix_exits_zero_with_system_message() {
    let file = tmp("b.py");
    fs::write(&file, "if x == None:\n    pass\n").unwrap();
    let payload = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        file.display()
    );

    Command::cargo_bin("idiomatic")
        .unwrap()
        .arg("hook")
        .env("IDIOMATIC_NO_TELEMETRY", "1")
        .write_stdin(payload)
        .assert()
        .success()
        .stdout(predicates::str::contains("applied 1 idiom fixes"))
        .stdout(predicates::str::contains("systemMessage"));

    assert_eq!(fs::read_to_string(&file).unwrap(), "if x is None:\n    pass\n");
}

#[test]
fn hook_ignores_non_write_tool() {
    Command::cargo_bin("idiomatic")
        .unwrap()
        .arg("hook")
        .env("IDIOMATIC_NO_TELEMETRY", "1")
        .write_stdin(r#"{"tool_name":"Bash","tool_input":{}}"#)
        .assert()
        .success();
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p idiomatic-cli --test hook_cli`
Expected: FAIL — `hook` subcommand not implemented.

- [ ] **Step 3: Implement `crates/idiomatic-cli/src/hook.rs`:**

```rust
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

// Allow `ext_lang`/`Path` imports to be recognized as used in all build configs.
const _: fn(&Path) = |_p| {};
```

> Note: drop the trailing `const _` line if clippy doesn't complain — it's only there to avoid an unused-import edge case for `Path`. Verify and remove if unnecessary; keep the file warning-clean.

- [ ] **Step 4: Wire the subcommand into `main.rs`** — add `mod hook;` with the other module declarations, add a `Hook` variant to the `Command` enum, and dispatch it. The enum and `main` become:

```rust
#[derive(Subcommand)]
enum Command {
    /// Lint files against the resolved idiom cascade.
    Check {
        #[arg(long)]
        fix: bool,
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
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p idiomatic-cli --test hook_cli`
Expected: PASS — autofix-in-place + exit 2 with `print-debugging` on stderr; pure-autofix exit 0 with `systemMessage`; non-Write ignored.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/idiomatic-cli/src/hook.rs crates/idiomatic-cli/src/main.rs crates/idiomatic-cli/tests/hook_cli.rs
git commit -m "feat(cli): idiomatic hook (PostToolUse autofix + instruct + telemetry)"
```

---

### Task 5: `idiomatic install-hook`

**Files:**
- Create: `crates/idiomatic-cli/src/install_hook.rs`
- Modify: `crates/idiomatic-cli/src/main.rs`
- Test: `crates/idiomatic-cli/tests/install_hook_cli.rs`

- [ ] **Step 1: Write the failing integration test** `crates/idiomatic-cli/tests/install_hook_cli.rs`:

```rust
use assert_cmd::Command;
use std::fs;

#[test]
fn install_hook_merges_idempotently() {
    let dir = std::env::temp_dir().join(format!("idiomatic-install-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    // pre-existing unrelated settings must be preserved
    fs::write(&settings, r#"{"model":"opus"}"#).unwrap();

    let run = || {
        Command::cargo_bin("idiomatic")
            .unwrap()
            .args(["install-hook", "--settings", settings.to_str().unwrap()])
            .assert()
            .success();
    };
    run();
    run(); // second run must not duplicate

    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(v["model"], "opus"); // preserved
    let post = v["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(post.len(), 1); // idempotent
    assert!(post[0].to_string().contains("idiomatic hook"));
    assert!(post[0].to_string().contains("Write|Edit"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p idiomatic-cli --test install_hook_cli`
Expected: FAIL — `install-hook` subcommand not implemented.

- [ ] **Step 3: Implement `crates/idiomatic-cli/src/install_hook.rs`:**

```rust
//! `idiomatic install-hook` — merge the PostToolUse entry into a settings.json.
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn run(settings_path: &Path) -> Result<()> {
    let mut root: serde_json::Value = if settings_path.is_file() {
        serde_json::from_str(&fs::read_to_string(settings_path)?)
            .with_context(|| format!("{} is not valid JSON", settings_path.display()))?
    } else {
        serde_json::json!({})
    };

    let obj = root.as_object_mut().context("settings root is not a JSON object")?;
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("`hooks` is not a JSON object")?;
    let post = hooks
        .entry("PostToolUse")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context("`hooks.PostToolUse` is not a JSON array")?;

    // Idempotent: don't add a second `idiomatic hook` entry.
    let already = post.iter().any(|e| e.to_string().contains("idiomatic hook"));
    if !already {
        post.push(serde_json::json!({
            "matcher": "Write|Edit",
            "hooks": [ { "type": "command", "command": "idiomatic hook" } ]
        }));
    }

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(settings_path, format!("{}\n", serde_json::to_string_pretty(&root)?))?;
    println!(
        "idiomatic hook {} in {}",
        if already { "already present" } else { "installed" },
        settings_path.display()
    );
    Ok(())
}
```

- [ ] **Step 4: Wire into `main.rs`** — add `mod install_hook;`, an `InstallHook` variant, and dispatch:

```rust
    /// Install the PostToolUse hook into a Claude Code settings.json.
    InstallHook {
        #[arg(long, default_value = ".claude/settings.json")]
        settings: PathBuf,
    },
```

and in `main`'s match:

```rust
        Command::InstallHook { settings } => {
            install_hook::run(&settings)?;
            Ok(ExitCode::SUCCESS)
        }
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p idiomatic-cli --test install_hook_cli`
Expected: PASS — unrelated `model` preserved, exactly one PostToolUse entry after two runs.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/idiomatic-cli/src/install_hook.rs crates/idiomatic-cli/src/main.rs crates/idiomatic-cli/tests/install_hook_cli.rs
git commit -m "feat(cli): idiomatic install-hook (idempotent settings.json merge)"
```

---

### Task 6: Skill renderer

**Files:**
- Modify: `crates/idiomatic-core/src/render.rs`
- Test: `crates/idiomatic-core/tests/skill_render.rs`

- [ ] **Step 1: Write the failing test** `crates/idiomatic-core/tests/skill_render.rs`:

```rust
use idiomatic_core::pack::LoadedPack;
use idiomatic_core::render::render_skill;
use idiomatic_core::{builtin_packs, resolve::resolve, Layer};

#[test]
fn renders_python_skill_from_seed_pack() {
    let packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base).unwrap())
        .collect();
    let set = resolve(&packs).unwrap();

    let skill = render_skill(&set, "python");

    // frontmatter
    assert!(skill.starts_with("---\n"));
    assert!(skill.contains("name: idiomatic-python"));
    // teaches each idiom (titles from the seed pack)
    assert!(skill.contains("Use `is None`"));
    assert!(skill.contains("Flatten deep nesting")); // skill-only idiom included
    // a fenced python example block is rendered
    assert!(skill.contains("```python"));
    assert!(skill.contains("# Avoid:"));
    assert!(skill.contains("# Prefer:"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p idiomatic-core --test skill_render`
Expected: FAIL — `render_skill` not defined.

- [ ] **Step 3: Implement `render_skill`** — add to `crates/idiomatic-core/src/render.rs` (below `render_diagnostic`, add the needed imports):

```rust
use crate::engine::support_lang;
use crate::resolve::IdiomSet;

/// Render the resolved idiom set for one language as a Claude Code skill
/// (`SKILL.md` content). This is a build artifact — never hand-edited.
pub fn render_skill(set: &IdiomSet, language: &str) -> String {
    let target = support_lang(language);
    let idioms: Vec<&Idiom> = set
        .iter()
        .filter(|i| target.is_some() && support_lang(&i.language) == target)
        .collect();

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("name: idiomatic-{language}\n"));
    out.push_str(&format!(
        "description: {language} idioms enforced by idiomatic — read before writing {language} so the in-loop gate has nothing to fix.\n"
    ));
    out.push_str("---\n\n");
    out.push_str(&format!("# Idiomatic {}\n\n", capitalize(language)));
    out.push_str(&format!(
        "{} idioms. Write code that follows these the first time.\n",
        idioms.len()
    ));

    for idiom in &idioms {
        out.push_str(&format!("\n## {}\n\n", idiom.title));
        let prose = idiom.skill_prose.as_deref().unwrap_or(&idiom.why);
        out.push_str(prose.trim_end());
        out.push('\n');
        if let Some(ex) = &idiom.examples {
            if ex.bad.is_some() || ex.good.is_some() {
                out.push_str(&format!("\n```{language}\n"));
                if let Some(bad) = &ex.bad {
                    out.push_str(&format!("# Avoid:\n{}\n", bad.trim_end()));
                }
                if let Some(good) = &ex.good {
                    out.push_str(&format!("# Prefer:\n{}\n", good.trim_end()));
                }
                out.push_str("```\n");
            }
        }
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
```

Also ensure `render.rs` has `use crate::resolve::Idiom;` at the top (added in Task 2 for `render_diagnostic`). If it isn't already a top-level import, add it.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p idiomatic-core --test skill_render`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/idiomatic-core/src/render.rs crates/idiomatic-core/tests/skill_render.rs
git commit -m "feat(core): skill renderer (resolved idioms → SKILL.md)"
```

---

### Task 7: `idiomatic skill-gen`

**Files:**
- Create: `crates/idiomatic-cli/src/skillgen.rs`
- Modify: `crates/idiomatic-cli/src/main.rs`
- Test: `crates/idiomatic-cli/tests/skillgen_cli.rs`

- [ ] **Step 1: Write the failing integration test** `crates/idiomatic-cli/tests/skillgen_cli.rs`:

```rust
use assert_cmd::Command;
use std::fs;

#[test]
fn skillgen_stdout_renders_python_skill() {
    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["skill-gen", "python"])
        .assert()
        .success()
        .stdout(predicates::str::contains("name: idiomatic-python"))
        .stdout(predicates::str::contains("Use `is None`"));
}

#[test]
fn skillgen_out_writes_skill_md() {
    let dir = std::env::temp_dir().join(format!("idiomatic-skillgen-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["skill-gen", "python", "--out", dir.to_str().unwrap()])
        .assert()
        .success();

    let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
    assert!(content.contains("name: idiomatic-python"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p idiomatic-cli --test skillgen_cli`
Expected: FAIL — `skill-gen` subcommand not implemented.

- [ ] **Step 3: Implement `crates/idiomatic-cli/src/skillgen.rs`:**

```rust
//! `idiomatic skill-gen <language> [--out <dir>]` — render the teaching skill.
use crate::cascade::load_cascade;
use anyhow::Result;
use idiomatic_core::render::render_skill;
use std::fs;
use std::path::Path;

pub fn run(language: &str, out: Option<&Path>) -> Result<()> {
    let set = load_cascade()?;
    let content = render_skill(&set, language);
    match out {
        Some(dir) => {
            fs::create_dir_all(dir)?;
            let file = dir.join("SKILL.md");
            fs::write(&file, content)?;
            println!("wrote {}", file.display());
        }
        None => print!("{content}"),
    }
    Ok(())
}
```

- [ ] **Step 4: Wire into `main.rs`** — add `mod skillgen;`, a `SkillGen` variant, and dispatch:

```rust
    /// Render the teaching skill (SKILL.md) for a language from the cascade.
    SkillGen {
        /// Target language, e.g. `python`.
        language: String,
        /// Write SKILL.md into this directory instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
```

and in `main`'s match:

```rust
        Command::SkillGen { language, out } => {
            skillgen::run(&language, out.as_deref())?;
            Ok(ExitCode::SUCCESS)
        }
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p idiomatic-cli --test skillgen_cli`
Expected: PASS — stdout render + `--out` writes `SKILL.md`.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/idiomatic-cli/src/skillgen.rs crates/idiomatic-cli/src/main.rs crates/idiomatic-cli/tests/skillgen_cli.rs
git commit -m "feat(cli): idiomatic skill-gen (render teaching skill)"
```

---

### Task 8: End-to-end verification + docs

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Full suite + lint**

Run: `cargo test --workspace`
Expected: PASS — 16 prior + 9 new = 25 total. New: render_diagnostic unit (1), telemetry append unit (1), skill_render integration (1), hook_cli (3), install_hook_cli (1), skillgen_cli (2). Confirm all green, no regressions.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 2: Manual end-to-end of the live loop**

```bash
cargo build --release -q
# 1. Generate the teaching skill
./target/release/idiomatic skill-gen python | head -20
# 2. Simulate the hook firing on a freshly-written file
printf 'if x == None:\n    if not k in d:\n        print(x)\n' > /tmp/loop.py
echo '{"tool_name":"Write","tool_input":{"file_path":"/tmp/loop.py"}}' \
  | IDIOMATIC_TELEMETRY=/tmp/idiomatic-telem.jsonl ./target/release/idiomatic hook; echo "exit=$?"
echo "--- file after hook ---"; cat /tmp/loop.py
echo "--- telemetry ---"; cat /tmp/idiomatic-telem.jsonl
# 3. Install the hook into a throwaway settings file
./target/release/idiomatic install-hook --settings /tmp/settings.json && cat /tmp/settings.json
```

Expected: skill-gen prints frontmatter + idiom sections; the hook rewrites `== None`→`is None` and `not k in d`→`k not in d`, exits 2 with `print-debugging` + "applied 2 idiom fixes" on stderr, leaves `print(x)`; telemetry file has one JSON line per trip; install-hook writes a `PostToolUse` entry.

- [ ] **Step 3: Update `README.md`** — under "What works today", add the live hook and skill-gen:
  - `idiomatic skill-gen <language>` renders the teaching skill (the "teach up front" half).
  - `idiomatic hook` is the PostToolUse gate (autofix in place; instruct on the rest); install with `idiomatic install-hook`.
  - Note telemetry: trips are appended to `~/.idiomatic/telemetry.jsonl` (override `IDIOMATIC_TELEMETRY`, disable `IDIOMATIC_NO_TELEMETRY=1`) to power the §9 feedback loop; ranking/reader is follow-on.
  - Move the PostToolUse hook + skill-gen out of the "follow-on" list; leave PyO3 binding and TypeScript pack there.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: README for hook, skill-gen, and telemetry"
```

---

## Verification

The phase is proven when:

1. `cargo test --workspace` is green — adds: diagnostic renderer unit test, telemetry append unit test, skill render integration test, three `hook_cli` cases (autofix+warn→exit 2, pure autofix→exit 0 systemMessage, non-Write ignored), idempotent `install_hook_cli`, and two `skillgen_cli` cases.
2. `cargo clippy --workspace --all-targets -- -D warnings` is clean.
3. The manual end-to-end (Task 8 Step 2) shows the full loop: a generated skill, the hook autofixing + instructing + recording telemetry, and the installer writing settings.json.

This delivers spec §11 steps 4–5 plus the §7 diagnostic renderer and §9 telemetry foundation. Follow-on plans: PyO3 binding (step 6), `typescript-core` pack (step 7 — the adapter is already language-generic), Node binding + CI recipe (step 8), and a telemetry *reader* that ranks idioms by trip count to point at weak skill prose (the §9 feedback loop's analysis half).

## Self-Review Notes

- **Spec coverage:** §7 PostToolUse hook (autofix in place → systemMessage; warn-and-instruct → stderr/exit 2; skill-only never evaluated) → Task 4. §7 the same packs/resolver via shared `cascade` → Task 1. §8 skill-gen from title/why/skill_prose/examples → Tasks 6–7. §9 telemetry per trip (idiom id, file, fix_policy) → Tasks 3–4. §4 diagnostic + skill-gen renderer components → Tasks 2, 6. Deferred per scope: PyO3 (6), TS pack (7), Node/CI (8), telemetry ranking, Stop gate.
- **Telemetry honesty:** `record_trip` is explicitly best-effort (errors ignored so the hot path never breaks) — documented in code, not a silent-failure accident. The hook still functions fully with telemetry disabled.
- **Type consistency:** `render_diagnostic(&Idiom)` and `render_skill(&IdiomSet, &str)` signatures are used identically in core tests and all three CLI consumers; `TripEntry { idiom_id, file, fix_policy, ts }` and `append_trip(&Path, &TripEntry)` match between Task 3 and Task 4; `cascade::{load_cascade, ext_lang}` are referenced consistently by `check`, `hook`, and `skillgen`.
- **No new behavior in `check`:** Task 1 is a pure extraction; Task 2 only swaps its print line through the renderer (the existing `check_cli` test still passes because the renderer emits the idiom id).
