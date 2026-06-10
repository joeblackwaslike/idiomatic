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
