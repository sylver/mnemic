use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use assert_cmd::Command as AssertCommand;
use mnemic_core::{check_fix, fix_envelope, to_json, FixRequest, FixedClock, FIX_ID_SEED};
use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        let file_type = entry.file_type().unwrap();
        if file_type.is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

fn temp_fixture(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&repo().join("fixtures").join(name), dir.path());
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut command = AssertCommand::cargo_bin("mnemic").unwrap();
    command.current_dir(dir);
    command.args(args).output().unwrap()
}

fn json_of(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout {}\nstderr {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn library_fix(dir: &Path) -> String {
    let vault = mnemic_core::open_vault(dir, Some(dir)).unwrap();
    let clock = FixedClock {
        unix_millis: 1_763_865_600_000,
    };
    let outcome = check_fix(FixRequest {
        vault: &vault,
        path: &vault.root,
        clock: &clock,
        lock_timeout: Duration::from_secs(5),
    })
    .unwrap();
    let _ = FIX_ID_SEED;
    to_json(&fix_envelope(&outcome)).unwrap()
}

#[test]
fn fix_id_reports_the_seeded_id_and_keeps_every_other_byte() {
    let dir = temp_fixture("fix-id");
    let original = fs::read(dir.path().join("people/quoted.md")).unwrap();
    let stdout = library_fix(dir.path());
    insta::assert_snapshot!(stdout);
    let edited = fs::read(dir.path().join("people/quoted.md")).unwrap();
    let text = String::from_utf8(edited.clone()).unwrap();
    let mut restored = String::new();
    let mut removed = 0;
    for line in text.split_inclusive('\n') {
        if removed == 0 && line.starts_with("id: ") {
            removed += 1;
            continue;
        }
        restored.push_str(line);
    }
    assert_eq!(removed, 1);
    assert_eq!(restored.as_bytes(), original.as_slice());
    assert!(text.contains("updated: 2026-11-23T00:00:00Z"));
    assert!(text.contains("# comment stays"));
    assert!(text.contains("title: \"Ada # not a comment\""));
    assert!(text.contains("weird: 'single'"));
    let again = library_fix(dir.path());
    let second: Value = serde_json::from_str(&again).unwrap();
    assert_eq!(second["data"]["changes"], serde_json::json!([]));
    assert_eq!(second["ok"], true);
    assert_eq!(
        fs::read(dir.path().join("people/quoted.md")).unwrap(),
        edited
    );
    let output = run(dir.path(), &["check", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fix_type_uses_the_top_level_folder() {
    let dir = temp_fixture("fix-type");
    let stdout = library_fix(dir.path());
    insta::assert_snapshot!(stdout);
    let nested = fs::read_to_string(dir.path().join("people/friends/nested.md")).unwrap();
    assert!(nested.contains("type: person\n"));
    let note = fs::read_to_string(dir.path().join("notes/scratch.md")).unwrap();
    assert!(note.contains("type: note\n"));
    assert!(note.contains("id: 01ARZ3NDEKTSV4RRFFQ69G5FB3\n"));
    let again = fs::read(dir.path().join("people/ada.md")).unwrap();
    library_fix(dir.path());
    assert_eq!(fs::read(dir.path().join("people/ada.md")).unwrap(), again);
}

#[test]
fn fix_does_not_guess_type_id_or_root_docs() {
    let dir = temp_fixture("fix-no-guess");
    let before = walk_bytes(dir.path());
    let stdout = library_fix(dir.path());
    insta::assert_snapshot!(stdout);
    assert_eq!(walk_bytes(dir.path()), before);
    let body: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(body["data"]["changes"], serde_json::json!([]));
    assert_eq!(body["ok"], false);
    let messages = body["data"]["errors"].to_string();
    assert!(messages.contains("missing type"));
    assert!(messages.contains("BAD_ID"));
    assert!(!messages.contains("AGENTS.md"));
    assert!(!messages.contains("README.md"));
}

fn lone_vault(rel: &str, bytes: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("mnemic.toml"),
        "schema_version = 1\nactor = \"sylver\"\n",
    )
    .unwrap();
    let path = dir.path().join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    dir
}

fn without_minted_lines(text: &str) -> String {
    let mut restored = String::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("id: ") || line.starts_with("type: ") {
            continue;
        }
        restored.push_str(line);
    }
    restored
}

fn fix_twice(dir: &Path, rel: &str) -> Vec<u8> {
    let first: Value = serde_json::from_str(&library_fix(dir)).unwrap();
    assert!(!first["data"]["changes"].as_array().unwrap().is_empty());
    let edited = fs::read(dir.join(rel)).unwrap();
    let again: Value = serde_json::from_str(&library_fix(dir)).unwrap();
    assert_eq!(again["data"]["changes"], serde_json::json!([]));
    assert_eq!(fs::read(dir.join(rel)).unwrap(), edited);
    edited
}

#[test]
fn fix_keeps_a_leading_bom_and_inserts_inside_the_existing_block() {
    let original = b"\xEF\xBB\xBF---\ntitle: Kept\n---\n\nKept body.\n";
    let dir = lone_vault("inbox/bom.md", original);
    let edited = fix_twice(dir.path(), "inbox/bom.md");
    assert_eq!(&edited[..3], b"\xEF\xBB\xBF");
    let text = String::from_utf8(edited).unwrap();
    assert_eq!(without_minted_lines(&text).as_bytes(), original);
    assert!(text.contains("title: Kept\n"));
    assert!(text.contains("type: note\n"));
    assert_eq!(text.matches("\n---\n").count(), 1);
}

#[test]
fn fix_fills_a_closed_empty_fence_and_leaves_the_body() {
    for source in ["---\n---\nBody stays.\n", "---\n\n---\nBody stays.\n"] {
        let dir = lone_vault("inbox/empty.md", source.as_bytes());
        let edited = fix_twice(dir.path(), "inbox/empty.md");
        let text = String::from_utf8(edited).unwrap();
        assert_eq!(without_minted_lines(&text), source);
        assert!(text.contains("type: note\n"));
        assert!(text.ends_with("Body stays.\n"));
    }
}

#[test]
fn fix_leaves_a_non_mapping_fence_untouched() {
    let original = b"---\n- a\n---\nBody\n";
    let dir = lone_vault("inbox/list.md", original);
    let stdout = library_fix(dir.path());
    let body: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(body["data"]["changes"], serde_json::json!([]));
    assert_eq!(
        fs::read(dir.path().join("inbox/list.md")).unwrap(),
        original
    );
}

#[test]
fn fix_no_frontmatter_creates_a_minimal_block_and_keeps_the_body() {
    let dir = temp_fixture("fix-no-frontmatter");
    let original = fs::read(dir.path().join("inbox/raw.md")).unwrap();
    let stdout = library_fix(dir.path());
    insta::assert_snapshot!(stdout);
    let edited = fs::read(dir.path().join("inbox/raw.md")).unwrap();
    assert!(edited.ends_with(&original));
    let text = String::from_utf8(edited).unwrap();
    assert!(text.starts_with("---\n"));
    assert!(text.contains("type: note\n"));
    assert!(!text.contains("title:"));
    assert!(!text.contains("updated:"));
    assert!(!text.contains("status:"));
    let body: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "CHECK_FAILED");
    assert!(body["data"]["changes"].as_array().unwrap().len() >= 2);
}

#[test]
fn a_path_outside_the_vault_is_not_written() {
    let dir = temp_fixture("fix-id");
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.md");
    fs::write(&secret, "outside\n").unwrap();
    std::os::unix::fs::symlink(&secret, dir.path().join("people/linked.md")).unwrap();
    let vault = dir.path().to_string_lossy().to_string();
    let output = run(
        dir.path(),
        &[
            "check",
            "--fix",
            "--json",
            "--vault",
            &vault,
            "people/linked.md",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(json_of(&output)["error"]["code"], "OUTSIDE_VAULT");
    assert_eq!(json_of(&output)["data"], Value::Null);
    assert_eq!(fs::read(&secret).unwrap(), b"outside\n");
}

#[test]
fn a_held_lock_is_lock_held_and_writes_nothing() {
    let dir = temp_fixture("fix-id");
    let original = fs::read(dir.path().join("people/quoted.md")).unwrap();
    let lock_dir = dir.path().join(".mnemic");
    fs::create_dir_all(&lock_dir).unwrap();
    let lock = lock_dir.join("write.lock");
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
    use std::io::Read;
    stdout.read_exact(&mut buf).unwrap();
    let output = run(dir.path(), &["check", "--fix", "--json"]);
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(
        output.status.code(),
        Some(75),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let body = json_of(&output);
    assert_eq!(body["error"]["code"], "LOCK_HELD");
    assert_eq!(body["data"], Value::Null);
    assert_eq!(
        fs::read(dir.path().join("people/quoted.md")).unwrap(),
        original
    );
}

#[test]
fn an_io_error_midway_keeps_earlier_fixes_and_a_rerun_converges() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("mnemic.toml"),
        "schema_version = 1\nactor = \"sylver\"\n",
    )
    .unwrap();
    let note = "---\n\
        type: person\n\
        schema_version: 1\n\
        title: A\n\
        created: 2026-11-23T00:00:00Z\n\
        updated: 2026-11-23T00:00:00Z\n\
        created_by: sylver\n\
        status: active\n\
        ---\n\n\
        Body.\n";
    fs::create_dir_all(dir.path().join("people")).unwrap();
    fs::create_dir_all(dir.path().join("zzz")).unwrap();
    fs::write(dir.path().join("people/a.md"), note).unwrap();
    fs::write(dir.path().join("zzz/b.md"), note).unwrap();
    let zzz = dir.path().join("zzz");
    let mut perms = fs::metadata(&zzz).unwrap().permissions();
    perms.set_mode(0o555);
    fs::set_permissions(&zzz, perms).unwrap();
    let output = run(dir.path(), &["check", "--fix", "--json"]);
    let people = fs::read_to_string(dir.path().join("people/a.md")).unwrap();
    if output.status.code() == Some(0) {
        assert!(people.contains("id: "));
        return;
    }
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(json_of(&output)["error"]["code"], "IO");
    assert_eq!(json_of(&output)["data"], Value::Null);
    assert!(people.contains("\nid: "));
    assert!(!fs::read_to_string(dir.path().join("zzz/b.md"))
        .unwrap()
        .contains("\nid: "));
    let mut perms = fs::metadata(&zzz).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&zzz, perms).unwrap();
    let output = run(dir.path(), &["check", "--fix", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let people_after = fs::read_to_string(dir.path().join("people/a.md")).unwrap();
    assert_eq!(people_after, people);
    assert!(fs::read_to_string(dir.path().join("zzz/b.md"))
        .unwrap()
        .contains("\nid: "));
    let third = run(dir.path(), &["check", "--fix", "--json"]);
    assert_eq!(json_of(&third)["data"]["changes"], serde_json::json!([]));
}

fn walk_bytes(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some(".mnemic") {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            walk(root, &path, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, fs::read(&path).unwrap()));
        }
    }
}
