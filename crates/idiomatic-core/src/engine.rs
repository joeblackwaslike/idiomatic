//! ast-grep adapter.
pub use ast_grep_language::SupportLang;

use ast_grep_config::{from_yaml_string, GlobalRules, RuleConfig};
use ast_grep_language::LanguageExt;

use crate::error::EngineError;
use crate::pack::FixPolicy;
use crate::resolve::Idiom;

/// Map an idiom `language` string to a SupportLang. Returns None for unknown.
pub fn support_lang(name: &str) -> Option<SupportLang> {
    match name.to_ascii_lowercase().as_str() {
        "python" | "py" => Some(SupportLang::Python),
        "typescript" | "ts" => Some(SupportLang::TypeScript),
        _ => None,
    }
}

/// One idiom compiled into an ast-grep rule, retaining the policy needed to
/// decide whether to autofix or merely report.
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
    /// (id + language + rule + optional fix) and parsing it via
    /// [`from_yaml_string`]. This keeps us off ast-grep's internal struct shapes.
    pub fn compile(idiom: &Idiom) -> Result<Self, EngineError> {
        // Validate the language up front so unsupported languages produce a clear
        // error rather than a deserialization failure from ast-grep.
        support_lang(&idiom.language)
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
        let yaml = serde_yaml_ng::to_string(&serde_yaml_ng::Value::Mapping(doc)).map_err(|e| {
            EngineError::Compile {
                id: idiom.id.clone(),
                message: e.to_string(),
            }
        })?;

        let globals = GlobalRules::default();
        let mut configs: Vec<RuleConfig<SupportLang>> = from_yaml_string(&yaml, &globals)
            .map_err(|e| EngineError::Compile {
                id: idiom.id.clone(),
                message: e.to_string(),
            })?;
        let config = configs.pop().ok_or_else(|| EngineError::Compile {
            id: idiom.id.clone(),
            message: "from_yaml_string produced no rule".into(),
        })?;

        Ok(CompiledIdiom {
            id: idiom.id.clone(),
            fix_policy: idiom.fix_policy,
            config,
        })
    }
}

/// Lint a source string with all compiled idioms; return every hit (no fixing).
pub fn lint_source(idioms: &[CompiledIdiom], lang: SupportLang, source: &str) -> Vec<Hit> {
    let ast = lang.ast_grep(source);
    let mut hits = Vec::new();
    for idiom in idioms {
        for m in ast.root().find_all(&idiom.config.matcher) {
            let range = m.range();
            hits.push(Hit {
                id: idiom.id.clone(),
                start: range.start,
                end: range.end,
            });
        }
    }
    hits
}

/// Apply every `autofix` idiom's rewrite to the source, returning the rewritten
/// text and the count of fixes applied. Non-autofix idioms are ignored here.
pub fn autofix_source(
    idioms: &[CompiledIdiom],
    lang: SupportLang,
    source: &str,
) -> (String, usize) {
    let ast = lang.ast_grep(source);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();

    for idiom in idioms {
        if idiom.fix_policy != FixPolicy::Autofix {
            continue;
        }
        let fixers = idiom.config.get_fixer().unwrap_or_default();
        let Some(fixer) = fixers.into_iter().next() else {
            continue;
        };
        for m in ast.root().find_all(&idiom.config.matcher) {
            let edit = m.replace_by(&fixer);
            let start = edit.position;
            let end = edit.position + edit.deleted_length;
            let text = String::from_utf8_lossy(&edit.inserted_text).into_owned();
            edits.push((start, end, text));
        }
    }

    // Greedy non-overlap selection: edits were collected against original
    // offsets, so right-to-left splicing is only valid for disjoint ranges.
    // When matches overlap/nest, keep the first (leftmost) and skip the rest;
    // a re-lint pass (CLI/hook) catches anything deferred.
    edits.sort_by_key(|e| (e.0, e.1));
    let mut kept: Vec<(usize, usize, String)> = Vec::new();
    let mut last_end = 0;
    for e in edits {
        if e.0 >= last_end {
            last_end = e.1;
            kept.push(e);
        }
    }
    let count = kept.len();
    kept.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = source.to_string();
    for (start, end, text) in kept {
        out.replace_range(start..end, &text);
    }
    (out, count)
}

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

    /// Regression test for overlapping / nested edit ranges.
    ///
    /// idiom A matches `foo($A)` → `FOO($A)` (spans the whole expression)
    /// idiom B matches `$X == None` → `$X is None` (nested inside A's match)
    ///
    /// With greedy leftmost-wins selection, A is kept (starts at 0) and B is
    /// skipped (its range is inside A's range).  count must equal 1 and the
    /// output must equal `"FOO(a == None)\n"` — no bytes dropped or duplicated.
    #[test]
    fn overlapping_edits_are_handled_without_corruption() {
        let rule_a: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("pattern: \"foo($A)\"").unwrap();
        let idiom_a = Idiom {
            id: "wrap-foo".into(),
            language: "python".into(),
            title: "Uppercase FOO".into(),
            why: "test".into(),
            severity: Severity::Warn,
            fix_policy: FixPolicy::Autofix,
            rule: Some(rule_a),
            fix: Some("FOO($A)".into()),
            skill_prose: None,
            examples: None,
            provenance: BTreeMap::new(),
        };

        let compiled = vec![
            CompiledIdiom::compile(&idiom_a).unwrap(),
            CompiledIdiom::compile(&compare_none()).unwrap(),
        ];

        let input = "foo(a == None)\n";
        let (out, count) = autofix_source(&compiled, SupportLang::Python, input);

        // Greedy leftmost-wins: the outer foo(...) edit is kept (starts at 0),
        // the nested `a == None` edit is skipped.
        // count must equal the number of edits actually applied — no phantom counts.
        assert_eq!(count, 1, "count must equal edits actually applied, got {count}");
        assert_eq!(
            out, "FOO(a == None)\n",
            "output must be a clean non-corrupted rewrite, got {out:?}"
        );
    }
}
