# `idiomatic` Core Implementation Plan (Build Order §11, Steps 1–3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Rust core of `idiomatic` — multi-doc YAML pack loader + validator + cascade resolver + ast-grep engine adapter + self-test harness — delivering a working `idiomatic check` CLI that lints and autofixes Python files against a seed `python-core` pack.

**Architecture:** A Cargo workspace with `idiomatic-core` (library: pack model → cascade resolver → ast-grep adapter → self-test harness) and `idiomatic-cli` (the `idiomatic` binary). Idioms are authored as multi-document YAML; the core resolves a `base → user → project` cascade with field-level merge and internal provenance, then bridges each resolved idiom into ast-grep by **synthesizing an ast-grep rule YAML and feeding it to `from_yaml_string`** — insulating us from ast-grep's internal struct shapes. `fix_policy` is the spine: `autofix` rewrites in place, `warn-and-instruct` reports, `skill-only` carries no detector.

**Tech Stack:** Rust (edition 2021, stable), `ast-grep-config`/`ast-grep-core`/`ast-grep-language` 0.43, `serde` + `serde_yaml_ng`, `thiserror` (lib errors), `clap` + `anyhow` (cli), `insta` (golden cascade tests), `assert_cmd` (cli integration tests).

**Scope:** Steps 1–3 of spec §11 only. Steps 4 (PostToolUse hook), 5 (skill-gen), 6 (PyO3 binding), 7 (TypeScript pack), 8 (Node/CI) are follow-on plans. The PyO3 crate is intentionally deferred but the workspace split below is the boundary it will plug into.

---

## Context

`idiomatic` makes AI agents write idiomatic code the first time (a generated skill) and silently repairs the cases they don't (a sub-100ms in-loop gate). Spec: [docs/superpowers/specs/2026-06-09-idiomatic-framework-design.md](../specs/2026-06-09-idiomatic-framework-design.md). This plan builds the foundation every other piece sits on: the pack format, the cascade, and the engine adapter. Until the loader/resolver/adapter exist and are proven against a real seed pack with self-tests, none of the downstream integrations (hook, skill-gen, bindings) have anything to render or enforce.

**Confirmed before planning:** `ast-grep-config` 0.43.0 (MIT) publicly exposes `from_yaml_string`, `RuleConfig<L>` (derefs to `SerializableRuleConfig`, `.matcher`, `.get_fixer()`), and the `Fixer` type; `ast-grep-language` exposes `SupportLang` with `lang.ast_grep(src).root()`. The Rust-only match+fix path the spec's hot path depends on is real.

**Spec deltas (deliberate, surfaced for review):**
- The spec's flagship `no-range-len` autofix is not a clean single-pattern rewrite (element name + `seq[i]` body uses are uninferable from one pattern). The seed pack instead seeds `autofix` with genuine single-node rewrites and routes `print(...)` to `warn-and-instruct`. `no-range-len` is left for a later pack pass using ast-grep `transform`/`rewriters`, or reclassified `warn-and-instruct`.
- The spec's "list-valued fields append by default / `replace: true`" rule has **no list-valued field in the v1 schema** (`rule.pattern`, `examples.bad/good`, `fix` are all scalar). The resolver implements scalar field-override + `disabled` now; list-append is left as a documented extension point with a placeholder test, to be implemented when a list field is first introduced (YAGNI).

**Environment blocker (fix before running any task):** The `lessons-learned` PreToolUse hook (`/Users/joe/github/joeblackwaslike/lessons-learned/hooks/pretooluse-lesson-inject.mjs`) is currently erroring on every Bash call. The implementation session must repair or temporarily disable that hook before `cargo` commands will run.

---

## File Structure

```
Cargo.toml                                  # workspace manifest
rust-toolchain.toml                         # pin stable
crates/
  idiomatic-core/
    Cargo.toml
    packs/
      python-core.yaml                      # bundled seed pack (4 idioms, all policies)
    src/
      lib.rs                                 # re-exports, Layer enum
      pack.rs                                # PackManifest, IdiomPatch, Examples, FixPolicy, Severity, multi-doc loader
      resolve.rs                             # cascade merge by id, provenance, disabled → IdiomSet
      validate.rs                            # load-time invariants on resolved idioms
      engine.rs                              # ast-grep adapter: compile + lint_source + autofix_source
      selftest.rs                            # run examples.bad/good as fixtures
      error.rs                               # thiserror error enums
    tests/
      cascade_golden.rs                      # insta golden resolution chains
      selftest_seed.rs                       # every seed idiom's examples pass
  idiomatic-cli/
    Cargo.toml
    src/
      main.rs                                # clap entry
      check.rs                               # `check` command: discover layers, resolve, lint paths, --fix, exit codes
    tests/
      check_cli.rs                           # assert_cmd integration tests
```

**Responsibility boundaries:** `pack.rs` knows YAML and the raw record shape only. `resolve.rs` knows merge/provenance only (no YAML, no ast-grep). `engine.rs` is the only module that touches ast-grep. `selftest.rs` depends on `engine.rs`. The CLI orchestrates; it owns filesystem discovery and exit codes.

---

### Task 0: Workspace scaffolding

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `crates/idiomatic-core/Cargo.toml`, `crates/idiomatic-core/src/lib.rs`, `crates/idiomatic-cli/Cargo.toml`, `crates/idiomatic-cli/src/main.rs`

- [ ] **Step 1: Repair the Bash hook**, then create the workspace manifest

`Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/idiomatic-core", "crates/idiomatic-cli"]

[workspace.package]
edition = "2021"
license = "MIT"
version = "0.1.0"

[workspace.dependencies]
ast-grep-config = "0.43"
ast-grep-core = "0.43"
ast-grep-language = "0.43"
serde = { version = "1", features = ["derive"] }
serde_yaml_ng = "0.10"
thiserror = "2"
anyhow = "1"
clap = { version = "4", features = ["derive"] }
dirs = "5"
insta = { version = "1", features = ["yaml"] }
assert_cmd = "2"
```

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
```

- [ ] **Step 2: Create `crates/idiomatic-core/Cargo.toml`**

```toml
[package]
name = "idiomatic-core"
edition.workspace = true
license.workspace = true
version.workspace = true

[dependencies]
ast-grep-config.workspace = true
ast-grep-core.workspace = true
ast-grep-language.workspace = true
serde.workspace = true
serde_yaml_ng.workspace = true
thiserror.workspace = true

[dev-dependencies]
insta.workspace = true
```

- [ ] **Step 3: Create `crates/idiomatic-core/src/lib.rs`** (modules declared as we add them; start minimal)

```rust
//! Core of the `idiomatic` idiom-enforcement framework.

pub mod error;
pub mod pack;
pub mod resolve;
pub mod validate;
pub mod engine;
pub mod selftest;

/// A configuration layer, lowest to highest precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Base,
    User,
    Project,
}
```

Create empty `src/error.rs`, `src/pack.rs`, `src/resolve.rs`, `src/validate.rs`, `src/engine.rs`, `src/selftest.rs` with a single `//! placeholder` line each so the crate compiles.

- [ ] **Step 4: Create `crates/idiomatic-cli/Cargo.toml`**

```toml
[package]
name = "idiomatic-cli"
edition.workspace = true
license.workspace = true
version.workspace = true

[[bin]]
name = "idiomatic"
path = "src/main.rs"

[dependencies]
idiomatic-core = { path = "../idiomatic-core" }
clap.workspace = true
anyhow.workspace = true
dirs.workspace = true

[dev-dependencies]
assert_cmd.workspace = true
```

- [ ] **Step 5: Create `crates/idiomatic-cli/src/main.rs`** (stub)

```rust
fn main() {
    println!("idiomatic: not yet implemented");
}
```

- [ ] **Step 6: Verify the workspace builds**

Run: `cargo build`
Expected: PASS — both crates compile.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml rust-toolchain.toml crates/
git commit -m "chore: scaffold idiomatic cargo workspace"
```

---

### Task 1: Error types + pack model + multi-doc loader

**Files:**
- Modify: `crates/idiomatic-core/src/error.rs`, `crates/idiomatic-core/src/pack.rs`
- Test: inline `#[cfg(test)]` in `pack.rs`

- [ ] **Step 1: Write `error.rs`**

```rust
//! Error types for the core.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackError {
    #[error("failed to parse YAML document {index}: {source}")]
    Yaml { index: usize, source: serde_yaml_ng::Error },
    #[error("pack is empty: expected a manifest document followed by idiom documents")]
    Empty,
    #[error("duplicate idiom id '{0}' within a single pack/layer")]
    DuplicateId(String),
}

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("idiom '{id}' is missing required field '{field}' after resolution")]
    MissingField { id: String, field: &'static str },
    #[error("unknown language '{lang}' for idiom '{id}'")]
    UnknownLanguage { id: String, lang: String },
    #[error("idiom '{id}': fix_policy=autofix requires a `fix`")]
    AutofixWithoutFix { id: String },
    #[error("idiom '{id}': fix_policy=skill-only must not declare a `rule`")]
    SkillOnlyWithRule { id: String },
    #[error("idiom '{id}': a `rule` is present but fix_policy=skill-only")]
    RuleWithSkillOnly { id: String },
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("idiom '{id}': failed to build ast-grep rule: {message}")]
    Compile { id: String, message: String },
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
}
```

- [ ] **Step 2: Write the failing test for multi-doc loading** in `pack.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
name: python-core
language: python
version: 0.1.0
---
id: compare-none
language: python
title: "Use `is None`"
why: "identity, not equality, for None"
severity: warn
fix_policy: autofix
rule:
  pattern: "$X == None"
fix: "$X is None"
examples:
  bad: "if x == None:\n    pass"
  good: "if x is None:\n    pass"
"#;

    #[test]
    fn loads_manifest_and_one_idiom() {
        let pack = LoadedPack::from_yaml_str(SAMPLE, crate::Layer::Base).unwrap();
        assert_eq!(pack.manifest.name, "python-core");
        assert_eq!(pack.idioms.len(), 1);
        let idiom = &pack.idioms[0];
        assert_eq!(idiom.id, "compare-none");
        assert_eq!(idiom.fix_policy, Some(FixPolicy::Autofix));
        assert_eq!(idiom.fix.as_deref(), Some("$X is None"));
    }

    #[test]
    fn rejects_duplicate_ids() {
        let dup = format!("{SAMPLE}---\nid: compare-none\nfix_policy: warn-and-instruct\n");
        let err = LoadedPack::from_yaml_str(&dup, crate::Layer::Base).unwrap_err();
        assert!(matches!(err, PackError::DuplicateId(_)));
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p idiomatic-core pack::`
Expected: FAIL — `LoadedPack`, `FixPolicy`, etc. not defined.

- [ ] **Step 4: Implement `pack.rs`**

```rust
//! Pack model and multi-document YAML loader.
use crate::error::PackError;
use crate::Layer;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FixPolicy {
    Autofix,
    WarnAndInstruct,
    SkillOnly,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Examples {
    #[serde(default)]
    pub bad: Option<String>,
    #[serde(default)]
    pub good: Option<String>,
}

/// The first document in a pack file.
#[derive(Debug, Clone, Deserialize)]
pub struct PackManifest {
    pub name: String,
    pub language: String,
    pub version: String,
}

/// One idiom document as authored. Every field except `id` is optional so that a
/// higher cascade layer can override a single field. `rule` is kept as an opaque
/// YAML value and forwarded to ast-grep verbatim.
#[derive(Debug, Clone, Deserialize)]
pub struct IdiomPatch {
    pub id: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub severity: Option<Severity>,
    #[serde(default)]
    pub fix_policy: Option<FixPolicy>,
    #[serde(default)]
    pub rule: Option<serde_yaml_ng::Value>,
    #[serde(default)]
    pub fix: Option<String>,
    #[serde(default)]
    pub skill_prose: Option<String>,
    #[serde(default)]
    pub examples: Option<Examples>,
    #[serde(default)]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct LoadedPack {
    pub manifest: PackManifest,
    pub idioms: Vec<IdiomPatch>,
    pub layer: Layer,
}

impl LoadedPack {
    /// Parse a multi-document pack: first document is the manifest, the rest are
    /// idiom patches. Enforces id-uniqueness within this pack.
    pub fn from_yaml_str(input: &str, layer: Layer) -> Result<Self, PackError> {
        let mut docs = serde_yaml_ng::Deserializer::from_str(input).enumerate();

        let (_, first) = docs.next().ok_or(PackError::Empty)?;
        let manifest = PackManifest::deserialize(first)
            .map_err(|source| PackError::Yaml { index: 0, source })?;

        let mut idioms = Vec::new();
        let mut seen = HashSet::new();
        for (index, doc) in docs {
            let patch = IdiomPatch::deserialize(doc)
                .map_err(|source| PackError::Yaml { index, source })?;
            if !seen.insert(patch.id.clone()) {
                return Err(PackError::DuplicateId(patch.id));
            }
            idioms.push(patch);
        }

        Ok(LoadedPack { manifest, idioms, layer })
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p idiomatic-core pack::`
Expected: PASS — both tests green.

- [ ] **Step 6: Commit**

```bash
git add crates/idiomatic-core/src/error.rs crates/idiomatic-core/src/pack.rs
git commit -m "feat(core): pack model and multi-document yaml loader"
```

---

### Task 2: Cascade resolver — field-level merge, provenance, disabled

**Files:**
- Modify: `crates/idiomatic-core/src/resolve.rs`
- Test: inline `#[cfg(test)]` in `resolve.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{FixPolicy, IdiomPatch, LoadedPack, Severity};
    use crate::Layer;

    fn base_pack() -> LoadedPack {
        LoadedPack::from_yaml_str(
            r#"
name: python-core
language: python
version: 0.1.0
---
id: compare-none
language: python
title: "Use is None"
why: "identity not equality"
severity: error
fix_policy: autofix
rule:
  pattern: "$X == None"
fix: "$X is None"
"#,
            Layer::Base,
        )
        .unwrap()
    }

    fn project_override() -> LoadedPack {
        // Only names `id` + `severity`: must override severity, keep pattern/fix.
        LoadedPack::from_yaml_str(
            "name: proj\nlanguage: python\nversion: 0.0.0\n---\nid: compare-none\nseverity: warn\n",
            Layer::Project,
        )
        .unwrap()
    }

    #[test]
    fn higher_layer_overrides_named_field_only() {
        let set = resolve(&[base_pack(), project_override()]).unwrap();
        let idiom = set.get("compare-none").unwrap();
        assert_eq!(idiom.severity, Severity::Warn); // overridden
        assert_eq!(idiom.fix.as_deref(), Some("$X is None")); // inherited
        assert_eq!(idiom.provenance["severity"], Layer::Project);
        assert_eq!(idiom.provenance["fix"], Layer::Base);
    }

    #[test]
    fn disabled_removes_idiom() {
        let off = LoadedPack::from_yaml_str(
            "name: p\nlanguage: python\nversion: 0\n---\nid: compare-none\ndisabled: true\n",
            Layer::Project,
        )
        .unwrap();
        let set = resolve(&[base_pack(), off]).unwrap();
        assert!(set.get("compare-none").is_none());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p idiomatic-core resolve::`
Expected: FAIL — `resolve`, `IdiomSet` not defined.

- [ ] **Step 3: Implement `resolve.rs`**

```rust
//! Cascade resolution: fold idiom patches across layers by id, field-by-field,
//! tracking which layer set each resolved field (provenance).
use crate::error::ResolveError;
use crate::pack::{Examples, FixPolicy, IdiomPatch, LoadedPack, Severity};
use crate::Layer;
use std::collections::BTreeMap;

/// Which layer last set a given field. Keyed by field name.
pub type Provenance = BTreeMap<String, Layer>;

/// A fully resolved idiom, ready for the engine and renderers.
#[derive(Debug, Clone)]
pub struct Idiom {
    pub id: String,
    pub language: String,
    pub title: String,
    pub why: String,
    pub severity: Severity,
    pub fix_policy: FixPolicy,
    pub rule: Option<serde_yaml_ng::Value>,
    pub fix: Option<String>,
    pub skill_prose: Option<String>,
    pub examples: Option<Examples>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Default)]
pub struct IdiomSet {
    idioms: Vec<Idiom>,
}

impl IdiomSet {
    pub fn get(&self, id: &str) -> Option<&Idiom> {
        self.idioms.iter().find(|i| i.id == id)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Idiom> {
        self.idioms.iter()
    }
    pub fn len(&self) -> usize {
        self.idioms.len()
    }
    pub fn is_empty(&self) -> bool {
        self.idioms.is_empty()
    }
}

/// Accumulator: optional fields + provenance, folded patch by patch.
#[derive(Default)]
struct Acc {
    language: Option<String>,
    title: Option<String>,
    why: Option<String>,
    severity: Option<Severity>,
    fix_policy: Option<FixPolicy>,
    rule: Option<serde_yaml_ng::Value>,
    fix: Option<String>,
    skill_prose: Option<String>,
    examples: Option<Examples>,
    disabled: bool,
    provenance: Provenance,
}

impl Acc {
    fn apply(&mut self, patch: &IdiomPatch, layer: Layer) {
        macro_rules! set {
            ($field:ident, $name:literal) => {
                if let Some(v) = patch.$field.clone() {
                    self.$field = Some(v);
                    self.provenance.insert($name.to_string(), layer);
                }
            };
        }
        set!(language, "language");
        set!(title, "title");
        set!(why, "why");
        set!(severity, "severity");
        set!(fix_policy, "fix_policy");
        set!(rule, "rule");
        set!(fix, "fix");
        set!(skill_prose, "skill_prose");
        set!(examples, "examples");
        if let Some(d) = patch.disabled {
            self.disabled = d;
            self.provenance.insert("disabled".to_string(), layer);
        }
        // NOTE: list-valued fields would append-by-default here (with `replace:
        // true` to discard the inherited list). No list field exists in the v1
        // schema; see plan "spec deltas". Extension point — do not delete.
    }

    fn finalize(self, id: String) -> Result<Idiom, ResolveError> {
        let field = |name| ResolveError::MissingField { id: id.clone(), field: name };
        Ok(Idiom {
            language: self.language.ok_or_else(|| field("language"))?,
            title: self.title.ok_or_else(|| field("title"))?,
            why: self.why.ok_or_else(|| field("why"))?,
            severity: self.severity.unwrap_or(Severity::Error),
            fix_policy: self.fix_policy.ok_or_else(|| field("fix_policy"))?,
            rule: self.rule,
            fix: self.fix,
            skill_prose: self.skill_prose,
            examples: self.examples,
            provenance: self.provenance,
            id,
        })
    }
}

/// Resolve packs (already ordered lowest→highest precedence) into a flat set.
/// Disabled idioms are dropped. Validation invariants are applied here.
pub fn resolve(packs: &[LoadedPack]) -> Result<IdiomSet, ResolveError> {
    // Preserve first-seen id order for deterministic output / golden tests.
    let mut order: Vec<String> = Vec::new();
    let mut accs: BTreeMap<String, Acc> = BTreeMap::new();

    for pack in packs {
        for patch in &pack.idioms {
            let acc = accs.entry(patch.id.clone()).or_insert_with(|| {
                order.push(patch.id.clone());
                Acc::default()
            });
            acc.apply(patch, pack.layer);
        }
    }

    let mut idioms = Vec::new();
    for id in order {
        let acc = accs.remove(&id).expect("id present");
        if acc.disabled {
            continue;
        }
        let idiom = acc.finalize(id)?;
        crate::validate::check_invariants(&idiom)?;
        idioms.push(idiom);
    }
    Ok(IdiomSet { idioms })
}
```

- [ ] **Step 4: Add a placeholder `check_invariants` so it compiles** in `validate.rs` (real body in Task 3)

```rust
//! Load-time validation of resolved idioms.
use crate::error::ResolveError;
use crate::resolve::Idiom;

pub fn check_invariants(_idiom: &Idiom) -> Result<(), ResolveError> {
    Ok(())
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p idiomatic-core resolve::`
Expected: PASS — override, inheritance, provenance, and disable all green.

- [ ] **Step 6: Commit**

```bash
git add crates/idiomatic-core/src/resolve.rs crates/idiomatic-core/src/validate.rs
git commit -m "feat(core): cascade resolver with field-level merge and provenance"
```

---

### Task 3: Validation invariants

**Files:**
- Modify: `crates/idiomatic-core/src/validate.rs`
- Test: inline `#[cfg(test)]` in `validate.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ResolveError;
    use crate::pack::{FixPolicy, Severity};
    use crate::resolve::Idiom;
    use std::collections::BTreeMap;

    fn idiom(fix_policy: FixPolicy, rule: bool, fix: Option<&str>) -> Idiom {
        Idiom {
            id: "x".into(),
            language: "python".into(),
            title: "t".into(),
            why: "w".into(),
            severity: Severity::Error,
            fix_policy,
            rule: rule.then(|| serde_yaml_ng::Value::Null),
            fix: fix.map(String::from),
            skill_prose: None,
            examples: None,
            provenance: BTreeMap::new(),
        }
    }

    #[test]
    fn autofix_requires_fix() {
        let err = check_invariants(&idiom(FixPolicy::Autofix, true, None)).unwrap_err();
        assert!(matches!(err, ResolveError::AutofixWithoutFix { .. }));
    }

    #[test]
    fn skill_only_rejects_rule() {
        let err = check_invariants(&idiom(FixPolicy::SkillOnly, true, None)).unwrap_err();
        assert!(matches!(
            err,
            ResolveError::SkillOnlyWithRule { .. } | ResolveError::RuleWithSkillOnly { .. }
        ));
    }

    #[test]
    fn unknown_language_rejected() {
        let mut i = idiom(FixPolicy::WarnAndInstruct, true, None);
        i.language = "cobol".into();
        let err = check_invariants(&i).unwrap_err();
        assert!(matches!(err, ResolveError::UnknownLanguage { .. }));
    }

    #[test]
    fn valid_autofix_passes() {
        assert!(check_invariants(&idiom(FixPolicy::Autofix, true, Some("y"))).is_ok());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p idiomatic-core validate::`
Expected: FAIL — placeholder always returns `Ok`.

- [ ] **Step 3: Implement `check_invariants`** (replace the placeholder body; keep the module header)

```rust
//! Load-time validation of resolved idioms.
use crate::engine::support_lang;
use crate::error::ResolveError;
use crate::pack::FixPolicy;
use crate::resolve::Idiom;

/// Enforce spec §5 invariants on a resolved idiom. Fail loud at load, never at
/// gate time.
pub fn check_invariants(idiom: &Idiom) -> Result<(), ResolveError> {
    let id = || idiom.id.clone();

    // Known language (parses into a SupportLang).
    if support_lang(&idiom.language).is_none() {
        return Err(ResolveError::UnknownLanguage { id: id(), lang: idiom.language.clone() });
    }

    match idiom.fix_policy {
        FixPolicy::Autofix if idiom.fix.is_none() => {
            return Err(ResolveError::AutofixWithoutFix { id: id() });
        }
        FixPolicy::SkillOnly if idiom.rule.is_some() => {
            return Err(ResolveError::SkillOnlyWithRule { id: id() });
        }
        _ => {}
    }
    Ok(())
}
```

- [ ] **Step 4: Add the `support_lang` helper stub to `engine.rs`** so `validate` compiles (full engine in Task 4)

```rust
//! ast-grep adapter.
use ast_grep_language::SupportLang;

/// Map an idiom `language` string to a SupportLang. Returns None for unknown.
pub fn support_lang(name: &str) -> Option<SupportLang> {
    match name.to_ascii_lowercase().as_str() {
        "python" | "py" => Some(SupportLang::Python),
        "typescript" | "ts" => Some(SupportLang::TypeScript),
        _ => None,
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p idiomatic-core validate::`
Expected: PASS — all four invariants enforced.

- [ ] **Step 6: Commit**

```bash
git add crates/idiomatic-core/src/validate.rs crates/idiomatic-core/src/engine.rs
git commit -m "feat(core): load-time validation invariants"
```

---

### Task 4: ast-grep engine adapter (spike-first)

**Files:**
- Modify: `crates/idiomatic-core/src/engine.rs`
- Test: inline `#[cfg(test)]` in `engine.rs`

> **Spike note:** `ast-grep-config` has ~33% doc coverage. Steps 1–2 are a deliberate spike: the first test pins the exact `from_yaml_string` signature, `GlobalRules` construction, the match-iteration call, and the match→edit application. If a signature differs from what's written here, fix the *adapter* to match the crate (the test asserts behavior, not signatures) and note the real signature in a code comment. Do not proceed to the seed pack until this test is green.

- [ ] **Step 1: Write the failing behavior test** (drives both lint and autofix)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{FixPolicy, Severity};
    use crate::resolve::Idiom;
    use std::collections::BTreeMap;

    fn compare_none() -> Idiom {
        let rule: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("pattern: \"$X == None\"").unwrap();
        Idiom {
            id: "compare-none".into(),
            language: "python".into(),
            title: "Use is None".into(),
            why: "identity".into(),
            severity: Severity::Warn,
            fix_policy: FixPolicy::Autofix,
            rule: Some(rule),
            fix: Some("$X is None".into()),
            skill_prose: None,
            examples: None,
            provenance: BTreeMap::new(),
        }
    }

    #[test]
    fn lint_reports_a_match() {
        let compiled = vec![CompiledIdiom::compile(&compare_none()).unwrap()];
        let hits = lint_source(&compiled, SupportLang::Python, "if x == None:\n    pass\n");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "compare-none");
    }

    #[test]
    fn autofix_rewrites_in_place() {
        let compiled = vec![CompiledIdiom::compile(&compare_none()).unwrap()];
        let (fixed, applied) =
            autofix_source(&compiled, SupportLang::Python, "if x == None:\n    pass\n");
        assert_eq!(fixed, "if x is None:\n    pass\n");
        assert_eq!(applied, 1);
    }

    #[test]
    fn good_source_is_untouched() {
        let compiled = vec![CompiledIdiom::compile(&compare_none()).unwrap()];
        let (fixed, applied) =
            autofix_source(&compiled, SupportLang::Python, "if x is None:\n    pass\n");
        assert_eq!(applied, 0);
        assert_eq!(fixed, "if x is None:\n    pass\n");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p idiomatic-core engine::`
Expected: FAIL — `CompiledIdiom`, `lint_source`, `autofix_source` not defined.

- [ ] **Step 3: Implement the adapter** (append below `support_lang` in `engine.rs`)

```rust
use crate::error::EngineError;
use crate::pack::FixPolicy;
use crate::resolve::Idiom;
use ast_grep_config::{from_yaml_string, GlobalRules, RuleConfig};
use ast_grep_core::AstGrep;
use ast_grep_language::LanguageExt;

/// One idiom compiled into an ast-grep rule, retaining the policy needed to
/// decide whether to autofix or report.
pub struct CompiledIdiom {
    pub id: String,
    pub fix_policy: FixPolicy,
    pub config: RuleConfig<SupportLang>,
}

/// A single lint hit (byte range into the source).
pub struct Hit {
    pub id: String,
    pub start: usize,
    pub end: usize,
}

impl CompiledIdiom {
    /// Bridge an idiom into ast-grep by synthesizing an ast-grep rule document
    /// (id + language + rule + optional fix) and parsing it via `from_yaml_string`.
    /// This keeps us off ast-grep's internal struct shapes.
    pub fn compile(idiom: &Idiom) -> Result<Self, EngineError> {
        let lang = support_lang(&idiom.language)
            .ok_or_else(|| EngineError::UnsupportedLanguage(idiom.language.clone()))?;

        let mut doc = serde_yaml_ng::Mapping::new();
        doc.insert("id".into(), idiom.id.clone().into());
        doc.insert("language".into(), idiom.language.clone().into());
        if let Some(rule) = &idiom.rule {
            doc.insert("rule".into(), rule.clone());
        }
        if let Some(fix) = &idiom.fix {
            doc.insert("fix".into(), fix.clone().into());
        }
        let yaml = serde_yaml_ng::to_string(&serde_yaml_ng::Value::Mapping(doc))
            .map_err(|e| EngineError::Compile { id: idiom.id.clone(), message: e.to_string() })?;

        let globals = GlobalRules::default();
        // SPIKE-CONFIRMED signature: from_yaml_string(&str, &GlobalRules)
        //   -> Result<Vec<RuleConfig<L>>, _> with L inferred from `language:`.
        let mut configs: Vec<RuleConfig<SupportLang>> = from_yaml_string(&yaml, &globals)
            .map_err(|e| EngineError::Compile { id: idiom.id.clone(), message: e.to_string() })?;
        let config = configs.pop().ok_or_else(|| EngineError::Compile {
            id: idiom.id.clone(),
            message: "from_yaml_string produced no rule".into(),
        })?;

        Ok(CompiledIdiom { id: idiom.id.clone(), fix_policy: idiom.fix_policy, config })
    }
}

/// Lint a source string with all compiled idioms; return every hit (no fixing).
pub fn lint_source(idioms: &[CompiledIdiom], lang: SupportLang, source: &str) -> Vec<Hit> {
    let ast: AstGrep<_> = lang.ast_grep(source);
    let mut hits = Vec::new();
    for idiom in idioms {
        for m in ast.root().find_all(&idiom.config.matcher) {
            let range = m.range();
            hits.push(Hit { id: idiom.id.clone(), start: range.start, end: range.end });
        }
    }
    hits
}

/// Apply every `autofix` idiom's rewrite to the source, returning the rewritten
/// text and the count of fixes applied. Non-autofix idioms are ignored here.
pub fn autofix_source(idioms: &[CompiledIdiom], lang: SupportLang, source: &str) -> (String, usize) {
    let ast: AstGrep<_> = lang.ast_grep(source);
    // (start, end, replacement) edits, collected then applied right-to-left.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();

    for idiom in idioms {
        if idiom.fix_policy != FixPolicy::Autofix {
            continue;
        }
        let fixers = idiom.config.get_fixer().unwrap_or_default();
        let Some(fixer) = fixers.into_iter().next() else { continue };
        for m in ast.root().find_all(&idiom.config.matcher) {
            // SPIKE-CONFIRMED: NodeMatch -> Edit via `replace_by(&fixer)`,
            // yielding (position, deleted_length, inserted_text). Adjust the two
            // lines below to the real Edit field names if they differ.
            let edit = m.replace_by(&fixer);
            let start = edit.position;
            let end = edit.position + edit.deleted_length;
            let text = String::from_utf8_lossy(&edit.inserted_text).into_owned();
            edits.push((start, end, text));
        }
    }

    let count = edits.len();
    edits.sort_by(|a, b| b.0.cmp(&a.0)); // right-to-left so offsets stay valid
    let mut out = source.to_string();
    for (start, end, text) in edits {
        out.replace_range(start..end, &text);
    }
    (out, count)
}
```

- [ ] **Step 4: Run the tests to verify they pass** (resolve any signature drift here)

Run: `cargo test -p idiomatic-core engine::`
Expected: PASS — match found, `if x == None:` rewrites to `if x is None:`, good source untouched.

- [ ] **Step 5: Commit**

```bash
git add crates/idiomatic-core/src/engine.rs
git commit -m "feat(core): ast-grep engine adapter (lint + autofix)"
```

---

### Task 5: Seed `python-core` pack + bundling

**Files:**
- Create: `crates/idiomatic-core/packs/python-core.yaml`
- Modify: `crates/idiomatic-core/src/lib.rs` (expose `builtin_packs()`)
- Test: `crates/idiomatic-core/tests/cascade_golden.rs`

- [ ] **Step 1: Author the seed pack** `crates/idiomatic-core/packs/python-core.yaml` (4 idioms, all policies)

```yaml
name: python-core
language: python
version: 0.1.0
---
id: compare-none
language: python
title: "Use `is None` instead of `== None`"
why: "None is a singleton; identity comparison is correct, faster, and idiomatic."
severity: warn
fix_policy: autofix
rule:
  pattern: "$X == None"
fix: "$X is None"
skill_prose: |
  Compare with `is None` / `is not None`, never `== None`. `None` is a singleton,
  so identity is what you actually mean, and `==` can be overridden by `__eq__`.
examples:
  bad: "if x == None:\n    pass"
  good: "if x is None:\n    pass"
---
id: negated-in
language: python
title: "Use `not in` instead of `not x in y`"
why: "`not in` is a single operator that reads naturally; `not (x in y)` is noisier."
severity: warn
fix_policy: autofix
rule:
  pattern: "not $X in $Y"
fix: "$X not in $Y"
examples:
  bad: "if not key in mapping:\n    pass"
  good: "if key not in mapping:\n    pass"
---
id: print-debugging
language: python
title: "Use logging instead of `print` for diagnostics"
why: "print() can't be filtered, leveled, or routed; logging can. There is no single safe rewrite."
severity: info
fix_policy: warn-and-instruct
rule:
  pattern: "print($$$ARGS)"
skill_prose: |
  Reach for the `logging` module instead of `print` for anything diagnostic.
  Get a module logger with `logger = logging.getLogger(__name__)` and call
  `logger.debug(...)` / `logger.info(...)` so output can be leveled and routed.
examples:
  bad: "print(\"value is\", value)"
  good: "logger.debug(\"value is %s\", value)"
---
id: flatten-nesting
language: python
title: "Flatten deep nesting with guard clauses"
why: "More than ~3 levels of nesting is a readability smell; early returns flatten it. Judgment call, no detector."
severity: info
fix_policy: skill-only
skill_prose: |
  When a function nests more than about three levels deep, invert conditions and
  return early (guard clauses) so the happy path stays at the left margin. This is
  a judgment call — there is no mechanical rewrite, so it is taught, not enforced.
```

- [ ] **Step 2: Write the failing golden test** `tests/cascade_golden.rs`

```rust
use idiomatic_core::pack::LoadedPack;
use idiomatic_core::{builtin_packs, resolve::resolve, Layer};

#[test]
fn seed_pack_resolves_to_four_idioms() {
    let packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base).unwrap())
        .collect();
    let set = resolve(&packs).unwrap();
    assert_eq!(set.len(), 4);

    let ids: Vec<&str> = set.iter().map(|i| i.id.as_str()).collect();
    insta::assert_yaml_snapshot!(ids);
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p idiomatic-core --test cascade_golden`
Expected: FAIL — `builtin_packs` not defined.

- [ ] **Step 4: Add `builtin_packs()` to `lib.rs`**

```rust
/// The seed packs shipped with the binary (the cascade's `base` layer).
pub fn builtin_packs() -> &'static [(&'static str, &'static str)] {
    &[("python-core", include_str!("../packs/python-core.yaml"))]
}
```

- [ ] **Step 5: Run, accept the snapshot, re-run**

Run: `cargo test -p idiomatic-core --test cascade_golden`
Then accept the generated snapshot: `cargo insta accept` (or rename `*.snap.new` → `*.snap`).
Re-run: `cargo test -p idiomatic-core --test cascade_golden`
Expected: PASS — four idioms, snapshot committed.

- [ ] **Step 6: Commit**

```bash
git add crates/idiomatic-core/packs/python-core.yaml crates/idiomatic-core/src/lib.rs crates/idiomatic-core/tests/
git commit -m "feat(core): seed python-core pack across all fix policies"
```

---

### Task 6: Self-testing example harness

**Files:**
- Modify: `crates/idiomatic-core/src/selftest.rs`
- Test: `crates/idiomatic-core/tests/selftest_seed.rs`

- [ ] **Step 1: Write the failing test** `tests/selftest_seed.rs`

```rust
use idiomatic_core::pack::LoadedPack;
use idiomatic_core::selftest::run_selftests;
use idiomatic_core::{builtin_packs, resolve::resolve, Layer};

#[test]
fn every_seed_idiom_passes_its_own_examples() {
    let packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base).unwrap())
        .collect();
    let set = resolve(&packs).unwrap();

    let results = run_selftests(&set);
    let failures: Vec<_> = results.iter().filter(|r| !r.passed).collect();
    assert!(failures.is_empty(), "self-test failures: {failures:#?}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p idiomatic-core --test selftest_seed`
Expected: FAIL — `run_selftests` not defined.

- [ ] **Step 3: Implement `selftest.rs`**

```rust
//! Each idiom is self-testing: its `examples.bad` must trip, `examples.good`
//! must pass, and for `autofix` idioms `autofix(bad)` must equal `good`.
use crate::engine::{autofix_source, lint_source, support_lang, CompiledIdiom};
use crate::pack::FixPolicy;
use crate::resolve::{Idiom, IdiomSet};

#[derive(Debug)]
pub struct SelfTestResult {
    pub id: String,
    pub passed: bool,
    pub detail: String,
}

fn pass(id: &str) -> SelfTestResult {
    SelfTestResult { id: id.into(), passed: true, detail: "ok".into() }
}
fn fail(id: &str, detail: impl Into<String>) -> SelfTestResult {
    SelfTestResult { id: id.into(), passed: false, detail: detail.into() }
}

fn test_one(idiom: &Idiom) -> SelfTestResult {
    // skill-only idioms have no detector — nothing to self-test.
    if idiom.fix_policy == FixPolicy::SkillOnly {
        return pass(&idiom.id);
    }
    let Some(examples) = &idiom.examples else { return pass(&idiom.id) };
    let lang = match support_lang(&idiom.language) {
        Some(l) => l,
        None => return fail(&idiom.id, "unknown language"),
    };
    let compiled = match CompiledIdiom::compile(idiom) {
        Ok(c) => vec![c],
        Err(e) => return fail(&idiom.id, format!("compile error: {e}")),
    };

    if let Some(bad) = &examples.bad {
        if lint_source(&compiled, lang, bad).is_empty() {
            return fail(&idiom.id, "examples.bad did not trip the rule");
        }
    }
    if let Some(good) = &examples.good {
        if !lint_source(&compiled, lang, good).is_empty() {
            return fail(&idiom.id, "examples.good incorrectly tripped the rule");
        }
    }
    if idiom.fix_policy == FixPolicy::Autofix {
        if let (Some(bad), Some(good)) = (&examples.bad, &examples.good) {
            let (fixed, _) = autofix_source(&compiled, lang, bad);
            if &fixed != good {
                return fail(&idiom.id, format!("autofix(bad) != good: got {fixed:?}"));
            }
        }
    }
    pass(&idiom.id)
}

/// Run self-tests for every idiom in the set.
pub fn run_selftests(set: &IdiomSet) -> Vec<SelfTestResult> {
    set.iter().map(test_one).collect()
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p idiomatic-core --test selftest_seed`
Expected: PASS — every seed idiom passes its own examples (this is also the gate that keeps `autofix` honest: `compare-none` and `negated-in` must round-trip bad→good).

- [ ] **Step 5: Commit**

```bash
git add crates/idiomatic-core/src/selftest.rs crates/idiomatic-core/tests/selftest_seed.rs
git commit -m "feat(core): self-testing example harness"
```

---

### Task 7: `idiomatic check` CLI

**Files:**
- Modify: `crates/idiomatic-cli/src/main.rs`
- Create: `crates/idiomatic-cli/src/check.rs`
- Test: `crates/idiomatic-cli/tests/check_cli.rs`

The CLI discovers layers (`base` = built-in packs; `user` = `~/.config/idiomatic/*.yaml`; `project` = `./.idiomatic/*.yaml`), resolves the cascade, then for each input file picks idioms whose language matches the file's extension and lints it. Default is report-only (CI/pre-commit); `--fix` applies `autofix` rewrites in place. Exit code: `0` if no `error`-severity hits remain, else `1`.

- [ ] **Step 1: Write the failing integration test** `tests/check_cli.rs`

```rust
use assert_cmd::Command;
use std::fs;

#[test]
fn check_fix_rewrites_a_python_file() {
    let dir = tempdir();
    let file = dir.join("sample.py");
    fs::write(&file, "if x == None:\n    pass\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["check", "--fix", file.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&file).unwrap(), "if x is None:\n    pass\n");
}

#[test]
fn check_reports_warn_and_instruct_without_fixing() {
    let dir = tempdir();
    let file = dir.join("p.py");
    fs::write(&file, "print(\"hi\")\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["check", file.to_str().unwrap()])
        .assert()
        .success() // info severity → exit 0
        .stdout(predicates::str::contains("print-debugging"));

    // unchanged: warn-and-instruct never rewrites
    assert_eq!(fs::read_to_string(&file).unwrap(), "print(\"hi\")\n");
}

fn tempdir() -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("idiomatic-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&base);
    base
}
```

Add `predicates = "3"` to `idiomatic-cli` `[dev-dependencies]` in its `Cargo.toml`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p idiomatic-cli --test check_cli`
Expected: FAIL — `check` subcommand unimplemented (stub binary).

- [ ] **Step 3: Implement `check.rs`**

```rust
//! `idiomatic check [--fix] <paths...>`
use anyhow::Result;
use idiomatic_core::engine::{autofix_source, lint_source, support_lang, CompiledIdiom};
use idiomatic_core::pack::{FixPolicy, LoadedPack};
use idiomatic_core::resolve::{resolve, Idiom, IdiomSet};
use idiomatic_core::{builtin_packs, Layer};
use std::fs;
use std::path::{Path, PathBuf};

pub struct CheckOutcome {
    pub had_error_severity: bool,
}

pub fn run(paths: &[PathBuf], fix: bool) -> Result<CheckOutcome> {
    let set = load_cascade()?;
    let mut had_error_severity = false;

    for path in paths {
        let Some(lang) = ext_lang(path) else { continue };
        let source = fs::read_to_string(path)?;

        // Compile only idioms whose language matches this file.
        let compiled: Vec<CompiledIdiom> = set
            .iter()
            .filter(|i| support_lang(&i.language) == Some(lang))
            .filter_map(|i| CompiledIdiom::compile(i).ok())
            .collect();

        if fix {
            let (fixed, n) = autofix_source(&compiled, lang, &source);
            if n > 0 {
                fs::write(path, &fixed)?;
                println!("applied {n} idiom fixes to {}", path.display());
            }
            // Re-lint the fixed text to report what autofix could not handle.
            report(&set, &compiled, lang, &fixed, path, &mut had_error_severity);
        } else {
            report(&set, &compiled, lang, &source, path, &mut had_error_severity);
        }
    }
    Ok(CheckOutcome { had_error_severity })
}

fn report(
    set: &IdiomSet,
    compiled: &[CompiledIdiom],
    lang: idiomatic_core::engine::SupportLang,
    source: &str,
    path: &Path,
    had_error: &mut bool,
) {
    use idiomatic_core::pack::Severity;
    for hit in lint_source(compiled, lang, source) {
        let Some(idiom) = set.get(&hit.id) else { continue };
        // autofix idioms that survive a fixed re-lint, plus warn-and-instruct, are reported.
        if idiom.fix_policy == FixPolicy::SkillOnly {
            continue;
        }
        if idiom.severity == Severity::Error {
            *had_error = true;
        }
        println!(
            "{}: [{}] {} — {}",
            path.display(),
            hit.id,
            idiom.title,
            idiom.why
        );
    }
    let _ = compiled; // silence unused in non-fix path shape
}

fn load_cascade() -> Result<IdiomSet> {
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

fn ext_lang(path: &Path) -> Option<idiomatic_core::engine::SupportLang> {
    match path.extension().and_then(|e| e.to_str())? {
        "py" => support_lang("python"),
        "ts" => support_lang("typescript"),
        _ => None,
    }
}
```

> **Note on the `Idiom` import:** `report` takes `&IdiomSet` and looks idioms up by id; the unused `Idiom` import can be dropped. Keep imports tight — `cargo build` warnings are failures in CI later.

- [ ] **Step 4: Implement `main.rs`**

```rust
mod check;

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
    }
}
```

Export `SupportLang` from core for the CLI: in `engine.rs` ensure `pub use ast_grep_language::SupportLang;` so `idiomatic_core::engine::SupportLang` resolves. (Add it near the top of `engine.rs`.)

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p idiomatic-cli --test check_cli`
Expected: PASS — `--fix` rewrites `== None`; `print(...)` is reported by id and left unchanged.

- [ ] **Step 6: Commit**

```bash
git add crates/idiomatic-cli/
git commit -m "feat(cli): idiomatic check with autofix and cascade discovery"
```

---

### Task 8: End-to-end verification + README

**Files:**
- Create: `README.md`

- [ ] **Step 1: Full test + lint pass**

Run: `cargo test --workspace`
Expected: PASS — all unit, golden, self-test, and CLI integration tests green.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS — no warnings (fix any; `cargo build` warnings become CI failures in step 8 of the spec).

- [ ] **Step 2: Manual smoke test**

```bash
printf 'if x == None:\n    if not k in d:\n        print(x)\n' > /tmp/smoke.py
cargo run -p idiomatic-cli -- check --fix /tmp/smoke.py
cat /tmp/smoke.py
```

Expected: `== None` → `is None`, `not k in d` → `k not in d`, `print(x)` reported as `print-debugging` and left in place. Output includes `applied 2 idiom fixes to /tmp/smoke.py`.

- [ ] **Step 3: Write `README.md`** documenting: what idiomatic is (1 paragraph from spec §1), the pack format (point at the spec), and `idiomatic check [--fix] <paths>`. Note that the PostToolUse hook, skill-gen, and Python binding are follow-on work.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: README for idiomatic core + check CLI"
```

---

## Verification

End-to-end, the core is proven when:

1. `cargo test --workspace` is green — covers: multi-doc load + duplicate-id rejection (Task 1), cascade override/inherit/provenance/disable (Task 2), all four validation invariants (Task 3), ast-grep lint + autofix round-trip (Task 4), the seed pack resolving to 4 idioms across all policies (Task 5), every seed idiom passing its own examples including `autofix(bad)==good` (Task 6), and the `check` CLI rewriting on `--fix` while reporting `warn-and-instruct` without touching the file (Task 7).
2. `cargo clippy --workspace --all-targets -- -D warnings` is clean.
3. The manual smoke test (Task 8 Step 2) shows both autofixes applied and `print-debugging` reported but unmodified.

This delivers spec §11 steps 1–3 as working, testable software. Follow-on plans: step 4 (PostToolUse hook reusing `autofix_source`/`lint_source`), step 5 (`skill-gen` renderer over `title`/`why`/`skill_prose`/`examples`), step 6 (PyO3 crate `crates/idiomatic-py` over `idiomatic-core`), step 7 (`typescript-core` pack — the adapter is already language-generic via `SupportLang`).

## Self-Review Notes

- **Spec coverage:** §5 pack format → Tasks 1,5. §5 invariants → Task 3. §6 cascade (field merge, disable, provenance) → Task 2. §7 backstop `check` → Task 7 (PostToolUse hook deferred to follow-on, per scope). §10 validation + self-testing + golden cascade → Tasks 1,3,5,6. §11 steps 1–3 → all tasks. §8 skill-gen, §9 telemetry: explicitly out of scope (follow-on).
- **Known risk:** Task 4 `replace_by`/`Edit` field names and `from_yaml_string` signature are spike-confirmed in Step 1–4; if the 0.43 API differs, fix the adapter (the tests assert behavior). This is the one place to expect friction.
- **YAGNI deltas surfaced:** list-append merge (no list field exists yet) and `no-range-len` autofix (not a clean single-pattern rewrite) — both documented in Context, neither blocks this plan.
