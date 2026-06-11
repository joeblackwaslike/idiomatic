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
