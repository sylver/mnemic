use std::fs;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("mnemic").expect("mnemic binary");
    command.current_dir(dir);
    command.args(args).output().expect("run mnemic")
}

fn json_value(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn init_vault() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    let output = run(dir.path(), &["init", "--as", "alice", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    dir
}

#[test]
fn verbs_from_a_subdirectory_use_the_parent_vault() {
    let vault = init_vault();
    let created = run(vault.path(), &["new", "decision", "Ada", "--json"]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));

    let sub = vault.path().join("people").join("sub");
    fs::create_dir_all(&sub).unwrap();

    let search = run(&sub, &["search", "ada", "--json"]);
    assert_eq!(search.status.code(), Some(0), "{}", stderr(&search));
    let search_body = json_value(&search);
    assert_eq!(search_body["ok"], true);
    assert_eq!(search_body["data"]["hits"][0]["path"], "decisions/ada.md");

    let got = run(&sub, &["get", "ada", "--json"]);
    assert_eq!(got.status.code(), Some(0), "{}", stderr(&got));
    assert_eq!(json_value(&got)["data"]["path"], "decisions/ada.md");

    let links = run(&sub, &["links", "ada", "--json"]);
    assert_eq!(links.status.code(), Some(0), "{}", stderr(&links));
    assert_eq!(json_value(&links)["data"]["path"], "decisions/ada.md");

    let log = run(&sub, &["log", "ada", "2026-11-23 | hello", "--json"]);
    assert_eq!(log.status.code(), Some(0), "{}", stderr(&log));
    assert_eq!(json_value(&log)["ok"], true);
    let note = fs::read_to_string(vault.path().join("decisions/ada.md")).unwrap();
    assert!(note.contains("2026-11-23 | hello"));

    let check = run(&sub, &["check", "--json"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    assert_eq!(json_value(&check)["ok"], true);

    let moved = run(&sub, &["mv", "ada", "decisions/moved.md", "--json"]);
    assert_eq!(moved.status.code(), Some(0), "{}", stderr(&moved));
    assert_eq!(json_value(&moved)["data"]["to"], "decisions/moved.md");
    assert!(vault.path().join("decisions/moved.md").is_file());
    assert!(!sub.join("decisions").exists());
}

#[test]
fn new_from_a_subdirectory_writes_a_vault_relative_note() {
    let vault = init_vault();
    let sub = vault.path().join("people").join("sub");
    fs::create_dir_all(&sub).unwrap();

    let output = run(&sub, &["new", "person", "Bob", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let body = json_value(&output);
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["path"], "people/bob.md");
    assert!(vault.path().join("people/bob.md").is_file());
    assert!(!sub.join("bob.md").exists());
}

#[test]
fn check_relative_path_walks_from_that_path() {
    let vault = init_vault();
    let people = vault.path().join("people");
    fs::create_dir_all(people.join("sub")).unwrap();

    let output = run(&people, &["check", "sub", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(json_value(&output)["ok"], true);
}

#[test]
fn explicit_vault_flag_does_not_walk() {
    let vault = init_vault();
    let people = vault.path().join("people");
    let output = run(
        vault.path(),
        &[
            "search",
            "x",
            "--vault",
            &people.to_string_lossy(),
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let body = json_value(&output);
    assert_eq!(body["error"]["code"], "VAULT_NOT_FOUND");
    assert_eq!(body["error"]["message"], "no mnemic.toml found");
}

#[test]
fn missing_vault_message_stays_vault_not_found() {
    let dir = tempfile::tempdir().expect("temp");
    let output = run(dir.path(), &["check", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let body = json_value(&output);
    assert_eq!(body["ok"], false);
    assert!(body["data"].is_null());
    assert_eq!(body["error"]["code"], "VAULT_NOT_FOUND");
    assert_eq!(body["error"]["message"], "no mnemic.toml found");
}

#[test]
fn init_from_a_subdirectory_stays_nested_vault() {
    let vault = init_vault();
    let people = vault.path().join("people");
    let output = run(&people, &["init", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(json_value(&output)["error"]["code"], "NESTED_VAULT");
    assert!(!people.join("mnemic.toml").exists());
}
