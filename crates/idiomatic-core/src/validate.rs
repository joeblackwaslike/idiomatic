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
        Idiom {
            id: "x".into(),
            language: "python".into(),
            title: "t".into(),
            why: "w".into(),
            severity: Severity::Error,
            fix_policy,
            rule: rule.then(|| serde_yaml_ng::Value::Null),
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
}
