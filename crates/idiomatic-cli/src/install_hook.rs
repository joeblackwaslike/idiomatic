//! `idiomatic install-hook` — merge the PostToolUse entry into a settings.json.
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn run(settings_path: &Path) -> Result<()> {
    let mut root: serde_json::Value = if settings_path.is_file() {
        serde_json::from_str(&fs::read_to_string(settings_path)?)
            .with_context(|| format!("{} is not valid JSON", settings_path.display()))?
    } else {
        serde_json::json!({})
    };

    let obj = root.as_object_mut().context("settings root is not a JSON object")?;
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("`hooks` is not a JSON object")?;
    let post = hooks
        .entry("PostToolUse")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context("`hooks.PostToolUse` is not a JSON array")?;

    // Idempotent: don't add a second `idiomatic hook` entry.
    let already = post.iter().any(|e| e.to_string().contains("idiomatic hook"));
    if !already {
        post.push(serde_json::json!({
            "matcher": "Write|Edit",
            "hooks": [ { "type": "command", "command": "idiomatic hook" } ]
        }));
    }

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(settings_path, format!("{}\n", serde_json::to_string_pretty(&root)?))?;
    println!(
        "idiomatic hook {} in {}",
        if already { "already present" } else { "installed" },
        settings_path.display()
    );
    Ok(())
}
