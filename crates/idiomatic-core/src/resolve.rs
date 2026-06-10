//! Cascade resolution: fold idiom patches across layers by id, field-by-field,
//! tracking which layer set each resolved field (provenance).
use crate::error::ResolveError;
use crate::pack::{Examples, FixPolicy, IdiomPatch, LoadedPack, Severity};
use crate::Layer;
use std::collections::BTreeMap;

/// Which layer last set a given field. Keyed by field name.
pub type Provenance = BTreeMap<String, Layer>;

/// A fully resolved idiom, ready for the engine and renderers.
#[derive(Debug, Clone)]
pub struct Idiom {
    pub id: String,
    pub language: String,
    pub title: String,
    pub why: String,
    pub severity: Severity,
    pub fix_policy: FixPolicy,
    pub rule: Option<serde_yaml_ng::Value>,
    pub fix: Option<String>,
    pub skill_prose: Option<String>,
    pub examples: Option<Examples>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Default)]
pub struct IdiomSet {
    idioms: Vec<Idiom>,
}

impl IdiomSet {
    pub fn get(&self, id: &str) -> Option<&Idiom> {
        self.idioms.iter().find(|i| i.id == id)
    }
    pub fn iter(&self) -> impl Iterator<Item = &Idiom> {
        self.idioms.iter()
    }
    pub fn len(&self) -> usize {
        self.idioms.len()
    }
    pub fn is_empty(&self) -> bool {
        self.idioms.is_empty()
    }
}

/// Accumulator: optional fields + provenance, folded patch by patch.
#[derive(Default)]
struct Acc {
    language: Option<String>,
    title: Option<String>,
    why: Option<String>,
    severity: Option<Severity>,
    fix_policy: Option<FixPolicy>,
    rule: Option<serde_yaml_ng::Value>,
    fix: Option<String>,
    skill_prose: Option<String>,
    examples: Option<Examples>,
    disabled: bool,
    provenance: Provenance,
}

impl Acc {
    fn apply(&mut self, patch: &IdiomPatch, layer: Layer) {
        macro_rules! set {
            ($field:ident, $name:literal) => {
                if let Some(v) = patch.$field.clone() {
                    self.$field = Some(v);
                    self.provenance.insert($name.to_string(), layer);
                }
            };
        }
        set!(language, "language");
        set!(title, "title");
        set!(why, "why");
        set!(severity, "severity");
        set!(fix_policy, "fix_policy");
        set!(rule, "rule");
        set!(fix, "fix");
        set!(skill_prose, "skill_prose");
        set!(examples, "examples");
        if let Some(d) = patch.disabled {
            self.disabled = d;
            self.provenance.insert("disabled".to_string(), layer);
        }
        // NOTE: list-valued fields would append-by-default here (with `replace:
        // true` to discard the inherited list). No list field exists in the v1
        // schema; see plan "spec deltas". Extension point — do not delete.
    }

    fn finalize(self, id: String) -> Result<Idiom, ResolveError> {
        let field = |name| ResolveError::MissingField { id: id.clone(), field: name };
        Ok(Idiom {
            language: self.language.ok_or_else(|| field("language"))?,
            title: self.title.ok_or_else(|| field("title"))?,
            why: self.why.ok_or_else(|| field("why"))?,
            severity: self.severity.unwrap_or(Severity::Error),
            fix_policy: self.fix_policy.ok_or_else(|| field("fix_policy"))?,
            rule: self.rule,
            fix: self.fix,
            skill_prose: self.skill_prose,
            examples: self.examples,
            provenance: self.provenance,
            id,
        })
    }
}

/// Resolve packs (already ordered lowest→highest precedence) into a flat set.
/// Disabled idioms are dropped. Validation invariants are applied here.
pub fn resolve(packs: &[LoadedPack]) -> Result<IdiomSet, ResolveError> {
    // Preserve first-seen id order for deterministic output / golden tests.
    let mut order: Vec<String> = Vec::new();
    let mut accs: BTreeMap<String, Acc> = BTreeMap::new();

    for pack in packs {
        for patch in &pack.idioms {
            let acc = accs.entry(patch.id.clone()).or_insert_with(|| {
                order.push(patch.id.clone());
                Acc::default()
            });
            acc.apply(patch, pack.layer);
        }
    }

    let mut idioms = Vec::new();
    for id in order {
        let acc = accs.remove(&id).expect("id present");
        if acc.disabled {
            continue;
        }
        let idiom = acc.finalize(id)?;
        crate::validate::check_invariants(&idiom)?;
        idioms.push(idiom);
    }
    Ok(IdiomSet { idioms })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{LoadedPack, Severity};
    use crate::Layer;

    fn base_pack() -> LoadedPack {
        LoadedPack::from_yaml_str(
            r#"
name: python-core
language: python
version: 0.1.0
---
id: compare-none
language: python
title: "Use is None"
why: "identity not equality"
severity: error
fix_policy: autofix
rule:
  pattern: "$X == None"
fix: "$X is None"
"#,
            Layer::Base,
        )
        .unwrap()
    }

    fn project_override() -> LoadedPack {
        // Only names `id` + `severity`: must override severity, keep pattern/fix.
        LoadedPack::from_yaml_str(
            "name: proj\nlanguage: python\nversion: 0.0.0\n---\nid: compare-none\nseverity: warn\n",
            Layer::Project,
        )
        .unwrap()
    }

    #[test]
    fn higher_layer_overrides_named_field_only() {
        let set = resolve(&[base_pack(), project_override()]).unwrap();
        let idiom = set.get("compare-none").unwrap();
        assert_eq!(idiom.severity, Severity::Warn); // overridden
        assert_eq!(idiom.fix.as_deref(), Some("$X is None")); // inherited
        assert_eq!(idiom.provenance["severity"], Layer::Project);
        assert_eq!(idiom.provenance["fix"], Layer::Base);
    }

    #[test]
    fn disabled_removes_idiom() {
        let off = LoadedPack::from_yaml_str(
            "name: p\nlanguage: python\nversion: 0\n---\nid: compare-none\ndisabled: true\n",
            Layer::Project,
        )
        .unwrap();
        let set = resolve(&[base_pack(), off]).unwrap();
        assert!(set.get("compare-none").is_none());
    }
}
