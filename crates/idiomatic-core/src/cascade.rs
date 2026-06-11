//! Cascade discovery: assemble the `base → user → project` layer set and resolve
//! it. Shared by the CLI and the Python binding.
use crate::engine::{support_lang, SupportLang};
use crate::error::ResolveError;
use crate::pack::LoadedPack;
use crate::resolve::{resolve, IdiomSet};
use crate::{builtin_packs, Layer};
use std::path::Path;

/// Errors discovering or resolving the cascade.
#[derive(Debug, thiserror::Error)]
pub enum CascadeError {
    #[error("failed to read pack: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Pack(#[from] crate::error::PackError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
}

/// Resolve the full `base → user → project` cascade. `base` = built-in packs,
/// `user` = `~/.config/idiomatic/*.yaml`, `project` = `./.idiomatic/*.yaml`.
pub fn load_cascade() -> Result<IdiomSet, CascadeError> {
    let mut packs: Vec<LoadedPack> = builtin_packs()
        .iter()
        .map(|(_, yaml)| LoadedPack::from_yaml_str(yaml, Layer::Base))
        .collect::<Result<_, _>>()?;

    if let Some(config) = dirs::config_dir() {
        load_dir(&config.join("idiomatic"), Layer::User, &mut packs)?;
    }
    load_dir(Path::new(".idiomatic"), Layer::Project, &mut packs)?;

    Ok(resolve(&packs)?)
}

fn load_dir(dir: &Path, layer: Layer, out: &mut Vec<LoadedPack>) -> Result<(), CascadeError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let yaml = std::fs::read_to_string(&path)?;
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
        "tsx" => support_lang("tsx"),
        _ => None,
    }
}
