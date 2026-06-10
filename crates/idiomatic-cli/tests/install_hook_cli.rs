use assert_cmd::Command;
use std::fs;

#[test]
fn install_hook_merges_idempotently() {
    let dir = std::env::temp_dir().join(format!("idiomatic-install-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let settings = dir.join("settings.json");
    // pre-existing unrelated settings must be preserved
    fs::write(&settings, r#"{"model":"opus"}"#).unwrap();

    let run = || {
        Command::cargo_bin("idiomatic")
            .unwrap()
            .args(["install-hook", "--settings", settings.to_str().unwrap()])
            .assert()
            .success();
    };
    run();
    run(); // second run must not duplicate

    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(v["model"], "opus"); // preserved
    let post = v["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(post.len(), 1); // idempotent
    assert!(post[0].to_string().contains("idiomatic hook"));
    assert!(post[0].to_string().contains("Write|Edit"));
}
