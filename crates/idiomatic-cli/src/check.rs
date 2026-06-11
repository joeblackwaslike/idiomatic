//! `idiomatic check [--fix] <paths...>`
use idiomatic_core::cascade::{ext_lang, load_cascade};
use anyhow::Result;
use idiomatic_core::engine::{autofix_source, lang_applies, lint_source, CompiledIdiom, Hit};
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
            .filter(|i| lang_applies(&i.language, lang))
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

/// Report hits for a single file, updating the global error flag.
///
/// # Exit-code contract
///
/// `idiomatic check` exits **non-zero (1)** if and only if at least one
/// **`error`-severity** violation remains after any autofix pass.
/// `warn` and `info` severity violations are advisory: they are reported on
/// stdout but do **not** cause a non-zero exit.  This mirrors the convention
/// used by linters such as ESLint and Clippy where warnings are informational
/// and errors are gate-breakers in CI.
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
        println!("{}: {}", path.display(), idiomatic_core::render::render_diagnostic(idiom));
    }
}
