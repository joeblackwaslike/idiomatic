//! Pack model and multi-document YAML loader.
use crate::error::PackError;
use crate::Layer;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FixPolicy {
    Autofix,
    WarnAndInstruct,
    SkillOnly,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Examples {
    #[serde(default)]
    pub bad: Option<String>,
    #[serde(default)]
    pub good: Option<String>,
}

/// The first document in a pack file.
#[derive(Debug, Clone, Deserialize)]
pub struct PackManifest {
    pub name: String,
    pub language: String,
    pub version: String,
}

/// One idiom document as authored. Every field except `id` is optional so that a
/// higher cascade layer can override a single field. `rule` is kept as an opaque
/// YAML value and forwarded to ast-grep verbatim.
#[derive(Debug, Clone, Deserialize)]
pub struct IdiomPatch {
    pub id: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub severity: Option<Severity>,
    #[serde(default)]
    pub fix_policy: Option<FixPolicy>,
    #[serde(default)]
    pub rule: Option<serde_yaml_ng::Value>,
    #[serde(default)]
    pub fix: Option<String>,
    #[serde(default)]
    pub skill_prose: Option<String>,
    #[serde(default)]
    pub examples: Option<Examples>,
    #[serde(default)]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct LoadedPack {
    pub manifest: PackManifest,
    pub idioms: Vec<IdiomPatch>,
    pub layer: Layer,
}

impl LoadedPack {
    /// Parse a multi-document pack: first document is the manifest, the rest are
    /// idiom patches. Enforces id-uniqueness within this pack.
    pub fn from_yaml_str(input: &str, layer: Layer) -> Result<Self, PackError> {
        let mut docs = serde_yaml_ng::Deserializer::from_str(input).enumerate();

        let (_, first) = docs.next().ok_or(PackError::Empty)?;
        let manifest = PackManifest::deserialize(first)
            .map_err(|source| PackError::Yaml { index: 0, source })?;

        let mut idioms = Vec::new();
        let mut seen = HashSet::new();
        for (index, doc) in docs {
            let patch = IdiomPatch::deserialize(doc)
                .map_err(|source| PackError::Yaml { index, source })?;
            if !seen.insert(patch.id.clone()) {
                return Err(PackError::DuplicateId(patch.id));
            }
            idioms.push(patch);
        }

        Ok(LoadedPack { manifest, idioms, layer })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PackError;

    const SAMPLE: &str = r#"
name: python-core
language: python
version: 0.1.0
---
id: compare-none
language: python
title: "Use `is None`"
why: "identity, not equality, for None"
severity: warn
fix_policy: autofix
rule:
  pattern: "$X == None"
fix: "$X is None"
examples:
  bad: "if x == None:\n    pass"
  good: "if x is None:\n    pass"
"#;

    #[test]
    fn loads_manifest_and_one_idiom() {
        let pack = LoadedPack::from_yaml_str(SAMPLE, crate::Layer::Base).unwrap();
        assert_eq!(pack.manifest.name, "python-core");
        assert_eq!(pack.idioms.len(), 1);
        let idiom = &pack.idioms[0];
        assert_eq!(idiom.id, "compare-none");
        assert_eq!(idiom.fix_policy, Some(FixPolicy::Autofix));
        assert_eq!(idiom.fix.as_deref(), Some("$X is None"));
    }

    #[test]
    fn rejects_duplicate_ids() {
        let dup = format!("{SAMPLE}---\nid: compare-none\nfix_policy: warn-and-instruct\n");
        let err = LoadedPack::from_yaml_str(&dup, crate::Layer::Base).unwrap_err();
        assert!(matches!(err, PackError::DuplicateId(_)));
    }
}
