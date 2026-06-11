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
    #[error("idiom '{id}': rule failed to compile: {message}")]
    RuleCompile { id: String, message: String },
    #[error("idiom id '{id}' is used for two languages: '{first}' and '{second}' — ids must be unique across languages")]
    IdLanguageConflict { id: String, first: String, second: String },
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("idiom '{id}': failed to build ast-grep rule: {message}")]
    Compile { id: String, message: String },
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
}
