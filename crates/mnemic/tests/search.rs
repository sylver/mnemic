mod common;

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

fn run(vault: &Path, args: &[&str]) -> Output {
    let mut command = common::mnemic();
    command.current_dir(vault);
    command.args(args).output().expect("run mnemic")
}

fn vault() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/search")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("json")
}

fn ids(value: &Value) -> Vec<String> {
    value["data"]["hits"]
        .as_array()
        .expect("hits")
        .iter()
        .map(|hit| hit["id"].as_str().expect("id").to_string())
        .collect()
}

#[test]
fn known_phrase_returns_ada_and_skips_root_docs() {
    let output = run(&vault(), &["search", "phosphor-lantern: west", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value = json(&output);
    insta::assert_snapshot!(serde_json::to_string_pretty(&value).unwrap());
    assert_eq!(ids(&value), vec!["01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string()]);
    assert_eq!(value["data"]["hits"][0]["path"], "people/ada.md");
    assert_eq!(value["data"]["hits"][0]["section"], "truth");
}

#[test]
fn filters_drop_ada_and_the_unfiltered_query_keeps_her() {
    let root = vault();
    let plain = run(&root, &["search", "phosphor-lantern: west", "--json"]);
    assert!(ids(&json(&plain)).contains(&"01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string()));

    let by_type = run(
        &root,
        &[
            "search",
            "phosphor-lantern: west",
            "--type",
            "note",
            "--json",
        ],
    );
    assert_eq!(by_type.status.code(), Some(0));
    assert!(!ids(&json(&by_type)).contains(&"01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string()));

    let by_tag = run(
        &root,
        &[
            "search",
            "phosphor-lantern: west",
            "--tag",
            "home",
            "--json",
        ],
    );
    assert!(!ids(&json(&by_tag)).contains(&"01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string()));

    let by_since = run(
        &root,
        &[
            "search",
            "phosphor-lantern: west",
            "--since",
            "2026-06-01",
            "--json",
        ],
    );
    assert!(!ids(&json(&by_since)).contains(&"01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string()));

    let tag_hit = run(
        &root,
        &[
            "search",
            "phosphor-lantern: west",
            "--tag",
            "work",
            "--json",
        ],
    );
    assert_eq!(
        ids(&json(&tag_hit)),
        vec!["01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string()]
    );
    let since_hit = run(
        &root,
        &[
            "search",
            "phosphor-lantern: west",
            "--since",
            "2026-01-15",
            "--json",
        ],
    );
    assert_eq!(
        ids(&json(&since_hit)),
        vec!["01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string()]
    );
}

#[test]
fn ranking_snapshot() {
    let output = run(&vault(), &["search", "alpha beta", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap());
}

#[test]
fn literal_dash_quote_and_colon_do_not_error() {
    let output = run(&vault(), &["search", "\"hello-world:", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = json(&output);
    assert_eq!(value["ok"], true);
    assert_eq!(ids(&value), vec!["01ARZ3NDEKTSV4RRFFQ69G5FB5".to_string()]);
}

#[test]
fn empty_query_zero_limit_and_visibility_are_usage() {
    let root = vault();
    let empty = run(&root, &["search", "--json"]);
    assert_eq!(empty.status.code(), Some(2));
    assert_eq!(json(&empty)["error"]["code"], "USAGE");

    let blank = run(&root, &["search", "   ", "--json"]);
    assert_eq!(blank.status.code(), Some(2));
    assert_eq!(json(&blank)["error"]["code"], "USAGE");

    let limit = run(&root, &["search", "ada", "--limit", "0", "--json"]);
    assert_eq!(limit.status.code(), Some(2));
    assert_eq!(json(&limit)["error"]["code"], "USAGE");

    let flag = run(&root, &["search", "ada", "--visibility", "private"]);
    assert_eq!(flag.status.code(), Some(2));

    let index = run(&root, &["index"]);
    assert_eq!(index.status.code(), Some(2));
}

#[test]
fn deleting_the_note_removes_it_from_the_next_search() {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&vault(), dir.path());
    let output = run(dir.path(), &["search", "phosphor-lantern: west", "--json"]);
    assert_eq!(
        ids(&json(&output)),
        vec!["01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string()]
    );
    fs::remove_file(dir.path().join("people/ada.md")).unwrap();
    let gone = run(dir.path(), &["search", "phosphor-lantern: west", "--json"]);
    assert_eq!(gone.status.code(), Some(0));
    assert!(json(&gone)["data"]["hits"].as_array().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn a_note_symlink_to_an_outside_fifo_is_not_a_hit() {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&vault(), dir.path());
    let outside = tempfile::tempdir().unwrap();
    mkfifo(&outside.path().join("pipe"));
    fs::write(outside.path().join("secret.md"), outside_note()).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("pipe"),
        dir.path().join("notes/escape.md"),
    )
    .unwrap();
    assert_search_ignores_escape(dir.path());
}

#[cfg(unix)]
#[test]
fn a_fifo_in_the_vault_is_not_a_hit() {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&vault(), dir.path());
    mkfifo(&dir.path().join("notes/fifo.md"));
    assert_search_ignores_escape(dir.path());
}

#[cfg(unix)]
#[test]
fn an_outside_directory_holding_a_fifo_and_a_note_is_not_a_hit() {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&vault(), dir.path());
    let outside = tempfile::tempdir().unwrap();
    mkfifo(&outside.path().join("pipe"));
    fs::write(outside.path().join("secret.md"), outside_note()).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("notes/escaped")).unwrap();
    assert_search_ignores_escape(dir.path());
}

#[cfg(unix)]
fn outside_note() -> &'static str {
    "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FB6\ntype: note\nschema_version: 1\ntitle: Secret\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\ncreated_by: sylver\nstatus: active\n---\n\nvault-escape-phrase phosphor-lantern\n"
}

#[cfg(unix)]
fn mkfifo(path: &Path) {
    let status = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("mkfifo");
    assert!(status.success(), "mkfifo {path:?}");
}

#[cfg(unix)]
fn assert_search_ignores_escape(vault: &Path) {
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    let bin = assert_cmd::cargo::cargo_bin("mnemic");
    let mut child = Command::new(bin);
    child
        .current_dir(vault)
        .env("XDG_CONFIG_HOME", common::config_home())
        .env_remove("HOME")
        .args(["search", "phosphor-lantern", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = child.spawn().expect("spawn search");
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(output) => output.expect("wait"),
        Err(_) => {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            panic!("search hung");
        }
    };
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(!text.contains("vault-escape-phrase"), "{text}");
    let value: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert!(
        ids(&value).contains(&"01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string()),
        "{text}"
    );
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}
