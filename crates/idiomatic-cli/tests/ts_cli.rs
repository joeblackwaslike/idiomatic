use assert_cmd::Command;
use std::fs;

fn tmp(name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("idiomatic-ts-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    base.join(name)
}

#[test]
fn check_fix_rewrites_a_typescript_file() {
    let file = tmp("a.ts");
    fs::write(&file, "if (a == b) {\n  return;\n}\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["check", "--fix", file.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&file).unwrap(), "if (a === b) {\n  return;\n}\n");
}

#[test]
fn check_reports_no_console_without_fixing() {
    let file = tmp("b.ts");
    fs::write(&file, "console.log(\"hi\");\n").unwrap();

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["check", file.to_str().unwrap()])
        .assert()
        .success() // info severity → exit 0
        .stdout(predicates::str::contains("no-console"));

    assert_eq!(fs::read_to_string(&file).unwrap(), "console.log(\"hi\");\n");
}

#[test]
fn skillgen_renders_typescript() {
    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["skill-gen", "typescript"])
        .assert()
        .success()
        .stdout(predicates::str::contains("name: idiomatic-typescript"))
        .stdout(predicates::str::contains("Use `===` instead of `==`"));
}
