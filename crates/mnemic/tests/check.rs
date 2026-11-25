use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("mnemic").expect("mnemic binary");
    command.current_dir(repo());
    command.args(args).output().expect("run mnemic")
}

fn assert_json(args: &[&str], code: i32) -> String {
    let output = run(args);
    assert_eq!(
        output.status.code(),
        Some(code),
        "{args:?} stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 stdout")
}

#[test]
fn good_vault_exits_zero() {
    let stdout = assert_json(&["check", "fixtures/good", "--json"], 0);
    insta::assert_snapshot!(stdout);
}

#[test]
fn broken_core_classes_use_findings_shape() {
    for class in [
        "bad-id",
        "missing-field",
        "no-frontmatter",
        "not-a-mapping",
        "duplicate-keys",
        "unterminated",
    ] {
        let stdout = assert_json(
            &["check", &format!("fixtures/broken-core/{class}"), "--json"],
            2,
        );
        insta::with_settings!({ snapshot_suffix => class }, {
            insta::assert_snapshot!(stdout);
        });
    }
}

#[test]
fn broken_tree_exits_two() {
    let output = run(&["check", "fixtures/broken-core", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(stdout.contains("\"CHECK_FAILED\""));
    assert!(stdout.contains("\"BAD_ID\""));
    assert!(stdout.contains("\"MISSING_FIELD\""));
    assert!(stdout.contains("\"BAD_FRONTMATTER\""));
}

#[test]
fn newer_vault_is_fatal() {
    let stdout = assert_json(&["check", "fixtures/unsupported", "--json"], 2);
    insta::assert_snapshot!(stdout);
}

#[test]
fn bad_config_is_fatal() {
    let stdout = assert_json(&["check", "fixtures/bad-config", "--json"], 2);
    insta::assert_snapshot!(stdout);
}

#[test]
fn missing_vault_is_fatal() {
    let dir = tempfile::tempdir().expect("temp");
    let mut command = Command::cargo_bin("mnemic").expect("mnemic");
    let output = command
        .current_dir(dir.path())
        .args(["check", "--json"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    insta::assert_snapshot!(stdout);
}

#[test]
fn one_note_keeps_the_version_gate_and_limits_findings() {
    let good = assert_json(&["check", "fixtures/one-note/log/welcome.md", "--json"], 0);
    assert!(good.contains("\"ok\": true"));
    let bad = assert_json(&["check", "fixtures/one-note/log/broken.md", "--json"], 2);
    assert!(bad.contains("\"CHECK_FAILED\""));
    assert!(bad.contains("log/broken.md"));
    assert!(!bad.contains("welcome.md"));
    let gated = assert_json(
        &["check", "fixtures/unsupported/log/welcome.md", "--json"],
        2,
    );
    assert!(gated.contains("\"UNSUPPORTED_VAULT_VERSION\""));
    assert!(gated.contains("\"data\": null"));
}

#[test]
fn unknown_check_flag_is_rejected() {
    let output = run(&["check", "--fix"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert!(
        stderr.contains("unexpected")
            || stderr.contains("unrecognized")
            || stderr.contains("--fix")
    );
}
