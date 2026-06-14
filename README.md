# idiomatic

A fast, language-abstracted **idiom enforcement framework** for AI coding agents
(and humans). It makes agents write idiomatic code the first time — a generated
skill carries every idiom as prose the agent reads before writing code — and
silently repairs the cases they don't, via a sub-100ms gate that *autofixes*
mechanically-decidable violations in place instead of bouncing the agent through
rewrite loops. The same idiom packs back a `check` CLI for pre-commit and CI, so
humans and agents are held to one ruleset.

> **Status:** All eight build-order steps are implemented — two reference
> languages (**Python and TypeScript**), **Python (PyO3)** and **Node (napi)**
> bindings, and a prebuilt-binary + pre-commit + CI distribution story. The
> release/publish workflows activate on the first tagged release once a Git
> remote is configured. See
> [the design spec](docs/superpowers/specs/2026-06-09-idiomatic-framework-design.md).

## What works today

- **Two reference languages** — `python-core` and `typescript-core` ship as seed
  packs; `check`/`hook`/`skill-gen` all work for `.py` and `.ts`. Adding a language
  is authoring a pack, not changing the core (TypeScript was one source line + a
  YAML pack).
- **Idiom packs** — a single multi-document YAML file per language: a manifest
  document followed by one document per idiom. Loaded, validated, and resolved
  through a `base → user → project` cascade with field-level merge by `id`.
- **ast-grep engine** — each mechanically-decidable idiom is bridged into
  [`ast-grep`](https://ast-grep.github.io) (tree-sitter, Rust) to match and, where
  a safe rewrite exists, autofix.
- **Self-testing idioms** — every idiom's `examples.bad`/`examples.good` run as
  fixtures: `bad` must trip, `good` must pass, and `autofix(bad)` must equal
  `good`. Bad idioms can't ship.
- **`idiomatic check`** — lint a path set against the resolved cascade.
- **`idiomatic hook`** — the live PostToolUse gate (the "enforce in-loop" half):
  autofixes the touched file in place, and only feeds the rare `warn-and-instruct`
  violation back to the agent. Install with `idiomatic install-hook`.
- **`idiomatic skill-gen`** — renders the resolved idioms as a Claude Code skill
  (the "teach up front" half), so the agent writes idiomatic code the first time.
- **Telemetry** — each hook trip is appended to `~/.idiomatic/telemetry.jsonl`
  (`{idiom_id, file, fix_policy, ts}`) to power trip-count ranking later.
- **Python binding** — an `idiomatic` Python package (PyO3, built with maturin)
  exposes the same engine in-process: `lint(source, language)`,
  `autofix(source, language)`, and `render_skill(language)`.
- **Node binding** — a napi addon (`lint`, `autofix`, `renderSkill`) exposing the
  same engine to JavaScript/TypeScript tooling.
- **Distribution** — prebuilt CLI binaries (cargo-dist → GitHub Releases with
  shell/PowerShell installers), per-platform npm packages for the Node addon, a
  `.pre-commit-hooks.yaml`, and an example CI workflow.

## Install

```sh
# Prebuilt CLI binary (once a release is published):
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/joeblackwaslike/idiomatic/releases/latest/download/idiomatic-cli-installer.sh | sh

# From source:
cargo install --path crates/idiomatic-cli
```

The Python binding builds with `cd crates/idiomatic-py && maturin develop`; the
Node addon with `cd crates/idiomatic-node && npm install && npm run build`.

## Usage

```sh
# Report idiom violations (CI / pre-commit; exit 1 on error-severity hits):
idiomatic check src/

# Apply autofixes in place and report what can't be auto-fixed:
idiomatic check --fix src/

# Generate the teaching skill for a language (stdout, or --out <dir> writes SKILL.md):
idiomatic skill-gen python

# Install the live PostToolUse gate into a Claude Code settings.json:
idiomatic install-hook            # defaults to .claude/settings.json
```

Once installed, `idiomatic hook` fires after every Write/Edit: it silently
autofixes what it can and only interrupts the agent for a `warn-and-instruct`
violation. Disable telemetry with `IDIOMATIC_NO_TELEMETRY=1` or redirect it with
`IDIOMATIC_TELEMETRY=<path>`.

Example, on a file containing `if x == None:` and `print(x)`:

```
applied 1 idiom fixes to app.py
app.py: [print-debugging] Use logging instead of `print` for diagnostics — print() can't be filtered, leveled, or routed; logging can.
```

`== None` is rewritten to `is None` silently; `print(...)` has no single safe
rewrite, so it's reported for the author to address.

### From Python

The same engine is available in-process via the `idiomatic` Python package
(built with [maturin](https://www.maturin.rs); `cd crates/idiomatic-py && maturin develop`):

```python
import idiomatic

idiomatic.lint("if x == None:\n    pass\n", "python")      # [Hit(id='compare-none', start=3, end=12)]
idiomatic.autofix("if x == None:\n    pass\n", "python")   # ('if x is None:\n    pass\n', 1)
idiomatic.render_skill("typescript")                        # SKILL.md text for TypeScript
```

### From Node

The same engine is available to JavaScript/TypeScript via the napi addon
(`npm install @idiomatic/node`):

```js
import { lint, autofix, renderSkill } from '@idiomatic/node';

autofix("if x == None:\n    pass\n", "python");  // { fixed: 'if x is None:\n    pass\n', count: 1 }
renderSkill("typescript");                        // SKILL.md text for TypeScript
```

### As a pre-commit hook

```yaml
# .pre-commit-config.yaml — requires the `idiomatic` binary on PATH (see Install)
repos:
  - repo: https://github.com/joeblackwaslike/idiomatic
    rev: v0.1.0
    hooks:
      - id: idiomatic        # report violations (blocks on error severity)
      # - id: idiomatic-fix  # autofix in place
```

For CI, copy [`.github/workflows/idiomatic.yml`](.github/workflows/idiomatic.yml)
— it installs the prebuilt binary and runs `idiomatic check` on changed files.

## The pack format

A pack is one YAML file: the first document is the manifest, each subsequent
document is one idiom. See [`crates/idiomatic-core/packs/python-core.yaml`](crates/idiomatic-core/packs/python-core.yaml)
for the seed pack and the [design spec](docs/superpowers/specs/2026-06-09-idiomatic-framework-design.md)
§5 for full field semantics.

```yaml
name: python-core
language: python
version: 0.1.0
---
id: compare-none
language: python
title: "Use `is None` instead of `== None`"
why: "None is a singleton; identity comparison is correct and idiomatic."
severity: warn
fix_policy: autofix        # autofix | warn-and-instruct | skill-only
rule:
  pattern: "$X == None"    # passed through to ast-grep
fix: "$X is None"          # required iff fix_policy: autofix
examples:
  bad: "if x == None:\n    pass"
  good: "if x is None:\n    pass"
```

`fix_policy` is the spine: `autofix` rewrites in place, `warn-and-instruct` is the
only policy that interrupts the agent, and `skill-only` carries no detector
(pure prose, taught not enforced).

## Layout

- `crates/idiomatic-core` — pack loader, cascade resolver, ast-grep engine
  adapter, self-test harness.
- `crates/idiomatic-cli` — the `idiomatic` binary (`check`).

## Develop

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## License

MIT
