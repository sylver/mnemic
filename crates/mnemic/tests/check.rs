use std::fs;
use std::path::{Path, PathBuf};
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
fn broken_type_classes_use_findings_shape() {
    for class in [
        "unknown-type",
        "missing-field",
        "alias-contact",
        "alias-phone",
        "schema-version-ahead",
    ] {
        let stdout = assert_json(
            &["check", &format!("fixtures/broken-type/{class}"), "--json"],
            2,
        );
        insta::with_settings!({ snapshot_suffix => class }, {
            insta::assert_snapshot!(stdout);
        });
    }
}

#[test]
fn invalid_pack_uses_the_findings_shape() {
    let err = mnemic_core::load_pack([("person", "folder = [")]).expect_err("pack");
    let envelope =
        mnemic_core::findings_envelope(vec![mnemic_core::invalid_pack_finding(&err)], Vec::new());
    let stdout = mnemic_core::to_json(&envelope).expect("json");
    assert!(stdout.contains("\"INVALID_PACK\""));
    assert!(stdout.contains("\"CHECK_FAILED\""));
    insta::assert_snapshot!(stdout);
}

#[test]
fn overlay_vault_accepts_replaced_and_custom_types() {
    let stdout = assert_json(&["check", "fixtures/overlay", "--json"], 0);
    insta::assert_snapshot!(stdout);
}

#[test]
fn overlay_required_field_comes_from_the_replacement() {
    let stdout = assert_json(&["check", "fixtures/overlay-gap", "--json"], 2);
    assert!(stdout.contains("\"MISSING_FIELD\""));
    assert!(stdout.contains("missing badge"));
    assert!(!stdout.contains("INVALID_PACK"));
}

#[test]
fn malformed_overlay_toml_is_invalid_pack() {
    let stdout = assert_json(&["check", "fixtures/overlay-malformed", "--json"], 2);
    assert!(stdout.contains("\"INVALID_PACK\""));
    assert!(stdout.contains("\"CHECK_FAILED\""));
}

#[test]
fn folder_clash_is_invalid_pack() {
    let stdout = assert_json(&["check", "fixtures/overlay-clash", "--json"], 2);
    assert!(stdout.contains("\"INVALID_PACK\""));
    assert!(stdout.contains("notes"));
}

#[test]
fn overlay_schema_version_governs_ahead_and_is_not_invalid_pack() {
    let ok = assert_json(
        &[
            "check",
            "fixtures/overlay-low-version/people/current.md",
            "--json",
        ],
        0,
    );
    assert!(ok.contains("\"ok\": true"));
    let ahead = assert_json(
        &[
            "check",
            "fixtures/overlay-low-version/people/ahead.md",
            "--json",
        ],
        2,
    );
    assert!(ahead.contains("\"SCHEMA_VERSION_AHEAD\""));
    assert!(ahead.contains("ahead of person 1"));
    assert!(!ahead.contains("INVALID_PACK"));
}

#[cfg(unix)]
#[test]
fn symlinked_overlay_file_is_invalid_pack_without_outside_text() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("mnemic-overlay-file-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let vault = root.join("vault");
    fs::create_dir_all(vault.join("types")).expect("vault");
    fs::write(
        vault.join("mnemic.toml"),
        "schema_version = 1\nactor = \"sylver\"\n",
    )
    .expect("config");
    let outside = root.join("org.toml");
    fs::write(
        &outside,
        "SECRET_OVERLAY_FILE_OUTSIDE\nschema_version = 1\nfolder = \"orgs\"\n",
    )
    .expect("outside");
    symlink(&outside, vault.join("types/org.toml")).expect("symlink");
    let stdout = assert_json_at(&vault, 2);
    assert!(stdout.contains("\"INVALID_PACK\""));
    assert!(stdout.contains("types/org.toml"));
    assert!(!stdout.contains("SECRET_OVERLAY_FILE_OUTSIDE"));
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn symlinked_types_dir_does_not_apply_an_outside_pack() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!("mnemic-overlay-dir-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let vault = root.join("vault");
    let outside = root.join("outside-types");
    fs::create_dir_all(vault.join("relics")).expect("vault");
    fs::create_dir_all(&outside).expect("outside");
    fs::write(
        vault.join("mnemic.toml"),
        "schema_version = 1\nactor = \"sylver\"\n",
    )
    .expect("config");
    fs::write(
        outside.join("relic.toml"),
        "schema_version = 1\nfolder = \"relics\"\ntimeline = false\nfield = []\n[state]\nvalues = [\"kept\"]\n",
    )
    .expect("relic");
    fs::write(
        vault.join("relics/one.md"),
        "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FB1\ntype: relic\nschema_version: 1\ntitle: One\ncreated: 2026-11-23T00:00:00Z\nupdated: 2026-11-23T00:00:00Z\ncreated_by: sylver\nstatus: active\nstate: kept\n---\n\nA relic.\n",
    )
    .expect("note");
    symlink(&outside, vault.join("types")).expect("symlink");
    let stdout = assert_json_at(&vault, 2);
    assert!(stdout.contains("\"INVALID_PACK\""));
    assert!(stdout.contains("types"));
    assert!(!stdout.contains("relic.toml"));
    let _ = fs::remove_dir_all(&root);
}

fn assert_json_at(path: &Path, code: i32) -> String {
    let path = path.to_str().expect("utf-8 path");
    assert_json(&["check", path, "--json"], code)
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
