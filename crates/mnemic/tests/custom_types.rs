use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dest");
    for entry in fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let dest = to.join(entry.file_name());
        let kind = entry.file_type().expect("file type");
        if kind.is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).expect("copy file");
        }
    }
}

fn fixture_vault(with_types: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    let fixture = repo().join("fixtures/shadow");
    assert!(
        !fixture.join("types").exists(),
        "fixtures/shadow must not ship a types directory"
    );
    copy_tree(&fixture, dir.path());
    if with_types {
        let assets = repo().join("plugins/mnemic/skills/shadow/assets/types");
        copy_tree(&assets, &dir.path().join("types"));
    }
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("mnemic").expect("mnemic binary");
    command.current_dir(dir);
    command.args(args).output().expect("run mnemic")
}

fn json_at(dir: &Path, args: &[&str], code: i32) -> Value {
    let output = run(dir, args);
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
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn error_codes(body: &Value) -> Vec<String> {
    body["data"]["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|item| item["code"].as_str().unwrap_or("").to_string())
        .collect()
}

fn paths_for(body: &Value, code: &str) -> Vec<String> {
    body["data"]["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .filter(|item| item["code"] == code)
        .map(|item| item["path"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn b1_fixture_plus_asset_types_checks_clean() {
    let dir = fixture_vault(true);
    let body = json_at(dir.path(), &["check", "--json"], 0);
    assert_eq!(body["ok"], true);
    assert!(error_codes(&body).is_empty());
}

#[test]
fn b2_deleted_cited_source_is_broken_link_on_the_shadow() {
    let dir = fixture_vault(true);
    fs::remove_file(dir.path().join("sources/ada-example-letter.md")).expect("delete source");
    let body = json_at(dir.path(), &["check", "--json"], 2);
    assert_eq!(body["error"]["code"], "CHECK_FAILED");
    let paths = paths_for(&body, "BROKEN_LINK");
    assert!(
        paths
            .iter()
            .any(|path| path == "shadows/shadow-ada-example.md"),
        "{paths:?}"
    );
}

#[test]
fn b3_channel_without_locator_is_missing_field() {
    let dir = fixture_vault(true);
    let path = dir.path().join("channels/x-adaexample.md");
    let text = fs::read_to_string(&path).expect("channel");
    let next = text.replace("locator: \"x:adaexample\"\n", "");
    assert_ne!(text, next, "locator line missing from fixture");
    fs::write(&path, next).expect("rewrite channel");
    let body = json_at(dir.path(), &["check", "--json"], 2);
    assert_eq!(body["error"]["code"], "CHECK_FAILED");
    let paths = paths_for(&body, "MISSING_FIELD");
    assert!(
        paths.iter().any(|path| path == "channels/x-adaexample.md"),
        "{paths:?}"
    );
}

#[test]
fn b4_changed_raw_byte_is_sha256_drift() {
    let dir = fixture_vault(true);
    let path = dir
        .path()
        .join("sources/raw/shadows/ada-example/letter.txt");
    let mut bytes = fs::read(&path).expect("raw");
    bytes[0] ^= 0x01;
    fs::write(&path, bytes).expect("rewrite raw");
    let body = json_at(dir.path(), &["check", "--json"], 2);
    assert_eq!(body["error"]["code"], "CHECK_FAILED");
    assert!(
        error_codes(&body).iter().any(|code| code == "SHA256_DRIFT"),
        "{body}"
    );
}

#[test]
fn b5_new_shadow_writes_the_seven_headings_and_checks() {
    let dir = fixture_vault(true);
    let created = run(
        dir.path(),
        &[
            "new",
            "shadow",
            "Shadow: Bob Example",
            "--as",
            "t",
            "--json",
        ],
    );
    assert_eq!(
        created.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let note = dir.path().join("shadows/shadow-bob-example.md");
    let text = fs::read_to_string(&note).expect("created shadow");
    for heading in [
        "## Frame",
        "## Questions",
        "## Tests",
        "## Refusals",
        "## Distinctions",
        "## Updates",
        "## Claims",
    ] {
        assert!(text.contains(heading), "missing {heading}\n{text}");
    }
    let body = json_at(dir.path(), &["check", "--json"], 0);
    assert_eq!(body["ok"], true);
}

#[test]
fn install_copy_create_set_log_checks_clean() {
    let dir = fixture_vault(false);
    let assets = repo().join("plugins/mnemic/skills/shadow/assets/types");
    copy_tree(&assets, &dir.path().join("types"));

    let created = run(
        dir.path(),
        &[
            "new",
            "shadow",
            "Shadow: Bob Example",
            "--as",
            "t",
            "--json",
        ],
    );
    assert_eq!(
        created.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    let channel = run(
        dir.path(),
        &[
            "new",
            "channel",
            "x:bobexample",
            "--as",
            "t",
            "--field",
            "shadow=shadow-bob-example",
            "--field",
            "locator=x:bobexample",
            "--field",
            "url=https://x.com/bobexample",
            "--json",
        ],
    );
    assert_eq!(
        channel.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&channel.stderr)
    );

    let raw_rel = "sources/raw/shadows/bob-example/x-bobexample/2026-11-26.jsonl";
    let raw = dir.path().join(raw_rel);
    fs::create_dir_all(raw.parent().expect("raw parent")).expect("raw dir");
    fs::write(&raw, "{\"id\":\"1\",\"time\":\"2026-11-26T00:00:00Z\",\"url\":\"https://x.com/bobexample/status/1\",\"text\":\"hello\"}\n").expect("raw");
    let digest = sha256_hex(&raw);

    let source = run(
        dir.path(),
        &[
            "new",
            "source",
            "x:bobexample 2026-11-26",
            "--as",
            "t",
            "--field",
            "url=https://x.com/bobexample/status/1",
            "--field",
            &format!("path={raw_rel}"),
            "--field",
            &format!("sha256={digest}"),
            "--field",
            "tags=shadow/bob-example",
            "--field",
            "tags=domain/example",
            "--json",
        ],
    );
    assert_eq!(
        source.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );

    let updated = run(
        dir.path(),
        &[
            "set",
            "x-bobexample",
            "--field",
            "cursor=2026-11-26",
            "--field",
            "last_run_at=2026-11-26",
            "--field",
            "state=ready",
            "--json",
        ],
    );
    assert_eq!(
        updated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&updated.stderr)
    );

    let logged = run(
        dir.path(),
        &[
            "log",
            "x-bobexample",
            "2026-11-26 | x:bobexample | stored 1 items, cursor 2026-11-26",
            "--json",
        ],
    );
    assert_eq!(
        logged.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&logged.stderr)
    );

    let body = json_at(dir.path(), &["check", "--json"], 0);
    assert_eq!(body["ok"], true);
    assert!(error_codes(&body).is_empty(), "{body}");
}

fn sha256_hex(path: &Path) -> String {
    let output = std::process::Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum");
    assert!(output.status.success(), "sha256sum failed");
    String::from_utf8(output.stdout)
        .expect("utf-8")
        .split_whitespace()
        .next()
        .expect("digest")
        .to_string()
}

#[test]
fn b6_fixture_without_types_is_unknown_type() {
    let dir = fixture_vault(false);
    let body = json_at(dir.path(), &["check", "--json"], 2);
    assert_eq!(body["error"]["code"], "CHECK_FAILED");
    let paths = paths_for(&body, "UNKNOWN_TYPE");
    assert!(
        paths
            .iter()
            .any(|path| path == "shadows/shadow-ada-example.md"),
        "{paths:?}"
    );
    assert!(
        paths.iter().any(|path| path == "channels/x-adaexample.md"),
        "{paths:?}"
    );
}
