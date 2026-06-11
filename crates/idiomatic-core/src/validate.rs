//! Load-time validation of resolved idioms.
use crate::engine::support_lang;
use crate::error::ResolveError;
use crate::pack::FixPolicy;
use crate::resolve::Idiom;

/// Enforce spec §5 invariants on a resolved idiom. Fail loud at load, never at
/// gate time.
pub fn check_invariants(idiom: &Idiom) -> Result<(), ResolveError> {
    let id = || idiom.id.clone();

    // Known language (parses into a SupportLang).
    if support_lang(&idiom.language).is_none() {
        return Err(ResolveError::UnknownLanguage { id: id(), lang: idiom.language.clone() });
    }

    match idiom.fix_policy {
        FixPolicy::Autofix if idiom.fix.is_none() => {
            return Err(ResolveError::AutofixWithoutFix { id: id() });
        }
        FixPolicy::SkillOnly if idiom.rule.is_some() => {
            return Err(ResolveError::SkillOnlyWithRule { id: id() });
        }
        _ => {}
    }

    // §10: fail loud at load if the rule doesn't compile. Skill-only idioms have
    // no rule, so they're naturally excluded.
    if idiom.rule.is_some() {
        crate::engine::CompiledIdiom::compile(idiom).map_err(|e| ResolveError::RuleCompile {
            id: id(),
            message: e.to_string(),
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ResolveError;
    use crate::pack::{FixPolicy, Severity};
    use crate::resolve::Idiom;
    use std::collections::BTreeMap;

    fn idiom(fix_policy: FixPolicy, rule: bool, fix: Option<&str>) -> Idiom {
        // Use a real, compilable rule when `rule` is true so that the
        // RuleCompile check doesn't trip on a sentinel Null value.
        let rule_val = rule.then(|| {
            serde_yaml_ng::from_str::<serde_yaml_ng::Value>("pattern: \"$X == None\"").unwrap()
        });
        Idiom {
            id: "x".into(),
            language: "python".into(),
            title: "t".into(),
            why: "w".into(),
            severity: Severity::Error,
            fix_policy,
            rule: rule_val,
            fix: fix.map(String::from),
            skill_prose: None,
            examples: None,
            provenance: BTreeMap::new(),
        }
    }

    #[test]
    fn autofix_requires_fix() {
        let err = check_invariants(&idiom(FixPolicy::Autofix, true, None)).unwrap_err();
        assert!(matches!(err, ResolveError::AutofixWithoutFix { .. }));
    }

    #[test]
    fn skill_only_rejects_rule() {
        let err = check_invariants(&idiom(FixPolicy::SkillOnly, true, None)).unwrap_err();
        assert!(matches!(
            err,
            ResolveError::SkillOnlyWithRule { .. } | ResolveError::RuleWithSkillOnly { .. }
        ));
    }

    #[test]
    fn unknown_language_rejected() {
        let mut i = idiom(FixPolicy::WarnAndInstruct, true, None);
        i.language = "cobol".into();
        let err = check_invariants(&i).unwrap_err();
        assert!(matches!(err, ResolveError::UnknownLanguage { .. }));
    }

    #[test]
    fn valid_autofix_passes() {
        assert!(check_invariants(&idiom(FixPolicy::Autofix, true, Some("y"))).is_ok());
    }

    /// An idiom with a rule that doesn't compile should fail with RuleCompile.
    #[test]
    fn bad_rule_returns_rule_compile_error() {
        // "($" is an unbalanced pattern that ast-grep rejects.
        let rule: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("pattern: \"($\"").unwrap();
        let i = Idiom {
            id: "bad-rule".into(),
            language: "python".into(),
            title: "t".into(),
            why: "w".into(),
            severity: Severity::Error,
            fix_policy: FixPolicy::WarnAndInstruct,
            rule: Some(rule),
            fix: None,
            skill_prose: None,
            examples: None,
            provenance: BTreeMap::new(),
        };
        let err = check_invariants(&i).unwrap_err();
        assert!(matches!(err, ResolveError::RuleCompile { .. }));
    }

    /// An idiom with a syntactically valid rule should still pass.
    #[test]
    fn valid_rule_passes_invariants() {
        let rule: serde_yaml_ng::Value =
            serde_yaml_ng::from_str("pattern: \"$X == None\"").unwrap();
        let i = Idiom {
            id: "good-rule".into(),
            language: "python".into(),
            title: "t".into(),
            why: "w".into(),
            severity: Severity::Error,
            fix_policy: FixPolicy::WarnAndInstruct,
            rule: Some(rule),
            fix: None,
            skill_prose: None,
            examples: None,
            provenance: BTreeMap::new(),
        };
        assert!(check_invariants(&i).is_ok());
    }
}
