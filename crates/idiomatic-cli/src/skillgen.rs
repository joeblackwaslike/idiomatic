//! `idiomatic skill-gen <language> [--out <dir>]` — render the teaching skill.
use crate::cascade::load_cascade;
use anyhow::Result;
use idiomatic_core::render::render_skill;
use std::fs;
use std::path::Path;

pub fn run(language: &str, out: Option<&Path>) -> Result<()> {
    let set = load_cascade()?;
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
