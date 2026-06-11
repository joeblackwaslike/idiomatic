# `idiomatic` PyO3 Binding Implementation Plan (Build Order §11, Step 6)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Python extension module (`import idiomatic`) that exposes the core's lint / autofix / skill-gen to Python in-process, built and packaged with maturin, following the Ruff / pydantic-core model.

**Architecture:** Promote cascade discovery from the CLI into `idiomatic-core` so both the CLI and a new `crates/idiomatic-py` crate share it. The Python crate is a thin PyO3 layer: three module functions (`lint`, `autofix`, `render_skill`) plus a small read-only `Hit` class, all delegating to existing core functions. The `pyo3/extension-module` feature is gated behind a crate feature so `cargo test/clippy --workspace` keeps working (links local libpython) while maturin builds portable abi3 wheels with it on.

**Tech Stack:** PyO3 (current 0.2x, Bound API, abi3), maturin (≥1.7), `dirs` (now in core), existing `idiomatic-core`; Python toolchain via `uv`.

**Scope:** Spec §11 step 6 (PyO3 binding + maturin packaging). Out of scope (follow-on): Node binding + CI wheel-building/publish (step 8 — this plan builds wheels locally and tests them, but wires no release CI), and exposing the live hook/telemetry to Python (the binding is for lint/autofix/skill-gen; the hook stays a Rust binary).

---

## Context

Steps 1–5 + 7 (on `main`) give us a language-generic core (`idiomatic-core`) and an `idiomatic` CLI for Python + TypeScript. This phase adds the Python binding so Python tooling (a pre-commit shim, a linter integration, notebooks) can call idiomatic in-process instead of shelling out — the Ruff/pydantic-core pattern the spec cites (§3.6).

Decisions (via AskUserQuestion): **API surface** = functional + light result objects (`lint`/`autofix`/`render_skill` + a `Hit` class); **cascade source** = promote discovery into `idiomatic-core` and auto-discover (builtin + `~/.config/idiomatic` + `./.idiomatic`) like the CLI.

**Confirmed firsthand (PyO3 docs, current `main`):**
- Module: `#[pymodule] fn idiomatic(m: &Bound<'_, PyModule>) -> PyResult<()>` + `m.add_function(wrap_pyfunction!(f, m)?)?` + `m.add_class::<Hit>()?`. The `#[pymodule]` fn name MUST equal `[lib] name` (the import name).
- Functions: `#[pyfunction] fn f(...) -> PyResult<T>`; Rust `String`→`str`, `(String, usize)`→`tuple`, `Vec<Hit>`→`list[Hit]`.
- Errors: `pyo3::exceptions::PyValueError::new_err(msg)` / `PyRuntimeError::new_err(msg)`.
- Cargo: `crate-type = ["cdylib", "rlib"]`; `cdylib` for the Python `.so`, `rlib` so `cargo test/clippy --workspace` can still build it.

**The load-bearing packaging gotcha (handle exactly as below):** PyO3's `extension-module` feature tells the linker *not* to link libpython (needed for portable wheels) — but with it always on, `cargo build`/`cargo test` of the crate fail to link on most platforms. The documented fix is to put `extension-module` behind a *crate* feature that's **off by default** and have **maturin turn it on**. Then:
- `cargo clippy/test --workspace` builds the crate *without* `extension-module` → links local libpython (PyO3's build script locates it on macOS/Linux) → green.
- `maturin develop/build` enables the feature → no libpython link → portable abi3 wheel.

This keeps our existing `cargo clippy --workspace --all-targets -- -D warnings` gate green while still producing real wheels.

**Current code this builds on:**
- `idiomatic_core::engine`: `support_lang(&str) -> Option<SupportLang>`, `CompiledIdiom::compile(&Idiom)`, `lint_source(&[CompiledIdiom], SupportLang, &str) -> Vec<Hit>` (`Hit { id, start, end }`), `autofix_source(&[CompiledIdiom], SupportLang, &str) -> (String, usize)`.
- `idiomatic_core::resolve::{IdiomSet, Idiom}`, `render::render_skill(&IdiomSet, &str) -> String`.
- CLI `crates/idiomatic-cli/src/cascade.rs` holds `load_cascade()`, `ext_lang()`, private `load_dir()` — Task 1 moves these into core.

---

## File Structure

```
crates/idiomatic-core/
  src/cascade.rs        # NEW (moved from CLI): load_cascade(), ext_lang(), load_dir()
  src/lib.rs            # MODIFY: `pub mod cascade;`
  Cargo.toml            # MODIFY: add `dirs`
crates/idiomatic-cli/
  src/cascade.rs        # DELETE (moved to core)
  src/{check,hook,skillgen}.rs, src/main.rs  # MODIFY: import from idiomatic_core::cascade
crates/idiomatic-py/    # NEW crate
  Cargo.toml            # cdylib+rlib, pyo3 (abi3) + extension-module feature
  pyproject.toml        # maturin build backend
  src/lib.rs            # #[pymodule] idiomatic: Hit, lint, autofix, render_skill
  tests/test_idiomatic.py   # pytest
  python/idiomatic/__init__.pyi  # type stub (optional but cheap)
Cargo.toml              # MODIFY: workspace members + pyo3/dirs in workspace.deps
```

---

### Task 1: Promote cascade discovery into `idiomatic-core`

**Files:**
- Modify: root `Cargo.toml`, `crates/idiomatic-core/Cargo.toml`, `crates/idiomatic-core/src/lib.rs`
- Create: `crates/idiomatic-core/src/cascade.rs`
- Delete: `crates/idiomatic-cli/src/cascade.rs`
- Modify: `crates/idiomatic-cli/src/{main.rs,check.rs,hook.rs,skillgen.rs}`

- [ ] **Step 1: Add `dirs` to core.** In root `Cargo.toml`, `dirs` is already in `[workspace.dependencies]` (the CLI uses it). In `crates/idiomatic-core/Cargo.toml` under `[dependencies]` add:

```toml
dirs.workspace = true
```

- [ ] **Step 2: Create `crates/idiomatic-core/src/cascade.rs`** with the discovery logic (moved from the CLI, now using only `crate::` paths):

```rust
//! Cascade discovery: assemble the `base → user → project` layer set and resolve
//! it. Shared by the CLI and the Python binding.
use crate::engine::{support_lang, SupportLang};
use crate::error::ResolveError;
use crate::pack::LoadedPack;
use crate::resolve::{resolve, IdiomSet};
use crate::{builtin_packs, Layer};
use std::path::Path;

/// Errors discovering or resolving the cascade.
#[derive(Debug, thiserror::Error)]
pub enum CascadeError {
    #[error("failed to read pack: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Pack(#[from] crate::error::PackError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
}

/// Resolve the full `base → user → project` cascade. `base` = built-in packs,
/// `user` = `~/.config/idiomatic/*.yaml`, `project` = `./.idiomatic/*.yaml`.
pub fn load_cascade() -> Result<IdiomSet, CascadeError> {
    let mut packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base))
        .collect::<Result<_, _>>()?;

    if let Some(config) = dirs::config_dir() {
        load_dir(&config.join("idiomatic"), Layer::User, &mut packs)?;
    }
    load_dir(Path::new(".idiomatic"), Layer::Project, &mut packs)?;

    Ok(resolve(&packs)?)
}

fn load_dir(dir: &Path, layer: Layer, out: &mut Vec<LoadedPack>) -> Result<(), CascadeError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let yaml = std::fs::read_to_string(&path)?;
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

Note: this introduces `CascadeError` (a typed error) instead of the CLI's previous `anyhow::Result`, so core stays anyhow-free. `ResolveError` must be re-exported or reachable — it already lives in `crate::error`/`crate::resolve`; import it from wherever it is defined (check `crate::error::ResolveError` vs `crate::resolve::ResolveError` and use the correct path).

- [ ] **Step 3: Declare the module** — add to `crates/idiomatic-core/src/lib.rs`:

```rust
pub mod cascade;
```

- [ ] **Step 4: Delete the CLI's cascade module** — remove `crates/idiomatic-cli/src/cascade.rs` and its `mod cascade;` line in `main.rs`.

- [ ] **Step 5: Update CLI imports** — in `check.rs`, `hook.rs`, and `skillgen.rs`, change `use crate::cascade::{...}` to `use idiomatic_core::cascade::{...}`. The CLI functions used `anyhow::Result` and `?`-converted the old cascade errors; `CascadeError` implements `std::error::Error`, so `?` into `anyhow::Result` still works. Verify each file compiles.

- [ ] **Step 6: Verify the refactor is behavior-preserving**

Run: `cargo test --workspace`
Expected: PASS — all 29 tests still green (the CLI integration tests prove discovery still works).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/idiomatic-core/ crates/idiomatic-cli/
git commit -m "refactor: promote cascade discovery from cli into core"
```

---

### Task 2: Scaffold the `idiomatic-py` crate and PyO3 module

**Files:**
- Modify: root `Cargo.toml`
- Create: `crates/idiomatic-py/Cargo.toml`, `crates/idiomatic-py/pyproject.toml`, `crates/idiomatic-py/src/lib.rs`

- [ ] **Step 1: Pin the current PyO3 version** — from the repo root run:

```bash
cargo add pyo3 --dry-run --features abi3-py39
```

Note the resolved version (e.g. `0.25.x`). Use that exact `0.<minor>` in the next steps. Everything below uses the Bound API, which is stable from 0.21 onward, so any current 0.2x works.

- [ ] **Step 2: Add to root `Cargo.toml`** — add the crate to `members` and pyo3 to `[workspace.dependencies]`:

```toml
[workspace]
resolver = "2"
members = ["crates/idiomatic-core", "crates/idiomatic-cli", "crates/idiomatic-py"]
```

```toml
# under [workspace.dependencies], using the version resolved in Step 1:
pyo3 = { version = "0.25", features = ["abi3-py39"] }
```

- [ ] **Step 3: Create `crates/idiomatic-py/Cargo.toml`:**

```toml
[package]
name = "idiomatic-py"
edition.workspace = true
license.workspace = true
version.workspace = true

[lib]
name = "idiomatic"          # = the Python import name and the #[pymodule] fn name
crate-type = ["cdylib", "rlib"]

[dependencies]
idiomatic-core = { path = "../idiomatic-core" }
pyo3.workspace = true

[features]
# Off by default so `cargo test/clippy --workspace` links local libpython and
# stays green. maturin turns this on to build a portable abi3 wheel.
extension-module = ["pyo3/extension-module"]
```

- [ ] **Step 4: Create `crates/idiomatic-py/pyproject.toml`:**

```toml
[build-system]
requires = ["maturin>=1.7,<2"]
build-backend = "maturin"

[project]
name = "idiomatic"
description = "Idiom enforcement for Python and TypeScript — lint, autofix, and skill generation."
requires-python = ">=3.9"
classifiers = ["Programming Language :: Rust", "Programming Language :: Python :: 3"]
dynamic = ["version"]

[tool.maturin]
features = ["pyo3/extension-module"]
module-name = "idiomatic"
```

- [ ] **Step 5: Create `crates/idiomatic-py/src/lib.rs`:**

```rust
//! Python bindings for `idiomatic` — lint, autofix, and skill generation.
use idiomatic_core::cascade::load_cascade;
use idiomatic_core::engine::{autofix_source, lint_source, support_lang, CompiledIdiom, SupportLang};
use idiomatic_core::render::render_skill as core_render_skill;
use idiomatic_core::resolve::IdiomSet;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

/// A single idiom match: the idiom `id` and the byte range `[start, end)` in the
/// source it matched.
#[pyclass(frozen)]
struct Hit {
    #[pyo3(get)]
    id: String,
    #[pyo3(get)]
    start: usize,
    #[pyo3(get)]
    end: usize,
}

#[pymethods]
impl Hit {
    fn __repr__(&self) -> String {
        format!("Hit(id='{}', start={}, end={})", self.id, self.start, self.end)
    }
}

/// Resolve the cascade and compile the idioms for `language`. Shared by lint/autofix.
fn prepare(language: &str) -> PyResult<(IdiomSet, SupportLang, Vec<CompiledIdiom>)> {
    let lang = support_lang(language)
        .ok_or_else(|| PyValueError::new_err(format!("unknown language: {language}")))?;
    let set =
        load_cascade().map_err(|e| PyRuntimeError::new_err(format!("cascade error: {e}")))?;
    let compiled: Vec<CompiledIdiom> = set
        .iter()
        .filter(|i| support_lang(&i.language) == Some(lang))
        .filter_map(|i| CompiledIdiom::compile(i).ok())
        .collect();
    Ok((set, lang, compiled))
}

/// Lint `source` for `language`; return the list of idiom matches.
#[pyfunction]
fn lint(source: &str, language: &str) -> PyResult<Vec<Hit>> {
    let (_set, lang, compiled) = prepare(language)?;
    Ok(lint_source(&compiled, lang, source)
        .into_iter()
        .map(|h| Hit { id: h.id, start: h.start, end: h.end })
        .collect())
}

/// Autofix `source` for `language`; return `(fixed_source, num_fixes_applied)`.
#[pyfunction]
fn autofix(source: &str, language: &str) -> PyResult<(String, usize)> {
    let (_set, lang, compiled) = prepare(language)?;
    Ok(autofix_source(&compiled, lang, source))
}

/// Render the teaching skill (SKILL.md text) for `language` from the cascade.
#[pyfunction]
fn render_skill(language: &str) -> PyResult<String> {
    let set =
        load_cascade().map_err(|e| PyRuntimeError::new_err(format!("cascade error: {e}")))?;
    Ok(core_render_skill(&set, language))
}

/// The `idiomatic` Python module.
#[pymodule]
fn idiomatic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Hit>()?;
    m.add_function(wrap_pyfunction!(lint, m)?)?;
    m.add_function(wrap_pyfunction!(autofix, m)?)?;
    m.add_function(wrap_pyfunction!(render_skill, m)?)?;
    Ok(())
}
```

- [ ] **Step 6: Verify it builds and the workspace gate stays green**

Run: `cargo build -p idiomatic-py`
Expected: PASS — compiles the cdylib+rlib (linking local libpython, since `extension-module` is off by default). If linking fails on macOS with `Undefined symbols ... _Py...`, PyO3's build script couldn't find a Python lib — set `PYO3_PYTHON=$(which python3)` and retry; do NOT add `extension-module` to the default features to "fix" it.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

Run: `cargo test --workspace`
Expected: PASS — 29 Rust tests (the py crate has no Rust `#[test]`s; its tests are Python, added next).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/idiomatic-py/
git commit -m "feat(py): pyo3 module exposing lint, autofix, render_skill"
```

---

### Task 3: Build the wheel with maturin and test from Python

**Files:**
- Create: `crates/idiomatic-py/tests/test_idiomatic.py`

- [ ] **Step 1: Write the pytest suite** `crates/idiomatic-py/tests/test_idiomatic.py`:

```python
import idiomatic
import pytest


def test_lint_reports_compare_none():
    hits = idiomatic.lint("if x == None:\n    pass\n", "python")
    assert any(h.id == "compare-none" for h in hits)
    h = next(h for h in hits if h.id == "compare-none")
    assert h.start < h.end  # a real byte range


def test_autofix_rewrites_in_place():
    fixed, n = idiomatic.autofix("if x == None:\n    pass\n", "python")
    assert fixed == "if x is None:\n    pass\n"
    assert n == 1


def test_autofix_leaves_good_code_untouched():
    fixed, n = idiomatic.autofix("if x is None:\n    pass\n", "python")
    assert n == 0
    assert fixed == "if x is None:\n    pass\n"


def test_render_skill_python():
    skill = idiomatic.render_skill("python")
    assert "name: idiomatic-python" in skill
    assert "Use `is None`" in skill


def test_typescript_supported():
    fixed, _ = idiomatic.autofix("const x = a;\n", "typescript")  # no-op but valid lang
    assert fixed == "const x = a;\n"
    skill = idiomatic.render_skill("typescript")
    assert "name: idiomatic-typescript" in skill


def test_unknown_language_raises():
    with pytest.raises(ValueError):
        idiomatic.lint("x", "cobol")
    with pytest.raises(ValueError):
        idiomatic.autofix("x", "cobol")
```

- [ ] **Step 2: Build into a venv and run the tests** (from `crates/idiomatic-py/`)

```bash
cd crates/idiomatic-py
uv venv
uv pip install maturin pytest
uv run maturin develop --features pyo3/extension-module
uv run pytest -q
```

Expected: maturin compiles the extension (with `extension-module` on), installs `idiomatic` into the venv, and all 6 pytest cases pass: lint finds `compare-none`, autofix round-trips `== None`→`is None`, good code untouched, the Python and TypeScript skills render, and unknown languages raise `ValueError`.

> If `maturin develop` reports it can't find a virtualenv, ensure the `uv venv` is activated or pass `--uv` (`uv run maturin develop --uv --features pyo3/extension-module`). If `import idiomatic` shadows the source dir, run pytest from a directory other than one containing a local `idiomatic/` package, or rely on the installed wheel (maturin develop installs it site-packages-wide).

- [ ] **Step 3: Add a `.gitignore` entry** for the Python build artifacts — append to the repo root `.gitignore`:

```
# python / maturin
crates/idiomatic-py/.venv/
**/__pycache__/
*.so
```

- [ ] **Step 4: Commit**

```bash
git add crates/idiomatic-py/tests/ .gitignore
git commit -m "test(py): pytest suite for the python binding"
```

---

### Task 4: End-to-end verification + docs

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Full Rust suite + lint**

Run: `cargo test --workspace`
Expected: PASS — 29 Rust tests, no regressions.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 2: Rebuild + retest the Python wheel** (clean room)

```bash
cd crates/idiomatic-py
uv run maturin develop --features pyo3/extension-module
uv run pytest -q
cd ../..
```

Expected: 6/6 pytest pass.

- [ ] **Step 3: Manual Python smoke test**

```bash
cd crates/idiomatic-py
uv run python -c "import idiomatic; print(idiomatic.autofix('if x == None:\n    pass\n', 'python'))"
cd ../..
```

Expected: prints `('if x is None:\n    pass\n', 1)`.

- [ ] **Step 4: Update `README.md`** — add the Python binding:
  - Under "What works today", note the `idiomatic` Python package (PyO3) exposing `lint(source, language)`, `autofix(source, language)`, `render_skill(language)`, built with maturin.
  - Add a short Python usage block:
    ```python
    import idiomatic
    fixed, n = idiomatic.autofix("if x == None:\n    pass\n", "python")  # ('if x is None:\n    pass\n', 1)
    ```
  - Note build: `cd crates/idiomatic-py && maturin develop` (or `pip install` once wheels are published — step 8).
  - Update the Status line: core, agent loop, TypeScript, and the Python binding all ship; remaining follow-on is the Node binding + CI wheel publishing (step 8).

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: README for the python (pyo3) binding"
```

---

## Verification

The phase is proven when:

1. `cargo test --workspace` (29) and `cargo clippy --workspace --all-targets -- -D warnings` are green — confirming the cascade-promotion refactor is behavior-preserving and the py crate builds within the workspace gate (without `extension-module`).
2. `maturin develop --features pyo3/extension-module` + `pytest` pass all 6 Python cases — lint/autofix/render_skill work in-process from Python for both Python and TypeScript, and unknown languages raise `ValueError`.
3. The manual `python -c` smoke test returns the autofixed tuple.

This delivers spec §11 step 6. The binding reuses the exact engine path the CLI uses (via the now-shared `idiomatic_core::cascade`), so Python consumers get identical results to `idiomatic check`. Follow-on: Node binding + pre-commit/CI recipe and wheel publishing (step 8), and exposing the live-hook/telemetry surface to Python if a Python-native gate is ever wanted.

## Self-Review Notes

- **Spec coverage:** §11 step 6 (PyO3 + maturin) → Tasks 2–3. §3.6 (Rust core + bindings, Ruff/pydantic-core model) → the thin-binding design. The cascade-promotion (Task 1) is the enabling refactor so the binding and CLI share one discovery path (no drift between `import idiomatic` and `idiomatic check`).
- **The one packaging risk** (the `extension-module`/libpython linking tension) is handled by the off-by-default crate feature — `cargo` links local Python, maturin builds portable wheels. This keeps the existing clippy/test gate green, which is why Task 2 Step 6 explicitly verifies the workspace build before any Python tooling is involved.
- **Error mapping:** unknown language → `ValueError` (Pythonic, testable); cascade IO/resolve failure → `RuntimeError`. Both asserted in pytest.
- **Type consistency:** `prepare(language)` centralizes cascade+compile so `lint` and `autofix` can't drift; `Hit`'s fields (`id`/`start`/`end`) mirror the core `Hit`; `render_skill` reuses the core renderer verbatim.
- **No behavior change to existing crates** beyond the import-path move in Task 1 — the 29 existing tests are the guard.
