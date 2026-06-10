use assert_cmd::Command;
use predicates::str::contains;
use std::fs;

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

fn tempdir() -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("idiomatic-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&base);
    base
}
