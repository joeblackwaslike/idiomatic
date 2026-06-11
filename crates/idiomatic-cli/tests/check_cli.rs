use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use std::path::PathBuf;

#[test]
fn check_fix_rewrites_a_python_file() {
    let dir = tempdir();
    let file = dir.join("sample.py");
    fs::write(&file, "if x == None:\n    pass\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["check", "--fix", file.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&file).unwrap(), "if x is None:\n    pass\n");
}

#[test]
fn check_reports_warn_and_instruct_without_fixing() {
    let dir = tempdir();
    let file = dir.join("p.py");
    fs::write(&file, "print(\"hi\")\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["check", file.to_str().unwrap()])
        .assert()
        .success() // info severity → exit 0
        .stdout(contains("print-debugging"));

    // unchanged: warn-and-instruct never rewrites
    assert_eq!(fs::read_to_string(&file).unwrap(), "print(\"hi\")\n");
}

// ── Exit-code contract tests ───────────────────────────────────────────────
//
// Contract: `idiomatic check` exits 1 iff at least one *error*-severity
// violation remains; `warn` and `info` are advisory and exit 0.

/// Exit-0 path: a file with only a `warn`-severity idiom (`compare-none`)
/// must exit 0.  Tying this to the contract explicitly.
#[test]
fn check_exit_contract_warn_severity_exits_zero() {
    let dir = tempdir_unique("contract-warn");
    let file = dir.join("w.py");
    fs::write(&file, "if x == None:\n    pass\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["check", file.to_str().unwrap()])
        .assert()
        // warn severity → exit 0 per contract
        .success();
}

/// Exit-1 path: a project-layer pack defines ONE `error`-severity
/// `warn-and-instruct` idiom; a matching source file must cause exit 1.
#[test]
fn check_exit_contract_error_severity_exits_one() {
    let proj = tempdir_unique("contract-error");
    let idiomatic_dir = proj.join(".idiomatic");
    fs::create_dir_all(&idiomatic_dir).unwrap();

    // Custom pack with a single error-severity idiom that matches `foo()`.
    let pack_yaml = r#"name: custom
language: python
version: 0.0.1
---
id: no-foo-error
language: python
title: "no foo"
why: "foo is banned"
severity: error
fix_policy: warn-and-instruct
rule:
  pattern: "foo()"
"#;
    fs::write(idiomatic_dir.join("custom.yaml"), pack_yaml).unwrap();

    let source_file = proj.join("bad.py");
    fs::write(&source_file, "foo()\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        // CWD = temp project so `.idiomatic/` is discovered
        .current_dir(&proj)
        .args(["check", "bad.py"])
        .assert()
        // error severity → exit 1 per contract
        .failure()
        .stdout(contains("no-foo-error"));
}

fn tempdir_unique(tag: &str) -> PathBuf {
    let base = std::env::temp_dir()
        .join(format!("idiomatic-{tag}-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    base
}

fn tempdir() -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("idiomatic-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&base);
    base
}
