//! Renderers: idioms → agent-facing diagnostics and teaching skills.

use crate::engine::support_lang;
use crate::resolve::{Idiom, IdiomSet};

/// Render a violation as an agent-facing fix-it instruction. The "idiomatic
/// shape" is the idiom's `fix` if present, else its `examples.good`.
pub fn render_diagnostic(idiom: &Idiom) -> String {
    let mut s = format!("[{}] {}\n  why: {}", idiom.id, idiom.title, idiom.why);
    let shape = idiom
        .fix
        .clone()
        .or_else(|| idiom.examples.as_ref().and_then(|e| e.good.clone()));
    if let Some(shape) = shape {
        // Indent multi-line shapes under the `prefer:` label.
        let indented = shape.replace('\n', "\n          ");
        s.push_str(&format!("\n  prefer: {indented}"));
    }
    s
}

/// Render the resolved idiom set for one language as a Claude Code skill
/// (`SKILL.md` content). This is a build artifact — never hand-edited.
pub fn render_skill(set: &IdiomSet, language: &str) -> String {
    let target = support_lang(language);
    let idioms: Vec<&Idiom> = set
        .iter()
        .filter(|i| target.is_some() && support_lang(&i.language) == target)
        .collect();

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("name: idiomatic-{language}\n"));
    out.push_str(&format!(
        "description: {language} idioms enforced by idiomatic — read before writing {language} so the in-loop gate has nothing to fix.\n"
    ));
    out.push_str("---\n\n");
    out.push_str(&format!("# Idiomatic {}\n\n", capitalize(language)));
    out.push_str(&format!(
        "{} idioms. Write code that follows these the first time.\n",
        idioms.len()
    ));

    for idiom in &idioms {
        out.push_str(&format!("\n## {}\n\n", idiom.title));
        let prose = idiom.skill_prose.as_deref().unwrap_or(&idiom.why);
        out.push_str(prose.trim_end());
        out.push('\n');
        if let Some(ex) = &idiom.examples {
            if ex.bad.is_some() || ex.good.is_some() {
                out.push_str(&format!("\n```{language}\n"));
                if let Some(bad) = &ex.bad {
                    out.push_str(&format!("# Avoid:\n{}\n", bad.trim_end()));
                }
                if let Some(good) = &ex.good {
                    out.push_str(&format!("# Prefer:\n{}\n", good.trim_end()));
                }
                out.push_str("```\n");
            }
        }
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{Examples, FixPolicy, Severity};
    use crate::resolve::Idiom;
    use std::collections::BTreeMap;

    fn warn_idiom() -> Idiom {
        Idiom {
            id: "print-debugging".into(),
            language: "python".into(),
            title: "Use logging instead of print".into(),
            why: "print can't be leveled or routed".into(),
            severity: Severity::Info,
            fix_policy: FixPolicy::WarnAndInstruct,
            rule: None,
            fix: None,
            skill_prose: None,
            examples: Some(Examples {
                bad: Some("print(x)".into()),
                good: Some("logger.debug(x)".into()),
            }),
            provenance: BTreeMap::new(),
        }
    }

    #[test]
    fn diagnostic_includes_id_title_why_and_shape() {
        let s = render_diagnostic(&warn_idiom());
        assert!(s.contains("[print-debugging]"));
        assert!(s.contains("Use logging instead of print"));
        assert!(s.contains("print can't be leveled or routed"));
        // shape falls back to examples.good when there's no `fix`
        assert!(s.contains("logger.debug(x)"));
    }
}
