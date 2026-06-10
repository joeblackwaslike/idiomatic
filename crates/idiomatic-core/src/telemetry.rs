//! Minimal JSONL telemetry: append one line per idiom trip. Powers the §9
//! feedback loop (trip-count ranking) without any reader/analysis yet.

use serde::Serialize;
use std::io::Write;
use std::path::Path;

/// One recorded idiom trip. Timestamps are supplied by the caller (the CLI uses
/// wall-clock seconds) so this module stays pure and testable.
#[derive(Debug, Serialize)]
pub struct TripEntry<'a> {
    pub idiom_id: &'a str,
    pub file: &'a str,
    pub fix_policy: &'a str,
    pub ts: u64,
}

/// Append one JSON line to the telemetry file, creating parent dirs as needed.
/// Best-effort by design: callers ignore the error so telemetry never breaks the
/// hot path. The `Result` is returned so tests can assert success.
pub fn append_trip(path: &Path, entry: &TripEntry) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_writes_one_json_line_per_call() {
        let dir = std::env::temp_dir().join(format!("idiomatic-telem-{}", std::process::id()));
        let path = dir.join("telemetry.jsonl");
        let _ = std::fs::remove_file(&path);

        let e1 = TripEntry { idiom_id: "compare-none", file: "a.py", fix_policy: "autofix", ts: 100 };
        let e2 = TripEntry { idiom_id: "print-debugging", file: "a.py", fix_policy: "warn-and-instruct", ts: 101 };
        append_trip(&path, &e1).unwrap();
        append_trip(&path, &e2).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        // each line is valid JSON carrying the idiom id
        let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["idiom_id"], "compare-none");
        assert_eq!(v["fix_policy"], "autofix");
        assert!(lines[1].contains("print-debugging"));
    }
}
