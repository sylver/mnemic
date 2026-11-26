use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_vault() -> PathBuf {
    let dir = tempfile::tempdir().expect("temp").keep();
    copy_dir(&repo().join("fixtures/mv"), &dir);
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy");
        }
    }
}

fn run(vault: &Path, args: &[&str]) -> std::process::Output {
    let mut command = assert_cmd::Command::cargo_bin("mnemic").expect("bin");
    command.current_dir(vault);
    command.args(args).output().expect("run")
}

fn run_crash(vault: &Path, point: &str, args: &[&str]) -> std::process::Output {
    let bin = assert_cmd::cargo::cargo_bin("mnemic");
    Command::new(bin)
        .current_dir(vault)
        .env("MNEMIC_MV_CRASH", point)
        .args(args)
        .output()
        .expect("run")
}

fn json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("json")
}

fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    walk(root, root, &mut files);
    files.sort();
    files
}

fn walk(root: &Path, dir: &Path, files: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).expect("read") {
        let entry = entry.expect("entry");
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel == ".mnemic/write.lock" {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, files);
        } else {
            files.push((rel, fs::read(&path).expect("read")));
        }
    }
}

#[test]
fn mv_keeps_the_id_adds_the_alias_and_rewrites_incoming_links() {
    let vault = copy_vault();
    let output = run(
        &vault,
        &["mv", "people/old-slug.md", "people/new-slug.md", "--json"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let body = fs::read_to_string(vault.join("people/new-slug.md")).unwrap();
    assert!(body.contains("id: 01ARZ3NDEKTSV4RRFFQ69G5FB2"));
    assert!(body.contains("old-slug"));
    assert!(!vault.join("people/old-slug.md").exists());
    let referrer = fs::read_to_string(vault.join("notes/referrer.md")).unwrap();
    assert!(referrer.contains("[[new-slug]]"));
    assert!(referrer.contains("[[new-slug|Ada]]"));
    assert!(referrer.contains("[[new-slug#Heading]]"));
    assert!(referrer.contains("![[new-slug]]"));
    assert!(referrer.contains("[[people/new-slug]]"));
    assert!(referrer.contains("[[Kept]]"));
    assert!(referrer.contains("`[[old-slug]]`"));
    assert!(referrer.contains("```\n[[old-slug]]\n```"));
    assert!(body.contains("[[new-slug]]"));
    assert!(body.contains("[[people/new-slug]]"));
    let value = json(&output);
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"]["id"], "01ARZ3NDEKTSV4RRFFQ69G5FB2");
    assert_eq!(value["data"]["alias_added"], true);
    assert!(value["data"]["rewritten"]
        .as_array()
        .unwrap()
        .iter()
        .any(|path| path == "notes/referrer.md"));
    assert!(value["error"].is_null());
    let got = run(&vault, &["get", "old-slug", "--json"]);
    assert_eq!(got.status.code(), Some(0), "{}", stderr(&got));
    assert_eq!(json(&got)["data"]["id"], "01ARZ3NDEKTSV4RRFFQ69G5FB2");
}

#[test]
fn no_rewrite_keeps_incoming_links_and_folder_only_skips_the_alias() {
    let vault = copy_vault();
    let output = run(
        &vault,
        &[
            "mv",
            "people/old-slug.md",
            "people/new-slug.md",
            "--no-rewrite",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let referrer = fs::read_to_string(vault.join("notes/referrer.md")).unwrap();
    assert!(referrer.contains("[[old-slug]]"));
    assert!(!referrer.contains("[[new-slug]]"));
    assert_eq!(
        json(&output)["data"]["rewritten"].as_array().unwrap().len(),
        0
    );
    let got = run(&vault, &["get", "old-slug", "--json"]);
    assert_eq!(got.status.code(), Some(0), "{}", stderr(&got));

    let vault = copy_vault();
    let before = fs::read_to_string(vault.join("people/old-slug.md")).unwrap();
    let output = run(
        &vault,
        &["mv", "people/old-slug.md", "notes/old-slug.md", "--json"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let moved = fs::read_to_string(vault.join("notes/old-slug.md")).unwrap();
    assert!(moved.contains("id: 01ARZ3NDEKTSV4RRFFQ69G5FB2"));
    assert_eq!(alias_block(&before), alias_block(&moved));
    assert_eq!(json(&output)["data"]["alias_added"], false);
    let referrer = fs::read_to_string(vault.join("notes/referrer.md")).unwrap();
    assert!(referrer.contains("[[notes/old-slug]]"));
    assert!(referrer.contains("[[old-slug]]\n"));
}

#[test]
fn refused_moves_change_no_note_bytes() {
    let cases = [
        ("people/old-slug.md", "notes/referrer.md", "DEST_EXISTS"),
        ("people/old-slug.md", "people/nope", "USAGE"),
        ("people/old-slug.md", "../outside.md", "OUTSIDE_VAULT"),
        (
            "people/old-slug.md",
            "sources/raw/old-slug.md",
            "RAW_IMMUTABLE",
        ),
        ("people/old-slug.md", ".mnemic/old-slug.md", "USAGE"),
    ];
    for (note, dest, code) in cases {
        let vault = copy_vault();
        let before = tree(&vault);
        let output = run(&vault, &["mv", note, dest, "--json"]);
        assert_eq!(output.status.code(), Some(2), "{code} {}", stderr(&output));
        assert_eq!(json(&output)["error"]["code"], code);
        assert_eq!(tree(&vault), before, "{code}");
    }

    let vault = copy_vault();
    let path = vault.join("people/old-slug.md");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("status: active", "status: archived");
    fs::write(&path, text).unwrap();
    let before = tree(&vault);
    let output = run(
        &vault,
        &["mv", "people/old-slug.md", "people/new-slug.md", "--json"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(json(&output)["error"]["code"], "NOTE_ARCHIVED");
    assert_eq!(tree(&vault), before);

    let vault = copy_vault();
    fs::write(
        vault.join("notes/other.md"),
        fs::read_to_string(vault.join("notes/referrer.md"))
            .unwrap()
            .replace("01ARZ3NDEKTSV4RRFFQ69G5FB3", "01ARZ3NDEKTSV4RRFFQ69G5FB4")
            .replace("aliases: []", "aliases:\n  - old-slug"),
    )
    .unwrap();
    let before = tree(&vault);
    let output = run(
        &vault,
        &["mv", "people/old-slug.md", "people/new-slug.md", "--json"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(json(&output)["error"]["code"], "ALIAS_NOT_UNIQUE");
    assert_eq!(tree(&vault), before);

    let vault = copy_vault();
    fs::write(
        vault.join("people/taken.md"),
        fs::read_to_string(vault.join("notes/referrer.md"))
            .unwrap()
            .replace("01ARZ3NDEKTSV4RRFFQ69G5FB3", "01ARZ3NDEKTSV4RRFFQ69G5FB5"),
    )
    .unwrap();
    let before = tree(&vault);
    let output = run(
        &vault,
        &["mv", "people/old-slug.md", "notes/taken.md", "--json"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(json(&output)["error"]["code"], "SLUG_COLLISION");
    assert_eq!(tree(&vault), before);
}

#[test]
fn crash_after_alias_and_after_rename_converges_on_rerun() {
    let vault = copy_vault();
    let crashed = run_crash(
        &vault,
        "after-alias",
        &["mv", "people/old-slug.md", "people/new-slug.md", "--json"],
    );
    assert_eq!(crashed.status.code(), Some(97));
    assert!(vault.join("people/old-slug.md").exists());
    let aliased = fs::read_to_string(vault.join("people/old-slug.md")).unwrap();
    assert!(aliased.contains("old-slug"));
    let got = run(&vault, &["get", "old-slug", "--json"]);
    assert_eq!(got.status.code(), Some(0), "{}", stderr(&got));

    let vault = copy_vault();
    let crashed = run_crash(
        &vault,
        "after-rename",
        &["mv", "people/old-slug.md", "people/new-slug.md", "--json"],
    );
    assert_eq!(crashed.status.code(), Some(97));
    assert!(vault.join("people/new-slug.md").exists());
    assert!(!vault.join("people/old-slug.md").exists());
    let referrer = fs::read_to_string(vault.join("notes/referrer.md")).unwrap();
    assert!(referrer.contains("[[old-slug]]"));
    let got = run(&vault, &["get", "old-slug", "--json"]);
    assert_eq!(got.status.code(), Some(0), "{}", stderr(&got));
    let again = run(
        &vault,
        &["mv", "people/old-slug.md", "people/new-slug.md", "--json"],
    );
    assert_eq!(again.status.code(), Some(0), "{}", stderr(&again));
    let referrer = fs::read_to_string(vault.join("notes/referrer.md")).unwrap();
    assert!(referrer.contains("[[new-slug]]"));
    assert!(!referrer.contains("[[old-slug]]\n") || referrer.contains("`[[old-slug]]`"));
}

#[test]
fn missing_source_basename_does_not_move_an_unrelated_note() {
    let vault = copy_vault();
    let before = tree(&vault);
    let missing = run(
        &vault,
        &["mv", "people/missing.md", "people/new-slug.md", "--json"],
    );
    assert_eq!(missing.status.code(), Some(2), "{}", stderr(&missing));
    assert_eq!(json(&missing)["error"]["code"], "NOT_FOUND");
    assert_eq!(tree(&vault), before);

    let other = run(
        &vault,
        &["mv", "notes/old-slug.md", "people/elsewhere.md", "--json"],
    );
    assert_eq!(other.status.code(), Some(2), "{}", stderr(&other));
    assert_eq!(json(&other)["error"]["code"], "NOT_FOUND");
    assert_eq!(tree(&vault), before);
    assert!(vault.join("people/old-slug.md").exists());

    fs::create_dir_all(vault.join("orgs")).unwrap();
    fs::write(
        vault.join("orgs/old-slug.md"),
        fs::read_to_string(vault.join("people/old-slug.md"))
            .unwrap()
            .replace("01ARZ3NDEKTSV4RRFFQ69G5FB2", "01ARZ3NDEKTSV4RRFFQ69G5FB6"),
    )
    .unwrap();
    let before = tree(&vault);
    let ambiguous = run(
        &vault,
        &["mv", "missing/old-slug.md", "people/new-slug.md", "--json"],
    );
    assert_eq!(ambiguous.status.code(), Some(2), "{}", stderr(&ambiguous));
    assert_eq!(json(&ambiguous)["error"]["code"], "AMBIGUOUS");
    assert_eq!(tree(&vault), before);
}

#[test]
fn directory_destination_keeps_the_basename_and_creates_parents() {
    let vault = copy_vault();
    fs::create_dir_all(vault.join("notes")).unwrap();
    let output = run(&vault, &["mv", "people/old-slug.md", "notes/", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(vault.join("notes/old-slug.md").exists());
    assert_eq!(json(&output)["data"]["alias_added"], false);
    assert_eq!(json(&output)["data"]["to"], "notes/old-slug.md");

    let vault = copy_vault();
    let output = run(
        &vault,
        &[
            "mv",
            "people/old-slug.md",
            "people/nested/new-slug.md",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(vault.join("people/nested/new-slug.md").exists());
    let got = run(&vault, &["get", "old-slug", "--json"]);
    assert_eq!(got.status.code(), Some(0), "{}", stderr(&got));
}

fn alias_block(text: &str) -> String {
    text.split("---")
        .nth(1)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("aliases:") || line.trim_start().starts_with("- "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
