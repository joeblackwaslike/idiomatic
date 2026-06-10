use assert_cmd::Command;
use std::fs;

#[test]
fn skillgen_stdout_renders_python_skill() {
    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["skill-gen", "python"])
        .assert()
        .success()
        .stdout(predicates::str::contains("name: idiomatic-python"))
        .stdout(predicates::str::contains("Use `is None`"));
}

#[test]
fn skillgen_out_writes_skill_md() {
    let dir = std::env::temp_dir().join(format!("idiomatic-skillgen-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);

    Command::cargo_bin("idiomatic")
        .unwrap()
        .args(["skill-gen", "python", "--out", dir.to_str().unwrap()])
        .assert()
        .success();

    let content = fs::read_to_string(dir.join("SKILL.md")).unwrap();
    assert!(content.contains("name: idiomatic-python"));
}
