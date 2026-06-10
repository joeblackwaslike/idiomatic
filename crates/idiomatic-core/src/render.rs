//! Renderers: idioms → agent-facing diagnostics and teaching skills.

use crate::resolve::Idiom;

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
