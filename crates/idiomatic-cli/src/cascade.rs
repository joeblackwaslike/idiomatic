//! Cascade discovery shared by the `check`, `hook`, and `skill-gen` commands.
use anyhow::Result;
use idiomatic_core::engine::{support_lang, SupportLang};
use idiomatic_core::pack::LoadedPack;
use idiomatic_core::resolve::{resolve, IdiomSet};
use idiomatic_core::{builtin_packs, Layer};
use std::fs;
use std::path::Path;

/// Resolve the full `base → user → project` cascade.
pub fn load_cascade() -> Result<IdiomSet> {
    let mut packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base))
        .collect::<std::result::Result<_, _>>()?;

    if let Some(config) = dirs::config_dir() {
        load_dir(&config.join("idiomatic"), Layer::User, &mut packs)?;
    }
    load_dir(Path::new(".idiomatic"), Layer::Project, &mut packs)?;

    Ok(resolve(&packs)?)
}

fn load_dir(dir: &Path, layer: Layer, out: &mut Vec<LoadedPack>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let yaml = fs::read_to_string(&path)?;
            out.push(LoadedPack::from_yaml_str(&yaml, layer)?);
        }
    }
    Ok(())
}

/// Map a file path's extension to a supported language, or None.
pub fn ext_lang(path: &Path) -> Option<SupportLang> {
    match path.extension().and_then(|e| e.to_str())? {
        "py" => support_lang("python"),
        "ts" => support_lang("typescript"),
        _ => None,
    }
}
