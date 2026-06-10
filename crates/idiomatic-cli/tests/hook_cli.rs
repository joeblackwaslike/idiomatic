use assert_cmd::Command;
use std::fs;

fn tmp(name: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!("idiomatic-hook-{}", std::process::id()));
    fs::create_dir_all(&base).unwrap();
    base.join(name)
}

#[test]
fn hook_autofixes_and_reports_warn() {
    let file = tmp("a.py");
    fs::write(&file, "if x == None:\n    print(x)\n").unwrap();
    let payload = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        file.display()
    );

    Command::cargo_bin("idiomatic")
        .unwrap()
        .arg("hook")
        .env("IDIOMATIC_NO_TELEMETRY", "1")
        .write_stdin(payload)
        .assert()
        .code(2) // warn-and-instruct present → feed back to Claude
        .stderr(predicates::str::contains("print-debugging"))
        .stderr(predicates::str::contains("applied 1 idiom fixes"));

    // compare-none was autofixed in place; print left for the agent
    assert_eq!(fs::read_to_string(&file).unwrap(), "if x is None:\n    print(x)\n");
}

#[test]
fn hook_pure_autofix_exits_zero_with_system_message() {
    let file = tmp("b.py");
    fs::write(&file, "if x == None:\n    pass\n").unwrap();
    let payload = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        file.display()
    );

    Command::cargo_bin("idiomatic")
        .unwrap()
        .arg("hook")
        .env("IDIOMATIC_NO_TELEMETRY", "1")
        .write_stdin(payload)
        .assert()
        .success()
        .stdout(predicates::str::contains("applied 1 idiom fixes"))
        .stdout(predicates::str::contains("systemMessage"));

    assert_eq!(fs::read_to_string(&file).unwrap(), "if x is None:\n    pass\n");
}

#[test]
fn hook_ignores_non_write_tool() {
    Command::cargo_bin("idiomatic")
        .unwrap()
        .arg("hook")
        .env("IDIOMATIC_NO_TELEMETRY", "1")
        .write_stdin(r#"{"tool_name":"Bash","tool_input":{}}"#)
        .assert()
        .success();
}
