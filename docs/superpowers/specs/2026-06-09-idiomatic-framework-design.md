# `idiomatic` — Design Spec

**Status:** Approved design (pre-implementation)
**Date:** 2026-06-09
**Author:** Joe Black

## 1. Summary

`idiomatic` is a fast, language-abstracted **idiom enforcement framework** for AI
coding agents (and humans). It exists to make agents write idiomatic code *the
first time*, and to silently repair the cases they don't — without dragging the
agent through expensive rewrite loops.

Two halves work together:

1. **Teach up front.** A generated skill carries every idiom as prose the agent
   reads before writing code.
2. **Enforce in-loop.** A sub-100ms gate fires inside the agent loop after each
   file write. Whatever is mechanically repairable, it *autofixes silently*. Only
   the rare violation with no safe rewrite is bounced back to the agent. The same
   idiom packs gate the repository in CI as a backstop.

Working name: `idiomatic` (placeholder — free to change).

## 2. Goals & Non-Goals

### Goals

- **Language-abstracted core.** Everything except the idiom rules themselves is
  language-agnostic. Python and TypeScript ship as the two reference languages;
  adding Go/Rust/etc. is a matter of authoring packs, not changing the core.
- **Extensible and overridable.** Anyone can add new idioms, disable inherited
  ones, or change behavior of existing ones via a config cascade — without
  touching Rust.
- **Fast enough for the hot path.** The live gate must run well under ~100ms per
  file so it never makes the agent feel sluggish.
- **Repair over report.** Mechanically decidable violations are fixed in place,
  not handed back as homework.
- **Single source of truth.** Idiom packs render to *both* skill prose and
  fix-it diagnostics. No drift between "what we teach" and "what we enforce."

### Non-Goals (initial)

- Judgment-only idioms in the gate (abstraction altitude, "feels idiomatic").
  These live in skill prose only.
- A Stop/pre-completion gate (YAGNI for v1).
- Node binding and `--explain` surface (deferred until Python + TS prove out).
- Building our own match engine (we stand on `ast-grep`).

## 3. Key Design Decisions

These were settled during brainstorming and are load-bearing:

1. **Idioms split into two populations.** *Mechanically decidable* (AST-shape
   rules — fast, deterministic) vs. *judgment calls* (need an LLM or human). The
   fast gate targets only the first; the second lives in prose. Never ask the
   fast check to validate vibes.

2. **Stand on `ast-grep`, don't build the matcher.** `ast-grep` is already
   Rust-core + tree-sitter + YAML pattern rules + Python/Node bindings + fast +
   MIT. The matcher is the commodity; our value is the layer above it. Owning the
   matcher would be a tax, not a moat.

3. **The gate is an autofixer, not a blocker.** This is the fix for the classic
   "markdown linter that makes the agent rewrite tables" pain. Reporting a
   mechanically-decidable fix is a design defect; repair it instead.

4. **`fix_policy` tri-state** per idiom: `autofix` / `warn-and-instruct` /
   `skill-only`. Only `warn-and-instruct` ever interrupts the agent.

5. **The lint rules are the machine-checkable subset of the skill.** One ruleset,
   two renderings (prose + executable). Generated from the same source.

6. **Rust core + bindings**, following the Ruff / Pydantic-core / ast-grep model
   (PyO3 + maturin for Python; napi for Node, later).

## 4. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Renderers        skill-gen (prose)   │   diagnostic (fix-it) │
├─────────────────────────────────────────────────────────────┤
│ Cascade resolver   base → user → project, field-level merge, │
│                    provenance tracked internally             │
├─────────────────────────────────────────────────────────────┤
│ Idiom-pack loader  YAML packs → validated IdiomSet           │
├─────────────────────────────────────────────────────────────┤
│ Match engine       ast-grep (tree-sitter, Rust, MIT)         │
└─────────────────────────────────────────────────────────────┘
   bindings:     Rust core → PyO3/maturin (Python) + napi (Node, later)
   integrations: PostToolUse hook (live) │ pre-commit / CI (backstop)
```

Everything above the match engine is owned. `ast-grep` is a dependency, never a
fork.

### Component responsibilities

| Component | Does | Depends on |
|---|---|---|
| Pack loader | Parse + validate YAML packs into an `IdiomSet` | YAML, schema |
| Cascade resolver | Merge layers by `id`, field-level, with provenance | Pack loader |
| Match engine adapter | Run resolved rules via ast-grep, collect matches & fixes | ast-grep |
| Diagnostic renderer | Render a violation as an agent-facing fix-it instruction | Resolver |
| Skill-gen renderer | Render the resolved set as a Claude Code skill | Resolver |
| PostToolUse hook | Lint one touched file, autofix or instruct | Engine + renderers |
| CLI (`check`) | Lint a path set for pre-commit / CI | Engine + renderers |

## 5. The Idiom Pack (core artifact)

One idiom = one YAML record. A pack is a directory of idiom records plus a
manifest (`pack.yaml`: name, language, version).

```yaml
id: no-range-len
language: python
title: "Use enumerate instead of range(len(...))"
why: "enumerate is clearer, avoids manual indexing bugs, and is the Pythonic idiom."
severity: error            # error | warn | info
fix_policy: autofix        # autofix | warn-and-instruct | skill-only
rule:                      # passed through to ast-grep; omitted for skill-only
  pattern: "for $I in range(len($SEQ)):"
fix: "for $I, $X in enumerate($SEQ):"   # ast-grep rewrite; required iff autofix
skill_prose: |             # rendered into the skill; optional, falls back to why
  Prefer `enumerate(seq)` over `range(len(seq))`. It reads as "index and item"
  and removes the `seq[i]` indirection that invites off-by-one errors.
examples:                  # optional; feed both the skill and the rule's tests
  bad:  "for i in range(len(items)):"
  good: "for i, item in enumerate(items):"
```

### Field semantics

- **`fix_policy`** is the tri-state that solves the rewrite-loop pain:
  - `autofix` — a deterministic, safe rewrite exists. The hook applies it in
    place; the agent is never interrupted. **Requires `fix`.**
  - `warn-and-instruct` — a detector exists but no single safe rewrite. The
    *only* policy that bounces back to the agent, carrying `title` + `why` +
    idiomatic shape.
  - `skill-only` — pure judgment, no detector. Lives only in prose. **Omits
    `rule`.**
- **`severity`** drives CLI exit codes and reporting, independent of `fix_policy`.

### Validation invariants (enforced at load)

- `fix_policy: autofix` ⇒ `fix` is present.
- `rule` present ⇒ `fix_policy` ≠ `skill-only`.
- `fix_policy: skill-only` ⇒ `rule` absent.
- `id` unique within a layer.
- `language` is a known/registered language.
- Schema-valid fields and types.

Failures are loud at load time, never deferred to gate time.

## 6. Cascade Resolution

Layers, lowest to highest precedence:

```
base (shipped packs) → user (~/.config/idiomatic) → project (./.idiomatic)
```

- **Field-level merge by `id`.** A higher layer overrides only the fields it
  names. `{id: no-range-len, severity: warn}` changes severity and keeps tracking
  the upstream `pattern`/`fix` — no copy-paste freeze, no drift when base
  improves.
- **Disable** with `disabled: true` on the `id`.
- **List-valued fields** (e.g. multiple patterns/examples) **append by default**;
  opt out with `replace: true` to discard the inherited list. This is the single
  borrowed bit of Tailwind-style extend/override, applied only where flat merge is
  genuinely ambiguous.
- **Provenance** (which layer set each resolved field) is tracked inside the
  resolver from v1. The `--explain <id>` surface that prints the resolution chain
  is deferred, but the data model supports it without a rewrite.

## 7. Enforcement Integration

### Primary — PostToolUse hook on Write/Edit (live steering)

1. Hook fires after a file mutation; lints **only the touched file**.
2. `autofix` violations → rewrite the file in place, return
   `"applied N idiom fixes to <file>"` so the agent's stale in-context copy is
   corrected and it knows the file changed under it.
3. `warn-and-instruct` violations → return the idiom `title` + `why` + idiomatic
   shape; the agent fixes on its next turn.
4. `skill-only` idioms are never evaluated here.

Performance budget: **< 100ms per file.** This budget is the entire reason for
Rust + ast-grep.

### Backstop — pre-commit + CI

`idiomatic check <paths>` runs the same packs through the same resolver. Humans
and agents are held to the identical ruleset. Exit codes driven by `severity`.

### Deferred

Stop/pre-completion gate — skipped for v1 (YAGNI).

## 8. Skill Generation

`idiomatic skill-gen <language>` resolves the cascade and renders a Claude Code
skill (`SKILL.md` + idiom sections) from `title` / `why` / `skill_prose` /
`examples`.

- The skill is a **build artifact**, never hand-edited. Single source of truth is
  the packs; regenerate on pack change.
- This is the "teach it right the first time" half. The gate is the safety net
  that should ideally have nothing to do.

## 9. Feedback Loop (differentiator)

The gate emits structured telemetry per trip: `idiom id`, `file`, `fix_policy`.

- `warn-and-instruct` trips are a **signal that the skill prose is failing to
  teach** that idiom — not merely a code defect.
- Ranking idioms by trip-count points directly at which skill sections to
  strengthen. This closes the loop between linter and skill — the part nothing
  else on the market does.

## 10. Error Handling & Testing

- **Pack validation on load** (Section 5 invariants): schema, policy/fix/rule
  coherence, unknown-language guard, duplicate-id detection. Fail loud at load.
- **Each idiom is self-testing.** Its `examples.bad` / `examples.good` run as
  fixtures: `bad` must trip, `good` must pass, and for `autofix` idioms
  `autofix(bad)` must equal `good`. This keeps community-contributed idioms honest
  and makes "add an idiom" safe.
- **Cascade tests**: golden-file resolution chains across base/user/project.
- **Bindings**: Rust core unit-tested; Python binding smoke-tested via maturin.

## 11. Build Order (first sub-project scope)

1. Rust core: pack loader + validator + cascade resolver (provenance internal).
2. Wrap `ast-grep`; wire `check` (CLI) against a seed `python-core` pack.
3. Self-testing example harness.
4. PostToolUse hook (autofix + warn rendering).
5. `skill-gen` renderer.
6. PyO3 binding + maturin packaging.
7. Seed `typescript-core` pack to prove language abstraction.
8. Node binding + pre-commit/CI recipe — later.

## 12. Open Questions / Deferred

- Node binding — after Python + TS core proves out.
- `--explain` surface — data model ready from v1, CLI deferred.
- Telemetry storage/format for the feedback loop — design when wiring step 4.
