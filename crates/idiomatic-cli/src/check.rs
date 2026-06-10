//! `idiomatic check [--fix] <paths...>`
use anyhow::Result;
use idiomatic_core::engine::{
    autofix_source, lint_source, support_lang, CompiledIdiom, Hit, SupportLang,
};
use idiomatic_core::pack::{FixPolicy, LoadedPack, Severity};
use idiomatic_core::resolve::{resolve, IdiomSet};
use idiomatic_core::{builtin_packs, Layer};
use std::fs;
use std::path::{Path, PathBuf};

pub struct CheckOutcome {
    pub had_error_severity: bool,
}

pub fn run(paths: &[PathBuf], fix: bool) -> Result<CheckOutcome> {
    let set = load_cascade()?;
    let mut had_error_severity = false;

    for path in paths {
        let Some(lang) = ext_lang(path) else {
            continue;
        };
        let source = fs::read_to_string(path)?;

        // Compile only idioms whose language matches this file.
        let compiled: Vec<CompiledIdiom> = set
            .iter()
            .filter(|i| support_lang(&i.language) == Some(lang))
            .filter_map(|i| CompiledIdiom::compile(i).ok())
            .collect();

        let text = if fix {
            let (fixed, n) = autofix_source(&compiled, lang, &source);
            if n > 0 {
                fs::write(path, &fixed)?;
                println!("applied {n} idiom fixes to {}", path.display());
            }
            fixed
        } else {
            source
        };

        // Re-lint the (possibly-fixed) text and report what remains.
        let hits = lint_source(&compiled, lang, &text);
        report(&set, hits, path, &mut had_error_severity);
    }
    Ok(CheckOutcome { had_error_severity })
}

fn report(set: &IdiomSet, hits: Vec<Hit>, path: &Path, had_error: &mut bool) {
    for hit in hits {
        let Some(idiom) = set.get(&hit.id) else {
            continue;
        };
        if idiom.fix_policy == FixPolicy::SkillOnly {
            continue;
        }
        if idiom.severity == Severity::Error {
            *had_error = true;
        }
        println!("{}: [{}] {} — {}", path.display(), hit.id, idiom.title, idiom.why);
    }
}

fn load_cascade() -> Result<IdiomSet> {
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

fn ext_lang(path: &Path) -> Option<SupportLang> {
    match path.extension().and_then(|e| e.to_str())? {
        "py" => support_lang("python"),
        "ts" => support_lang("typescript"),
        _ => None,
    }
}
