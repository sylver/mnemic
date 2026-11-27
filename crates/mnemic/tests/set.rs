mod common;

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use serde_json::Value;

fn run(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut command = common::mnemic();
    command.current_dir(dir);
    command.args(args).output().expect("run mnemic")
}

fn init(actor: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    let mut args = vec!["init", "--json"];
    if actor {
        args.extend(["--as", "t"]);
    }
    let output = run(dir.path(), &args);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    dir
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn json_value(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn code(output: &std::process::Output) -> String {
    json_value(output)["error"]["code"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn quests(dir: &Path) {
    fs::create_dir_all(dir.join("types")).unwrap();
    fs::write(
        dir.join("types/quests.toml"),
        "schema_version = 1\nfolder = \"quests\"\ntimeline = false\n[state]\nvalues = [\"open\"]\n[[field]]\nname = \"goal\"\ntype = \"string\"\nrequired = true\n",
    )
    .unwrap();
    fs::write(dir.join("types/quests.md"), "A quest.\n").unwrap();
}

fn dated(dir: &Path) {
    fs::create_dir_all(dir.join("types")).unwrap();
    fs::write(
        dir.join("types/mark.toml"),
        "schema_version = 1\nfolder = \"marks\"\ntimeline = false\n[state]\nvalues = [\"open\"]\n[[field]]\nname = \"d\"\ntype = \"date\"\nrequired = false\n[[field]]\nname = \"k\"\ntype = \"string\"\nrequired = false\n",
    )
    .unwrap();
    fs::write(dir.join("types/mark.md"), "A mark.\n").unwrap();
}

fn without_updated(text: &str) -> String {
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if bare.starts_with("updated:") || bare.starts_with("email:") {
            continue;
        }
        out.push_str(line);
    }
    out
}

#[test]
fn b1_new_field_then_check() {
    let dir = init(true);
    quests(dir.path());
    let created = run(
        dir.path(),
        &[
            "new",
            "quests",
            "Q",
            "--field",
            "goal=ship",
            "--as",
            "t",
            "--json",
        ],
    );
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let text = fs::read_to_string(dir.path().join("quests/q.md")).unwrap();
    assert!(text.contains("goal: ship\n"), "{text}");
    let check = run(dir.path(), &["check", "--json"]);
    assert_eq!(check.status.code(), Some(0), "{}", stderr(&check));
}

#[test]
fn b2_missing_required_field_writes_nothing() {
    let dir = init(true);
    quests(dir.path());
    let output = run(dir.path(), &["new", "quests", "Q2", "--as", "t", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(code(&output), "USAGE");
    assert!(json_value(&output)["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("goal"));
    assert!(!dir.path().join("quests/q2.md").exists());
}

#[test]
fn b3_set_preserves_other_bytes_and_b4_is_noop() {
    let dir = init(true);
    let note = dir.path().join("people/ada.md");
    let original = "\
---\n\
# keep\n\
title: \"Ada\"\n\
type: person\n\
id: \"01ARZ3NDEKTSV4RRFFQ69G5FAV\"\n\
schema_version: 1\n\
created: 2026-11-23T00:00:00Z\n\
created_by: \"t\"\n\
status: active\n\
updated: 2026-11-23T00:00:00Z\n\
---\n\
\n\
Body stays\n";
    fs::write(&note, original).unwrap();
    let output = run(
        dir.path(),
        &[
            "set",
            "ada",
            "--field",
            "email=a@b.c",
            "--json",
            "--vault",
            dir.path().to_str().unwrap(),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let body = json_value(&output);
    assert_eq!(body["data"]["changed"], serde_json::json!(["email"]));
    assert_eq!(body["data"]["noop"], false);
    insta::assert_snapshot!(
        "b3_set_email",
        String::from_utf8(output.stdout.clone()).unwrap()
    );
    let edited = fs::read_to_string(&note).unwrap();
    assert_eq!(without_updated(&edited), without_updated(original));
    assert!(edited.contains("email: \"a@b.c\"\n"));
    assert!(edited.contains("updated: "));
    assert!(!edited.contains("updated: 2026-11-23T00:00:00Z\n"));

    let modified = fs::metadata(&note).unwrap().modified().unwrap();
    let bytes = fs::read(&note).unwrap();
    let again = run(
        dir.path(),
        &[
            "set",
            "ada",
            "--field",
            "email=a@b.c",
            "--json",
            "--vault",
            dir.path().to_str().unwrap(),
        ],
    );
    assert_eq!(again.status.code(), Some(0), "{}", stderr(&again));
    let noop = json_value(&again);
    assert_eq!(noop["data"]["noop"], true);
    assert_eq!(noop["data"]["changed"], serde_json::json!([]));
    insta::assert_snapshot!("b4_set_noop", String::from_utf8(again.stdout).unwrap());
    assert_eq!(fs::read(&note).unwrap(), bytes);
    assert_eq!(fs::metadata(&note).unwrap().modified().unwrap(), modified);
}

#[test]
fn b5_state_must_be_declared() {
    let dir = init(true);
    let created = run(
        dir.path(),
        &[
            "new",
            "source",
            "S",
            "--url",
            "https://example.com",
            "--json",
        ],
    );
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let ok = run(dir.path(), &["set", "s", "--field", "state=read", "--json"]);
    assert_eq!(ok.status.code(), Some(0), "{}", stderr(&ok));
    let text = fs::read_to_string(dir.path().join("sources/s.md")).unwrap();
    assert!(text.contains("state: read\n"), "{text}");
    let bad = run(
        dir.path(),
        &["set", "s", "--field", "state=bogus", "--json"],
    );
    assert_eq!(bad.status.code(), Some(2), "{}", stderr(&bad));
    assert_eq!(code(&bad), "USAGE");
    let message = json_value(&bad)["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(message.contains("unread"), "{message}");
    assert!(message.contains("read"), "{message}");
    assert!(message.contains("archived"), "{message}");
}

#[test]
fn b6_block_sequence_becomes_a_flow_list() {
    let dir = init(true);
    let note = dir.path().join("people/n.md");
    let original = "---\nid: \"01ARZ3NDEKTSV4RRFFQ69G5FAV\"\ntype: person\nschema_version: 1\ntitle: \"N\"\ncreated: 2026-11-23T00:00:00Z\nupdated: 2026-11-23T00:00:00Z\ncreated_by: \"t\"\nstatus: active\ntags:\n  - old\n---\n\nKeep\n";
    fs::write(&note, original).unwrap();
    let output = run(
        dir.path(),
        &[
            "set", "n", "--field", "tags=a", "--field", "tags=b", "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let edited = fs::read_to_string(&note).unwrap();
    assert!(edited.contains("tags: [a, b]\n"), "{edited}");
    assert!(edited.contains("\n\nKeep\n"), "{edited}");
    assert!(!edited.contains("- old"));
}

#[test]
fn b7_add_is_noop_then_appends() {
    let dir = init(true);
    let note = dir.path().join("people/n.md");
    fs::write(
        &note,
        "\
---\n\
id: \"01ARZ3NDEKTSV4RRFFQ69G5FAV\"\n\
type: person\n\
schema_version: 1\n\
title: \"N\"\n\
created: 2026-11-23T00:00:00Z\n\
updated: 2026-11-23T00:00:00Z\n\
created_by: \"t\"\n\
status: active\n\
tags: [a]\n\
---\n\
\n\
Keep\n",
    )
    .unwrap();
    let first = run(dir.path(), &["set", "n", "--add", "tags=a", "--json"]);
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    assert_eq!(json_value(&first)["data"]["noop"], true);
    let second = run(dir.path(), &["set", "n", "--add", "tags=b", "--json"]);
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    let text = fs::read_to_string(&note).unwrap();
    assert!(text.contains("tags: [a, b]\n"), "{text}");
}

#[test]
fn b8_refused_keys_leave_the_file() {
    let dir = init(true);
    let created = run(dir.path(), &["new", "person", "Ada", "--json"]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let path = dir.path().join("people/ada.md");
    let before = fs::read(&path).unwrap();
    let mut snaps = String::new();
    for field in ["id=01ARZ3NDEKTSV4RRFFQ69G5FAV", "nope=x", "aliases=x"] {
        let output = run(dir.path(), &["set", "ada", "--field", field, "--json"]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{field}: {}",
            stderr(&output)
        );
        assert_eq!(code(&output), "USAGE");
        let message = json_value(&output)["error"]["message"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let key = field.split('=').next().unwrap();
        assert!(message.contains(key), "{message}");
        assert!(message.contains("person"), "{message}");
        snaps.push_str(&String::from_utf8(output.stdout).unwrap());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    insta::assert_snapshot!("b8_refused_keys", snaps);
}

#[test]
fn b9_date_field_rejects_a_bad_calendar_day() {
    let dir = init(true);
    dated(dir.path());
    let created = run(dir.path(), &["new", "mark", "M", "--json"]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let bad = run(
        dir.path(),
        &["set", "m", "--field", "d=2026-02-30", "--json"],
    );
    assert_eq!(bad.status.code(), Some(2), "{}", stderr(&bad));
    assert_eq!(code(&bad), "USAGE");
    let ok = run(
        dir.path(),
        &[
            "set",
            "m",
            "--field",
            "d=2026-11-27T10:00:00+01:00",
            "--json",
        ],
    );
    assert_eq!(ok.status.code(), Some(0), "{}", stderr(&ok));
    let text = fs::read_to_string(dir.path().join("marks/m.md")).unwrap();
    assert!(text.contains("d: 2026-11-27T10:00:00+01:00\n"), "{text}");
}

#[test]
fn b10_held_lock_exits_75() {
    let dir = init(true);
    let created = run(dir.path(), &["new", "person", "Ada", "--json"]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let note = dir.path().join("people/ada.md");
    let original = fs::read(&note).unwrap();
    let lock = dir.path().join(".mnemic/write.lock");
    let mut child = Command::new("python3")
        .arg("-c")
        .arg(
            "import fcntl,time,sys\n\
             f=open(sys.argv[1],'a')\n\
             fcntl.flock(f, fcntl.LOCK_EX)\n\
             print('held', flush=True)\n\
             time.sleep(30)\n",
        )
        .arg(&lock)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut buf = [0u8; 5];
    std::io::Read::read_exact(&mut stdout, &mut buf).unwrap();
    let started = SystemTime::now();
    let output = run(
        dir.path(),
        &["set", "ada", "--field", "email=a@b.c", "--json"],
    );
    let _ = child.kill();
    let _ = child.wait();
    assert!(started.elapsed().unwrap().as_secs() < 30);
    assert_eq!(output.status.code(), Some(75), "{}", stderr(&output));
    assert_eq!(code(&output), "LOCK_HELD");
    assert_eq!(fs::read(&note).unwrap(), original);
}

#[test]
fn b11_block_scalar_is_usage() {
    let dir = init(true);
    dated(dir.path());
    let created = run(dir.path(), &["new", "mark", "M", "--json"]);
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let note = dir.path().join("marks/m.md");
    let mut text = fs::read_to_string(&note).unwrap();
    let close = text.rfind("---\n").expect("fence");
    text.insert_str(close, "k: |\n  hello\n");
    fs::write(&note, &text).unwrap();
    let output = run(dir.path(), &["set", "m", "--field", "k=x", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(code(&output), "USAGE");
    let message = json_value(&output)["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(message.contains('k'), "{message}");
    assert_eq!(fs::read_to_string(&note).unwrap(), text);
}

#[test]
fn b12_url_flag_and_field_is_usage() {
    let dir = init(true);
    let output = run(
        dir.path(),
        &[
            "new", "source", "S", "--url", "u", "--field", "url=v", "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(code(&output), "USAGE");
    assert!(!dir.path().join("sources/s.md").exists());
}
