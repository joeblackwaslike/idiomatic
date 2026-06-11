//! Node.js bindings for `idiomatic` — lint, autofix, and skill generation.
#![deny(clippy::all)]

use idiomatic_core::cascade::load_cascade;
use idiomatic_core::engine::{
    autofix_source, lint_source, support_lang, CompiledIdiom, SupportLang,
};
use idiomatic_core::render::render_skill as core_render_skill;
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// A single idiom match: the idiom `id` and the byte range `[start, end)`.
#[napi(object)]
pub struct Hit {
    pub id: String,
    pub start: u32,
    pub end: u32,
}

/// Result of an autofix pass.
#[napi(object)]
pub struct AutofixResult {
    pub fixed: String,
    pub count: u32,
}

/// Resolve the cascade and compile the idioms for `language`.
fn prepare(language: &str) -> Result<(SupportLang, Vec<CompiledIdiom>)> {
    let lang = support_lang(language).ok_or_else(|| {
        Error::new(
            Status::InvalidArg,
            format!("unknown language: {language}"),
        )
    })?;
    let set = load_cascade().map_err(|e| {
        Error::new(
            Status::GenericFailure,
            format!("cascade error: {e}"),
        )
    })?;
    let compiled: Vec<CompiledIdiom> = set
        .iter()
        .filter(|i| support_lang(&i.language) == Some(lang))
        .filter_map(|i| CompiledIdiom::compile(i).ok())
        .collect();
    Ok((lang, compiled))
}

/// Lint `source` for `language`; return the list of idiom matches.
#[napi]
pub fn lint(source: String, language: String) -> Result<Vec<Hit>> {
    let (lang, compiled) = prepare(&language)?;
    Ok(lint_source(&compiled, lang, &source)
        .into_iter()
        .map(|h| Hit {
            id: h.id,
            start: h.start as u32,
            end: h.end as u32,
        })
        .collect())
}

/// Autofix `source` for `language`; return the fixed text and fix count.
#[napi]
pub fn autofix(source: String, language: String) -> Result<AutofixResult> {
    let (lang, compiled) = prepare(&language)?;
    let (fixed, count) = autofix_source(&compiled, lang, &source);
    Ok(AutofixResult {
        fixed,
        count: count as u32,
    })
}

/// Render the teaching skill (SKILL.md text) for `language`.
#[napi]
pub fn render_skill(language: String) -> Result<String> {
    support_lang(&language).ok_or_else(|| {
        Error::new(
            Status::InvalidArg,
            format!("unknown language: {language}"),
        )
    })?;
    let set = load_cascade().map_err(|e| {
        Error::new(
            Status::GenericFailure,
            format!("cascade error: {e}"),
        )
    })?;
    Ok(core_render_skill(&set, &language))
}
