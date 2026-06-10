//! `idiomatic check [--fix] <paths...>`
use crate::cascade::{ext_lang, load_cascade};
use anyhow::Result;
use idiomatic_core::engine::{autofix_source, lint_source, support_lang, CompiledIdiom, Hit};
use idiomatic_core::pack::{FixPolicy, Severity};
use idiomatic_core::resolve::IdiomSet;
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
