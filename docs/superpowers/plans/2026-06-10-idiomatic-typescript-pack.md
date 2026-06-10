# `idiomatic` TypeScript Pack Implementation Plan (Build Order §11, Step 7)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a seed `typescript-core` idiom pack and prove the framework's core thesis end-to-end — that adding a language is *authoring a pack*, not changing the core: `check`, `hook`, and `skill-gen` all work for TypeScript with zero engine changes.

**Architecture:** Author `typescript-core.yaml` (4 idioms across all three `fix_policy` values), add it to `builtin_packs()`, and let the existing language-generic machinery do the rest — the engine already maps `SupportLang::TypeScript`, `ext_lang` already routes `.ts`, the resolver already merges language-tagged idioms into one set, and the self-test harness already runs each idiom in its own language. The only code change is one line in `builtin_packs()`; everything else is authoring + test updates.

**Tech Stack:** Existing Rust workspace + `ast-grep` (its tree-sitter TypeScript grammar is already pulled in via `ast-grep-language`). No new dependencies.

**Scope:** Spec §11 step 7 only. Out of scope (follow-on): PyO3 binding (step 6), Node binding + CI recipe (step 8), and a telemetry reader (§9 analysis half).

---

## Context

Steps 1–5 (on `main`) gave us a language-generic core: pack loader → resolver → ast-grep engine → renderers → live `hook` + `skill-gen`, all driven by language-tagged idioms. Everything was *built* generic but only ever *exercised* against Python. This phase is the proof: a second language, TypeScript, added purely by authoring a pack. If `check`/`hook`/`skill-gen` work for `.ts` files with only a one-line `builtin_packs()` change, the abstraction holds.

Decision (via AskUserQuestion): scope = TypeScript pack (step 7) alone; PyO3 binding deferred.

**Key facts about the existing machinery this relies on (verified against current code):**
- `idiomatic_core::builtin_packs()` returns `&'static [(&str, &str)]` — currently just `python-core`. The resolver loads all entries as `Layer::Base` and merges their idioms into one `IdiomSet`.
- `engine::support_lang("typescript" | "ts")` → `Some(SupportLang::TypeScript)`; `cascade::ext_lang` maps `.ts` → TypeScript. So a `.ts` file is linted only against TS-language idioms; a `.py` file only against Python ones.
- `selftest::run_selftests(&IdiomSet)` compiles and runs *each idiom in its own language* (`support_lang(&idiom.language)`), so the existing `selftest_seed` integration test will **automatically validate the new TypeScript idioms** the moment they're in `builtin_packs()`. This is the architecture-generalization proof — no new harness needed.
- `render_skill(&set, language)` filters idioms by language, so `skill-gen typescript` renders only TS idioms.

**Load-bearing risk (the one thing to validate, like the Python engine spike):** the TS idiom *patterns* must actually match and round-trip under ast-grep's TypeScript grammar. The chosen patterns below are best-known; **the existing `selftest_seed` test is the acceptance gate** (`bad` trips, `good` passes, `autofix(bad) == good` byte-exact). Task 1 is authorized to adjust patterns/examples to satisfy the harness, keeping each idiom's intent and its `fix_policy`. The riskiest is `optional-chaining` (a repeated metavariable `$A && $A.$B`); validate it first.

**Spec delta surfaced (deliberate):** the cascade resolver keys merges by `id` alone (spec §6), not by `(language, id)`. With two languages in `base`, idiom ids must be **globally unique across languages** or a Python and a TypeScript idiom sharing an id would wrongly merge. The TS ids below are deliberately distinct from the Python ones, so this is not a problem today — but it's a latent multi-language hazard. Task 3 files a follow-up to key the resolver by `(language, id)`; this plan does not change the resolver.

---

## File Structure

```
crates/idiomatic-core/
  packs/typescript-core.yaml          # NEW: 4 TS idioms (2 autofix, 1 warn, 1 skill-only)
  src/lib.rs                          # MODIFY: add typescript-core to builtin_packs()
  tests/cascade_golden.rs             # MODIFY: now resolves to 8 idioms (was 4)
  tests/snapshots/*.snap              # MODIFY: regenerated id list
  tests/skill_render.rs               # MODIFY: add a TypeScript render assertion
crates/idiomatic-cli/
  tests/ts_cli.rs                     # NEW: check --fix and skill-gen for TypeScript
```

No source modules change except the one `builtin_packs()` line — that's the whole point.

---

### Task 1: Author the `typescript-core` pack and wire it in

**Files:**
- Create: `crates/idiomatic-core/packs/typescript-core.yaml`
- Modify: `crates/idiomatic-core/src/lib.rs`, `crates/idiomatic-core/tests/cascade_golden.rs`, the snapshot, `crates/idiomatic-core/tests/skill_render.rs`

- [ ] **Step 1: Author `crates/idiomatic-core/packs/typescript-core.yaml`** (4 idioms; ids deliberately distinct from the Python pack):

```yaml
name: typescript-core
language: typescript
version: 0.1.0
---
id: triple-equals
language: typescript
title: "Use `===` instead of `==`"
why: "`==` does type coercion with surprising rules; `===` compares without coercion and is the TS/JS idiom."
severity: warn
fix_policy: autofix
rule:
  pattern: "$A == $B"
fix: "$A === $B"
skill_prose: |
  Always compare with `===` / `!==`, never `==` / `!=`. Loose equality coerces
  operands with rules almost nobody remembers; strict equality says exactly what
  you mean. (The one classic exception — `x == null` to catch both `null` and
  `undefined` — is better written explicitly.)
examples:
  bad: "if (a == b) {\n  return;\n}"
  good: "if (a === b) {\n  return;\n}"
---
id: optional-chaining
language: typescript
title: "Use optional chaining instead of `a && a.b`"
why: "`a?.b` is shorter, communicates intent, and avoids the truthiness pitfalls of `&&` guards."
severity: warn
fix_policy: autofix
rule:
  pattern: "$A && $A.$B"
fix: "$A?.$B"
examples:
  bad: "const name = user && user.name;"
  good: "const name = user?.name;"
---
id: no-console
language: typescript
title: "Use a logger instead of `console.log` for diagnostics"
why: "console.log can't be leveled, filtered, or routed and tends to ship to production. There is no single safe rewrite."
severity: info
fix_policy: warn-and-instruct
rule:
  pattern: "console.log($$$ARGS)"
skill_prose: |
  Reach for a real logger (pino, winston, or your app's logging module) instead of
  `console.log` for anything diagnostic, so output can be leveled and routed and
  doesn't leak to production stdout.
examples:
  bad: "console.log(\"value\", value);"
  good: "logger.debug(\"value\", value);"
---
id: prefer-named-exports
language: typescript
title: "Prefer named exports over a default export"
why: "Named exports give every consumer the same name, enable safe renames and tree-shaking, and avoid default-export refactor friction. Judgment call, no detector."
severity: info
fix_policy: skill-only
skill_prose: |
  Prefer named exports (`export const x`, `export function f`) over `export
  default`. Named exports keep one canonical name across the codebase, make
  rename-symbol and auto-import reliable, and avoid the default-export quirks with
  re-exports and tree-shaking. This is a design judgment, so it's taught, not
  enforced.
```

- [ ] **Step 2: Add the pack to `builtin_packs()`** in `crates/idiomatic-core/src/lib.rs` — change:

```rust
pub fn builtin_packs() -> &'static [(&'static str, &'static str)] {
    &[("python-core", include_str!("../packs/python-core.yaml"))]
}
```

to:

```rust
pub fn builtin_packs() -> &'static [(&'static str, &'static str)] {
    &[
        ("python-core", include_str!("../packs/python-core.yaml")),
        ("typescript-core", include_str!("../packs/typescript-core.yaml")),
    ]
}
```

- [ ] **Step 3: Run the self-test harness — this is the acceptance gate for the patterns**

Run: `cargo test -p idiomatic-core --test selftest_seed`
Expected: PASS — `every_seed_idiom_passes_its_own_examples` now also runs the 4 TS idioms: `triple-equals` and `optional-chaining` must round-trip `autofix(bad) == good` byte-exact, `no-console`'s `bad` must trip and `good` must not, `prefer-named-exports` (skill-only) is skipped.

**If a TS idiom fails:** debug with a throwaway probe (e.g. the `ast-grep` CLI if installed, or a scratch `#[test]` calling `engine::autofix_source`) to see what ast-grep's TypeScript grammar actually matches/produces. You are authorized to adjust the `pattern`, `fix`, or `examples` in `typescript-core.yaml` to make the idiom genuinely correct — keep the idiom's intent and its `fix_policy`, keep 4 idioms across the same 3 policies. Validate `optional-chaining` (repeated metavar `$A`) first; if ast-grep can't bind a repeated metavar in a rewrite there, fall back to making `optional-chaining` `warn-and-instruct` (drop `fix`, keep `rule`) and add a second clean autofix idiom in its place (e.g. `no-var`-style is not clean; prefer something like `!== undefined` normalization only if it round-trips). Report any change and the ast-grep behavior that motivated it.

- [ ] **Step 4: Update the cascade golden test** `crates/idiomatic-core/tests/cascade_golden.rs` — the seed now resolves to 8 idioms. Replace the test with:

```rust
use idiomatic_core::pack::LoadedPack;
use idiomatic_core::{builtin_packs, resolve::resolve, Layer};

#[test]
fn seed_packs_resolve_across_languages() {
    let packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base).unwrap())
        .collect();
    let set = resolve(&packs).unwrap();
    assert_eq!(set.len(), 8); // 4 python + 4 typescript

    let ids: Vec<&str> = set.iter().map(|i| i.id.as_str()).collect();
    insta::assert_yaml_snapshot!(ids);
}
```

- [ ] **Step 5: Regenerate the snapshot**

Run: `cargo test -p idiomatic-core --test cascade_golden`
The test name changed, so the old snapshot file (`cascade_golden__seed_pack_resolves_to_four_idioms.snap`) is now stale and a new pending snapshot is produced. Accept the new one (`cargo insta accept`, or rename the `.snap.new` → `.snap`) and **delete the stale old snapshot file**. The new snapshot should list 8 ids in resolution order: compare-none, negated-in, print-debugging, flatten-nesting, triple-equals, optional-chaining, no-console, prefer-named-exports.
Re-run: `cargo test -p idiomatic-core --test cascade_golden`
Expected: PASS.

- [ ] **Step 6: Add a TypeScript skill-render assertion** to `crates/idiomatic-core/tests/skill_render.rs` (append this test; leave the existing python test as-is):

```rust
#[test]
fn renders_typescript_skill_from_seed_pack() {
    let packs: Vec<idiomatic_core::pack::LoadedPack> = idiomatic_core::builtin_packs()
        .iter()
        .map(|(_, yaml)| idiomatic_core::pack::LoadedPack::from_yaml_str(yaml, idiomatic_core::Layer::Base).unwrap())
        .collect();
    let set = idiomatic_core::resolve::resolve(&packs).unwrap();

    let skill = idiomatic_core::render::render_skill(&set, "typescript");

    assert!(skill.contains("name: idiomatic-typescript"));
    assert!(skill.contains("Use `===` instead of `==`"));
    assert!(skill.contains("```typescript"));
    // python idioms must NOT leak into the typescript skill
    assert!(!skill.contains("Use `is None`"));
}
```

- [ ] **Step 7: Verify the full core suite + clippy**

Run: `cargo test -p idiomatic-core`
Expected: PASS — selftest_seed (now 8 idioms), cascade_golden (8), both skill_render tests, all prior unit tests.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/idiomatic-core/packs/typescript-core.yaml crates/idiomatic-core/src/lib.rs crates/idiomatic-core/tests/
git commit -m "feat(core): seed typescript-core pack across all fix policies"
```

---

### Task 2: Prove the CLI loop works for TypeScript

**Files:**
- Create: `crates/idiomatic-cli/tests/ts_cli.rs`

This task adds no source code — it proves `check`/`skill-gen` (and, by sharing the same engine path, the hook) work for `.ts` files purely from the pack added in Task 1.

- [ ] **Step 1: Write the failing integration test** `crates/idiomatic-cli/tests/ts_cli.rs`:

```rust
use assert_cmd::Command;
use std::fs;

fn tmp(name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("idiomatic-ts-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    base.join(name)
}

#[test]
fn check_fix_rewrites_a_typescript_file() {
    let file = tmp("a.ts");
    fs::write(&file, "if (a == b) {\n  return;\n}\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["check", "--fix", file.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&file).unwrap(), "if (a === b) {\n  return;\n}\n");
}

#[test]
fn check_reports_no_console_without_fixing() {
    let file = tmp("b.ts");
    fs::write(&file, "console.log(\"hi\");\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["check", file.to_str().unwrap()])
        .assert()
        .success() // info severity → exit 0
        .stdout(predicates::str::contains("no-console"));

    assert_eq!(fs::read_to_string(&file).unwrap(), "console.log(\"hi\");\n");
}

#[test]
fn skillgen_renders_typescript() {
    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["skill-gen", "typescript"])
        .assert()
        .success()
        .stdout(predicates::str::contains("name: idiomatic-typescript"))
        .stdout(predicates::str::contains("Use `===` instead of `==`"));
}
```

- [ ] **Step 2: Run to verify it fails — or passes**

Run: `cargo test -p idiomatic-cli --test ts_cli`
Expected: this should PASS immediately if Task 1's pack is correct (no source changes needed — the CLI is already language-generic). If `check_fix_rewrites_a_typescript_file` fails on the exact rewritten bytes, the discrepancy is in the `triple-equals` pattern/fix or the example formatting from Task 1 — reconcile by matching the test's expected output to what the idiom actually produces (and ensure Task 1's self-test example agrees). The test asserting byte-exact output is the real bar; if ast-grep formats the rewrite differently, fix the *expectation* to the correct ast-grep output and make the Task 1 example match it, so the two never disagree.

> Note: this is the rare task where the test may pass on first run. That's the proof the abstraction holds — keep the test; it's the regression guard for "TypeScript still works."

- [ ] **Step 3: Commit**

```bash
git add crates/idiomatic-cli/tests/ts_cli.rs
git commit -m "test(cli): prove check + skill-gen work for typescript"
```

---

### Task 3: Verification, README, and the resolver follow-up

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Full suite + lint**

Run: `cargo test --workspace`
Expected: PASS — all prior tests + the new TS self-tests, golden (8), TS skill render, and 3 `ts_cli` cases.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 2: Manual end-to-end across both languages**

```bash
cargo build --release -q
# TypeScript autofix + report
printf 'if (a == b) {\n  const n = user && user.name;\n  console.log(n);\n}\n' > /tmp/t.ts
./target/release/idiomatic check --fix /tmp/t.ts; echo "exit=$?"
echo "--- after ---"; cat /tmp/t.ts
# TypeScript skill
./target/release/idiomatic skill-gen typescript | head -16
# Python still works (no regression)
printf 'if x == None:\n    pass\n' > /tmp/t.py
./target/release/idiomatic check --fix /tmp/t.py; cat /tmp/t.py
```

Expected: `a == b` → `a === b`, `user && user.name` → `user?.name`, `console.log(n)` reported as `no-console` and left in place; the TS skill renders; Python `== None` → `is None` still works.

- [ ] **Step 3: Update `README.md`** — TypeScript is now a shipped language:
  - In "What works today" (or the pack section), note that **two reference languages ship**: `python-core` and `typescript-core`, and that `check`/`hook`/`skill-gen` work for `.py` and `.ts`.
  - Update the Status line: the framework now proves language abstraction with Python + TypeScript; PyO3 binding and Node/CI remain follow-on.
  - Add a one-line TypeScript example alongside the Python one if natural.

- [ ] **Step 4: File the resolver follow-up** (the latent `(language, id)` hazard):

```bash
bd create -t bug -p 2 "Cascade resolver keys merges by id alone, not (language, id)" -d "resolve() groups idiom patches by id across all packs/layers regardless of language. With multiple language packs in base, a Python and a TypeScript idiom sharing an id would wrongly merge into one idiom. Today TS ids are deliberately distinct from Python ids so it's latent, but adding languages or third-party packs makes collisions likely. Fix: key the resolver (and duplicate-id detection) by (language, id). Found while adding the typescript-core pack (step 7)."
```

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: README for shipped TypeScript pack"
```

---

## Verification

The phase is proven when:

1. `cargo test --workspace` is green — critically, `selftest_seed` now validates all 4 TypeScript idioms through the real ast-grep TypeScript grammar (the architecture-generalization proof), and `ts_cli` shows `check --fix` and `skill-gen` working for `.ts` with zero source changes beyond the one `builtin_packs()` line.
2. `cargo clippy --workspace --all-targets -- -D warnings` is clean.
3. The manual run (Task 3 Step 2) autofixes and reports a real `.ts` file and renders a TypeScript skill, while Python continues to work unchanged.

This delivers spec §11 step 7 and validates the central design claim: **a new language is a pack, not a core change** (the only Rust touched is one line adding the pack to `builtin_packs()`). Follow-on: PyO3 binding (step 6), Node binding + pre-commit/CI recipe (step 8), telemetry reader (§9), and the `(language, id)` resolver keying filed in Task 3.

## Self-Review Notes

- **Spec coverage:** §11 step 7 (seed typescript-core pack proving language abstraction) → Task 1. §7/§8 behavior for the new language (check/hook/skill-gen) → Task 2. §10 self-testing extends to TS automatically via the existing harness → Task 1 Step 3.
- **The one real risk is pattern correctness** under ast-grep's TypeScript grammar (esp. `optional-chaining`'s repeated metavar) — gated by the existing self-test harness, with explicit authorization and a fallback to reclassify `optional-chaining` as `warn-and-instruct` if ast-grep can't rewrite a repeated metavar.
- **Latent hazard surfaced, not silently absorbed:** the resolver's id-only keying is documented and filed as a follow-up; TS ids are kept globally distinct so nothing breaks now.
- **Type/name consistency:** the new test `seed_packs_resolve_across_languages` replaces `seed_pack_resolves_to_four_idioms` (old snapshot deleted); `render_skill(&set, "typescript")` matches the signature used by the Python test and the `skill-gen` CLI.
- **Minimal-change discipline:** exactly one source line changes (`builtin_packs()`); the rest is authoring + tests, which is precisely what "adding a language is authoring a pack" should mean.
