//! Python bindings for `idiomatic` — lint, autofix, and skill generation.
//!
//! **Project-layer discovery** (`./.idiomatic`) is resolved relative to the
//! Python process's current working directory at the time the cascade is loaded.
//! For the module-level convenience functions (`lint`, `autofix`, `render_skill`)
//! the cascade is re-loaded on every call. For hot loops, construct a `Linter`
//! once and reuse it — the cascade is resolved only at construction time.
use idiomatic_core::cascade::load_cascade;
use idiomatic_core::engine::{autofix_source, lang_applies, lint_source, support_lang, CompiledIdiom, SupportLang};
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

/// Compile the idioms for `language` from an already-resolved `IdiomSet`.
fn compile_for(set: &IdiomSet, language: &str) -> PyResult<(SupportLang, Vec<CompiledIdiom>)> {
    let lang = support_lang(language)
        .ok_or_else(|| PyValueError::new_err(format!("unknown language: {language}")))?;
    let compiled: Vec<CompiledIdiom> = set
        .iter()
        .filter(|i| lang_applies(&i.language, lang))
        .filter_map(|i| CompiledIdiom::compile(i).ok())
        .collect();
    Ok((lang, compiled))
}

/// Lint `source` for `language`; return the list of idiom matches.
///
/// The cascade (including `./.idiomatic`) is resolved relative to the process's
/// current working directory on each call. For repeated calls, prefer `Linter`.
#[pyfunction]
fn lint(source: &str, language: &str) -> PyResult<Vec<Hit>> {
    let set = load_cascade().map_err(|e| PyRuntimeError::new_err(format!("cascade error: {e}")))?;
    let (lang, compiled) = compile_for(&set, language)?;
    Ok(lint_source(&compiled, lang, source)
        .into_iter()
        .map(|h| Hit { id: h.id, start: h.start, end: h.end })
        .collect())
}

/// Autofix `source` for `language`; return `(fixed_source, num_fixes_applied)`.
///
/// The cascade (including `./.idiomatic`) is resolved relative to the process's
/// current working directory on each call. For repeated calls, prefer `Linter`.
#[pyfunction]
fn autofix(source: &str, language: &str) -> PyResult<(String, usize)> {
    let set = load_cascade().map_err(|e| PyRuntimeError::new_err(format!("cascade error: {e}")))?;
    let (lang, compiled) = compile_for(&set, language)?;
    Ok(autofix_source(&compiled, lang, source))
}

/// Render the teaching skill (SKILL.md text) for `language` from the cascade.
///
/// The cascade (including `./.idiomatic`) is resolved relative to the process's
/// current working directory on each call. For repeated calls, prefer `Linter`.
#[pyfunction]
fn render_skill(language: &str) -> PyResult<String> {
    support_lang(language)
        .ok_or_else(|| PyValueError::new_err(format!("unknown language: {language}")))?;
    let set =
        load_cascade().map_err(|e| PyRuntimeError::new_err(format!("cascade error: {e}")))?;
    Ok(core_render_skill(&set, language))
}

/// A reusable linter that resolves the idiom cascade once.
///
/// The cascade is read from the built-in packs plus `~/.config/idiomatic` and
/// `./.idiomatic` **relative to the current working directory** at construction
/// time. Construct a new `Linter` if the working directory or those config
/// files change.
#[pyclass]
struct Linter {
    set: IdiomSet,
}

#[pymethods]
impl Linter {
    #[new]
    fn new() -> PyResult<Self> {
        let set = load_cascade()
            .map_err(|e| PyRuntimeError::new_err(format!("cascade error: {e}")))?;
        Ok(Linter { set })
    }

    /// Lint `source` for `language`; returns the list of idiom matches.
    fn lint(&self, source: &str, language: &str) -> PyResult<Vec<Hit>> {
        let (lang, compiled) = compile_for(&self.set, language)?;
        Ok(lint_source(&compiled, lang, source)
            .into_iter()
            .map(|h| Hit { id: h.id, start: h.start, end: h.end })
            .collect())
    }

    /// Autofix `source` for `language`; returns `(fixed_source, num_fixes)`.
    fn autofix(&self, source: &str, language: &str) -> PyResult<(String, usize)> {
        let (lang, compiled) = compile_for(&self.set, language)?;
        Ok(autofix_source(&compiled, lang, source))
    }

    /// Render the teaching skill (SKILL.md text) for `language`.
    fn render_skill(&self, language: &str) -> PyResult<String> {
        support_lang(language)
            .ok_or_else(|| PyValueError::new_err(format!("unknown language: {language}")))?;
        Ok(core_render_skill(&self.set, language))
    }
}

/// The `idiomatic` Python module.
#[pymodule]
fn idiomatic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Hit>()?;
    m.add_class::<Linter>()?;
    m.add_function(wrap_pyfunction!(lint, m)?)?;
    m.add_function(wrap_pyfunction!(autofix, m)?)?;
    m.add_function(wrap_pyfunction!(render_skill, m)?)?;
    Ok(())
}
