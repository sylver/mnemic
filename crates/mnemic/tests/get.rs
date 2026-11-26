use assert_cmd::Command;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command as StdCommand, Output};
use std::time::Duration;

fn vault() -> String {
    format!("{}/../../fixtures/read", env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("mnemic").expect("mnemic binary");
    command.args(args).output().expect("run mnemic")
}

fn json_of(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

#[test]
fn get_by_id_slug_alias_and_path_returns_the_same_id() {
    let vault = vault();
    let queries = [
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "old-slug",
        "ada-alias",
        "people/old-slug.md",
    ];
    for query in queries {
        let output = run(&["get", query, "--json", "--vault", &vault]);
        assert_eq!(output.status.code(), Some(0), "{query}: {output:?}");
        let body = json_of(&output);
        assert_eq!(body["ok"], true);
        assert_eq!(body["data"]["id"], "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        assert!(body["error"].is_null());
    }
    let snapshot = run(&["get", "people/old-slug.md", "--json", "--vault", &vault]);
    let stdout = String::from_utf8(snapshot.stdout).expect("utf-8");
    insta::assert_snapshot!(stdout);
}

#[test]
fn get_ambiguous_alias_lists_candidates() {
    let output = run(&["get", "ambiguous-alias", "--json", "--vault", &vault()]);
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    insta::assert_snapshot!(stdout);
}

#[test]
fn links_lists_broken_wikilink_and_exits_zero() {
    let output = run(&["links", "old-slug", "--json", "--vault", &vault()]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let body = json_of(&output);
    assert_eq!(body["ok"], true);
    let broken = body["data"]["broken"].as_array().expect("broken");
    assert!(broken
        .iter()
        .any(|edge| { edge["raw"] == "missing-slug" && edge["kind"] == "wikilink" }));
    assert!(body["data"]["outgoing"]
        .as_array()
        .expect("outgoing")
        .iter()
        .all(|edge| edge.get("kind").is_some()));
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    insta::assert_snapshot!(stdout);
}

#[test]
fn get_missing_id_and_slug_are_not_found() {
    let vault = vault();
    for query in ["01ARZ3NDEKTSV4RRFFQ69G5FZZ", "no-such-slug"] {
        let output = run(&["get", query, "--json", "--vault", &vault]);
        assert_eq!(output.status.code(), Some(2), "{query}");
        let body = json_of(&output);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
        assert!(body["data"].is_null());
    }
}

#[test]
fn visibility_flag_is_usage() {
    let output = run(&["get", "old-slug", "--visibility", "public"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf-8");
    assert!(stderr.to_ascii_lowercase().contains("visibility") || stderr.contains("USAGE"));
}

#[test]
fn get_refuses_paths_that_escape_the_vault() {
    let output = run(&["get", "../outside.md", "--json", "--vault", &vault()]);
    assert_eq!(output.status.code(), Some(2));
    let body = json_of(&output);
    assert_eq!(body["error"]["code"], "OUTSIDE_VAULT");

    let dir = tempfile::tempdir().expect("vault");
    fs::write(
        dir.path().join("mnemic.toml"),
        "schema_version = 1\nactor = \"sylver\"\n",
    )
    .unwrap();
    let outside = tempfile::tempdir().expect("outside");
    let secret = outside.path().join("secret.md");
    fs::write(
        &secret,
        "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FAV\ntype: note\nschema_version: 1\ntitle: Secret\ncreated: 2026-11-23T00:00:00Z\nupdated: 2026-11-23T00:00:00Z\ncreated_by: sylver\nstatus: active\n---\nSECRET_BODY_MARKER\n",
    )
    .unwrap();
    symlink(&secret, dir.path().join("leak.md")).unwrap();
    let mut command = Command::cargo_bin("mnemic").expect("mnemic");
    let output = command
        .current_dir(dir.path())
        .args(["get", "leak.md", "--json"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let text = String::from_utf8(output.stdout).expect("utf-8");
    assert!(!text.contains("SECRET_BODY_MARKER"));
    let body = serde_json::from_str::<serde_json::Value>(&text).expect("json");
    assert_eq!(body["error"]["code"], "OUTSIDE_VAULT");
}

#[test]
fn parent_and_absolute_paths_are_outside_vault_for_get_and_links() {
    let vault = vault();
    let outside = "/tmp/mnemic-outside-note.md";
    for command in ["get", "links"] {
        for query in ["../x.md", outside] {
            let output = run(&[command, query, "--json", "--vault", &vault]);
            assert_eq!(output.status.code(), Some(2), "{command} {query}: {output:?}");
            assert_eq!(json_of(&output)["error"]["code"], "OUTSIDE_VAULT");
        }
    }
}

#[cfg(unix)]
#[test]
fn get_on_a_symlink_to_an_outside_fifo_is_outside_vault() {
    let vault = tempfile::tempdir().expect("vault");
    fs::write(
        vault.path().join("mnemic.toml"),
        "schema_version = 1\nactor = \"sylver\"\n",
    )
    .unwrap();
    let outside = tempfile::tempdir().expect("outside");
    let fifo = outside.path().join("trap");
    mkfifo(&fifo);
    symlink(&fifo, vault.path().join("leak.md")).unwrap();
    let output = Command::cargo_bin("mnemic")
        .expect("mnemic")
        .current_dir(vault.path())
        .args(["get", "leak.md", "--json"])
        .timeout(Duration::from_secs(2))
        .output()
        .expect("get returns before the fifo timeout");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let body = json_of(&output);
    assert_eq!(body["error"]["code"], "OUTSIDE_VAULT");
}

#[cfg(unix)]
#[test]
fn get_ignores_an_outside_directory_symlink_and_resolves_the_real_note() {
    let vault = tempfile::tempdir().expect("vault");
    fs::write(
        vault.path().join("mnemic.toml"),
        "schema_version = 1\nactor = \"sylver\"\n",
    )
    .unwrap();
    fs::create_dir(vault.path().join("people")).unwrap();
    let note = "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FAV\ntype: note\nschema_version: 1\ntitle: Real\ncreated: 2026-11-23T00:00:00Z\nupdated: 2026-11-23T00:00:00Z\ncreated_by: sylver\nstatus: active\n---\nREAL_NOTE\n";
    fs::write(vault.path().join("people/ada.md"), note).unwrap();
    let outside = tempfile::tempdir().expect("outside");
    fs::create_dir(outside.path().join("people")).unwrap();
    fs::write(outside.path().join("people/ada.md"), note).unwrap();
    symlink(outside.path(), vault.path().join("escape")).unwrap();
    let output = Command::cargo_bin("mnemic")
        .expect("mnemic")
        .current_dir(vault.path())
        .args(["get", "ada", "--json"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let body = json_of(&output);
    assert_eq!(body["data"]["path"], "people/ada.md");
    assert_eq!(body["data"]["id"], "01ARZ3NDEKTSV4RRFFQ69G5FAV");
}

fn mkfifo(path: &Path) {
    let status = StdCommand::new("mkfifo").arg(path).status().expect("mkfifo");
    assert!(status.success(), "mkfifo {path:?}");
}
