//! `idiomatic skill-gen <language> [--out <dir>]` — render the teaching skill.
use anyhow::Result;
use idiomatic_core::cascade::load_cascade;
use idiomatic_core::engine::{lang_applies, support_lang};
use idiomatic_core::render::render_skill;
use std::fs;
use std::path::Path;

pub fn run(language: &str, out: Option<&Path>) -> Result<()> {
    // Reject outright unknown languages — emitting a "0 idioms" skill for a
    // language we've never heard of would produce a misleading build artifact.
    let lang = support_lang(language)
        .ok_or_else(|| anyhow::anyhow!("unknown language '{language}'"))?;

    let set = load_cascade()?;

    // Warn (but still emit) if the resolved cascade has no idioms for this
    // language — a known language with no idioms is valid but unusual.
    let idiom_count = set.iter().filter(|i| lang_applies(&i.language, lang)).count();
    if idiom_count == 0 {
        eprintln!("warning: no idioms for '{language}' yet");
    }

    let content = render_skill(&set, language);
    match out {
        Some(dir) => {
            fs::create_dir_all(dir)?;
            let file = dir.join("SKILL.md");
            fs::write(&file, content)?;
            println!("wrote {}", file.display());
        }
        None => print!("{content}"),
    }
    Ok(())
}
