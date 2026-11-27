use std::fs;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use mnemic_core::{create_note, FixedClock, NewRequest, UlidGenerator};
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

fn init_vault(actor: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    let mut args = vec!["init", "--json"];
    if actor {
        args.extend(["--as", "sylver"]);
    }
    let output = run(dir.path(), &args);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    dir
}

fn code(output: &Output) -> String {
    json_value(output)["error"]["code"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

#[test]
fn new_person_prints_path_and_id_and_checks() {
    let dir = init_vault(true);
    let output = run(dir.path(), &["new", "person", "Test Person", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let body = json_value(&output);
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["path"], "people/test-person.md");
    assert_eq!(body["data"]["noop"], false);
    assert!(body["error"].is_null());
    let id = body["data"]["id"].as_str().expect("id");
    assert_eq!(id.len(), 26);
    let text = fs::read_to_string(dir.path().join("people/test-person.md")).unwrap();
    assert!(text.contains("status: active"));
    assert!(!text.contains("visibility"));
    assert!(text.contains("created_by: \"sylver\""));
    let check = run(dir.path(), &["check", "--json"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
}

#[test]
fn titles_with_colon_quotes_and_hash_are_valid_yaml() {
    let dir = init_vault(true);
    let title = "Say: \"hi\" #1";
    let output = run(dir.path(), &["new", "person", title, "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let path = json_value(&output)["data"]["path"]
        .as_str()
        .unwrap()
        .to_string();
    let text = fs::read_to_string(dir.path().join(&path)).unwrap();
    let parsed = mnemic_core::parse_note(&text).expect("frontmatter");
    assert_eq!(parsed.values["title"].as_str(), Some(title));
    let check = run(dir.path(), &["check", "--json"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
}

#[test]
fn same_id_and_path_is_a_noop() {
    let dir = init_vault(true);
    let first = run(dir.path(), &["new", "person", "Test Person", "--json"]);
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    let id = json_value(&first)["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let path = dir.path().join("people/test-person.md");
    let before = fs::read(&path).unwrap();
    let again = run(
        dir.path(),
        &["new", "person", "Test Person", "--id", &id, "--json"],
    );
    assert_eq!(again.status.code(), Some(0), "{}", stderr(&again));
    let body = json_value(&again);
    assert_eq!(body["data"]["noop"], true);
    assert_eq!(body["data"]["id"], id);
    assert_eq!(body["data"]["path"], "people/test-person.md");
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn same_id_at_a_different_path_is_id_exists() {
    let dir = init_vault(true);
    let first = run(dir.path(), &["new", "person", "Ada", "--json"]);
    let id = json_value(&first)["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let output = run(
        dir.path(),
        &["new", "person", "Other", "--id", &id, "--json"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(code(&output), "ID_EXISTS");
    assert!(!dir.path().join("people/other.md").exists());
}

#[test]
fn slug_collision_across_folders_and_aliases() {
    let dir = init_vault(true);
    let org = run(dir.path(), &["new", "org", "Sam", "--json"]);
    assert_eq!(org.status.code(), Some(0), "{}", stderr(&org));
    let person = run(dir.path(), &["new", "person", "Sam", "--json"]);
    assert_eq!(person.status.code(), Some(2), "{}", stderr(&person));
    assert_eq!(code(&person), "SLUG_COLLISION");
    assert!(!dir.path().join("people/sam.md").exists());

    let alias = dir.path().join("people/ada.md");
    fs::write(
        &alias,
        "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FB1\ntype: person\nschema_version: 1\ntitle: Ada\ncreated: 2026-11-23T00:00:00Z\nupdated: 2026-11-23T00:00:00Z\ncreated_by: sylver\nstatus: active\naliases:\n  - samwise\n---\n\nA person.\n",
    )
    .unwrap();
    let hit = run(dir.path(), &["new", "topic", "Samwise", "--json"]);
    assert_eq!(hit.status.code(), Some(2), "{}", stderr(&hit));
    assert_eq!(code(&hit), "SLUG_COLLISION");
}

#[test]
fn empty_slug_is_usage() {
    let dir = init_vault(true);
    let output = run(dir.path(), &["new", "person", "???", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(code(&output), "USAGE");
    assert!(
        stderr(&output).to_lowercase().contains("slug")
            || json_value(&output)["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("slug")
    );
    assert!(!dir.path().join("people").read_dir().unwrap().any(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "md")
    }));
}

#[test]
fn missing_template_and_unknown_type() {
    let dir = init_vault(true);
    fs::create_dir_all(dir.path().join("types")).unwrap();
    fs::write(
        dir.path().join("types/quest.toml"),
        "schema_version = 1\nfolder = \"quests\"\ntimeline = false\nfield = []\n[state]\nvalues = [\"open\"]\n",
    )
    .unwrap();
    let missing = run(dir.path(), &["new", "quest", "Ring", "--json"]);
    assert_eq!(missing.status.code(), Some(2), "{}", stderr(&missing));
    assert_eq!(code(&missing), "MISSING_TEMPLATE");
    assert!(!dir.path().join("quests").exists());
    let unknown = run(dir.path(), &["new", "dragon", "Smaug", "--json"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(code(&unknown), "UNKNOWN_TYPE");
}

#[test]
fn journal_date_and_second_file() {
    let dir = init_vault(true);
    let first = run(
        dir.path(),
        &[
            "new",
            "journal",
            "Standup",
            "--date",
            "2026-11-23",
            "--json",
        ],
    );
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    assert_eq!(
        json_value(&first)["data"]["path"],
        "journal/2026/2026-11-23.md"
    );
    let text = fs::read_to_string(dir.path().join("journal/2026/2026-11-23.md")).unwrap();
    assert!(text.contains("date: 2026-11-23\n"));
    assert!(!text.contains("date: 2026-11-23T"));
    assert!(!text.contains("[["));
    let check = run(dir.path(), &["check", "--json"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let body = json_value(&check);
    assert_eq!(body["ok"], true);
    assert!(body["data"]["errors"].as_array().unwrap().is_empty());
    let second = run(
        dir.path(),
        &[
            "new",
            "journal",
            "Standup",
            "--date",
            "2026-11-23",
            "--json",
        ],
    );
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    assert_eq!(
        json_value(&second)["data"]["path"],
        "journal/2026/2026-11-23-standup.md"
    );
}

#[test]
fn journal_without_date_uses_the_local_calendar_day() {
    let dir = init_vault(true);
    let output = run(dir.path(), &["new", "journal", "Today", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let path = json_value(&output)["data"]["path"]
        .as_str()
        .unwrap()
        .to_string();
    let text = fs::read_to_string(dir.path().join(&path)).unwrap();
    let today = String::from_utf8(
        std::process::Command::new("date")
            .arg("+%F")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let today = today.trim().to_string();
    assert!(text.contains(&format!("date: {today}\n")), "{text}");
    let year = &today[..4];
    assert!(path.starts_with(&format!("journal/{year}/{today}")));
    let check = run(dir.path(), &["check", "--json"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let body = json_value(&check);
    assert_eq!(body["ok"], true);
    assert!(body["data"]["errors"].as_array().unwrap().is_empty());
}

#[test]
fn every_builtin_type_checks_clean() {
    let dir = init_vault(true);
    let types = [
        "person", "org", "place", "project", "idea", "decision", "topic", "source", "journal",
        "note",
    ];
    for type_name in types {
        let title = format!("Fresh {type_name}");
        let mut args = vec!["new", type_name, title.as_str(), "--json"];
        if type_name == "source" {
            args.extend(["--url", "https://example.com/fresh"]);
        }
        let output = run(dir.path(), &args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{type_name}: {}",
            stderr(&output)
        );
    }
    let check = run(dir.path(), &["check", "--json"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let body = json_value(&check);
    assert_eq!(body["ok"], true);
    assert!(body["data"]["errors"].as_array().unwrap().is_empty());
}

#[test]
fn source_without_locator_is_usage_and_writes_nothing() {
    let dir = init_vault(true);
    let output = run(dir.path(), &["new", "source", "Missing Locator", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(code(&output), "USAGE");
    let blank = run(
        dir.path(),
        &["new", "source", "Blank Url", "--url", "   ", "--json"],
    );
    assert_eq!(blank.status.code(), Some(2), "{}", stderr(&blank));
    assert_eq!(code(&blank), "USAGE");
    let sources = dir.path().join("sources");
    let wrote = sources.exists()
        && fs::read_dir(&sources).unwrap().any(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "md")
        });
    assert!(!wrote);
}

#[test]
fn source_with_path_checks_clean() {
    let dir = init_vault(true);
    let output = run(
        dir.path(),
        &[
            "new",
            "source",
            "Local Blob",
            "--path",
            "sources/raw/blob.txt",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = fs::read_to_string(dir.path().join("sources/local-blob.md")).unwrap();
    assert!(text.contains("path: \"sources/raw/blob.txt\"\n"));
    let check = run(dir.path(), &["check", "--json"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
    let body = json_value(&check);
    assert_eq!(body["ok"], true);
    assert!(body["data"]["errors"].as_array().unwrap().is_empty());
}

#[test]
fn person_rejects_url_and_writes_nothing() {
    let dir = init_vault(true);
    let output = run(
        dir.path(),
        &[
            "new",
            "person",
            "X",
            "--url",
            "https://example.com",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(code(&output), "USAGE");
    assert!(!dir.path().join("people/x.md").exists());
}

#[test]
fn missing_author_names_both_ways() {
    let dir = init_vault(false);
    let output = run(dir.path(), &["new", "person", "Test Person", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(code(&output), "USAGE");
    let message = json_value(&output)["error"]["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(message.contains("--as"), "{message}");
    assert!(message.contains("actor"), "{message}");
    assert!(!dir.path().join("people/test-person.md").exists());
}

#[test]
fn bad_id_is_rejected() {
    let dir = init_vault(true);
    let output = run(
        dir.path(),
        &[
            "new",
            "person",
            "Test Person",
            "--id",
            "not-an-ulid",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(code(&output), "BAD_ID");
    assert!(!dir.path().join("people/test-person.md").exists());
}

#[test]
fn symlink_folder_outside_the_vault_is_refused() {
    let dir = init_vault(true);
    let outside = tempfile::tempdir().expect("outside");
    let people = dir.path().join("people");
    fs::remove_dir_all(&people).unwrap();
    std::os::unix::fs::symlink(outside.path(), &people).unwrap();
    let output = run(dir.path(), &["new", "person", "Test Person", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(code(&output), "OUTSIDE_VAULT");
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

#[test]
fn injected_clock_and_ulid_are_stable() {
    let dir = init_vault(true);
    let clock = FixedClock {
        unix_millis: 1_795_392_000_000,
    };
    let mut ids = UlidGenerator::from_seed(7);
    let mut expected = UlidGenerator::from_seed(7);
    let id = expected.next(&clock);
    let result = create_note(NewRequest {
        vault: dir.path(),
        type_name: "person",
        title: "Test Person",
        id: None,
        actor: Some("sylver"),
        date: None,
        path: None,
        url: None,
        fields: &[],
        clock: &clock,
        ids: &mut ids,
        lock_timeout: mnemic_core::write::DEFAULT_LOCK_TIMEOUT,
    })
    .expect("create");
    assert_eq!(result.id, id);
    assert_eq!(result.path, "people/test-person.md");
    let text = fs::read_to_string(dir.path().join(&result.path)).unwrap();
    assert!(text.contains("created: 2026-11-23T00:00:00Z"));
    assert!(text.contains("updated: 2026-11-23T00:00:00Z"));
}

#[test]
fn note_lands_in_inbox() {
    let dir = init_vault(true);
    let output = run(dir.path(), &["new", "note", "Scratch", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(json_value(&output)["data"]["path"], "inbox/scratch.md");
}
