use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("dest");
    for entry in fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let dest = to.join(entry.file_name());
        let file_type = entry.file_type().expect("file type");
        if file_type.is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).expect("copy");
        }
    }
}

fn temp_from_fixture(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/{name}"));
    copy_tree(&fixture, dir.path());
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("mnemic").expect("mnemic binary");
    command.current_dir(dir);
    command.args(args).output().expect("run mnemic")
}

fn json_value(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn names(value: &Value, field: &str) -> Vec<String> {
    value["data"][field]
        .as_array()
        .unwrap_or_else(|| panic!("{field} array"))
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

#[test]
fn empty_dir_scaffolds_vault_without_copying_templates_or_git() {
    let dir = tempfile::tempdir().expect("temp dir");
    let output = run(dir.path(), &["init", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = json_value(&output);
    assert_eq!(body["ok"], true);
    assert!(body["error"].is_null());
    let created = names(&body, "created");
    for required in [
        "mnemic.toml",
        "AGENTS.md",
        ".gitignore",
        "inbox/.gitkeep",
        "people/.gitkeep",
    ] {
        assert!(created.iter().any(|path| path == required), "{created:?}");
    }
    assert!(names(&body, "skipped").is_empty());
    let agents = fs::read_to_string(dir.path().join("AGENTS.md")).expect("agents");
    assert!(agents.contains("mnemic new <type>"));
    assert!(agents.contains("Search the vault before creating a page"));
    assert!(agents.contains("One file per entity"));
    assert!(agents.contains("Append to the timeline"));
    assert!(agents.contains("sources/raw/"));
    assert!(agents.contains("Prefer the `mnemic` CLI with `--json`"));
    assert!(agents.contains("does not commit or push"));
    assert!(agents.contains("written immediately, not batched"));
    assert!(!agents.to_ascii_lowercase().contains("visibility"));
    assert!(!agents.contains("brain.toml"));
    assert!(!agents.contains(".brain/"));
    let ignore = fs::read_to_string(dir.path().join(".gitignore")).expect("gitignore");
    assert!(ignore.contains(".mnemic/\n") || ignore.lines().any(|line| line.trim() == ".mnemic/"));
    let toml = fs::read_to_string(dir.path().join("mnemic.toml")).expect("toml");
    assert!(toml.contains("schema_version = 1"));
    assert!(!toml.contains("default_visibility"));
    assert!(!toml.contains("[index]"));
    assert!(!toml.contains("[packs]"));
    assert!(!dir.path().join(".git").exists());
    assert!(!dir.path().join("types").exists());
    assert!(dir.path().join(".mnemic").is_dir());
}

#[test]
fn nonempty_dir_keeps_notes_and_appends_gitignore() {
    let dir = temp_from_fixture("init-nonempty");
    let note = fs::read(dir.path().join("keep.md")).expect("note");
    let raw = fs::read(dir.path().join("sources/raw/blob.md")).expect("raw");
    let output = run(dir.path(), &["init", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(dir.path().join("keep.md")).expect("note"), note);
    assert_eq!(
        fs::read(dir.path().join("sources/raw/blob.md")).expect("raw"),
        raw
    );
    let ignore = fs::read_to_string(dir.path().join(".gitignore")).expect("gitignore");
    assert!(ignore.starts_with("*.tmp\n"));
    assert!(ignore.lines().any(|line| line.trim() == ".mnemic/"));
    assert!(dir.path().join("mnemic.toml").is_file());
    assert!(dir.path().join("AGENTS.md").is_file());
}

#[test]
fn nested_unsupported_and_bad_config_write_nothing() {
    let nested = temp_from_fixture("init-nested");
    let child = nested.path().join("child");
    let before = fs::read(nested.path().join("mnemic.toml")).expect("parent toml");
    let output = run(&child, &["init", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = json_value(&output);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "NESTED_VAULT");
    assert!(body["data"].is_null());
    assert_eq!(
        fs::read(nested.path().join("mnemic.toml")).expect("parent"),
        before
    );
    assert!(!child.join("AGENTS.md").exists());
    assert!(!child.join(".gitignore").exists());

    let bad = temp_from_fixture("init-bad-config");
    let bad_before = fs::read(bad.path().join("mnemic.toml")).expect("bad toml");
    let output = run(bad.path(), &["init", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(json_value(&output)["error"]["code"], "BAD_CONFIG");
    assert_eq!(
        fs::read(bad.path().join("mnemic.toml")).expect("bad"),
        bad_before
    );
    assert!(!bad.path().join("AGENTS.md").exists());

    let newer = tempfile::tempdir().expect("temp");
    fs::write(
        newer.path().join("mnemic.toml"),
        "schema_version = 2\nactor = \"sylver\"\n",
    )
    .expect("write");
    let newer_before = fs::read(newer.path().join("mnemic.toml")).expect("newer");
    let output = run(newer.path(), &["init", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        json_value(&output)["error"]["code"],
        "UNSUPPORTED_VAULT_VERSION"
    );
    assert_eq!(
        fs::read(newer.path().join("mnemic.toml")).expect("newer"),
        newer_before
    );
    assert!(!newer.path().join("AGENTS.md").exists());
}

#[test]
fn existing_agents_file_is_skipped_and_left_unchanged() {
    let dir = tempfile::tempdir().expect("temp");
    let original = b"local agents\n";
    fs::write(dir.path().join("AGENTS.md"), original).expect("write");
    let output = run(dir.path(), &["init", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(dir.path().join("AGENTS.md")).expect("agents"),
        original
    );
    let skipped = names(&json_value(&output), "skipped");
    assert!(skipped.iter().any(|path| path == "AGENTS.md"));
}

#[test]
fn second_init_does_not_rewrite_toml_or_gitignore() {
    let dir = tempfile::tempdir().expect("temp");
    let first = run(dir.path(), &["init", "--json", "--as", "sylver"]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let toml_path = dir.path().join("mnemic.toml");
    let ignore_path = dir.path().join(".gitignore");
    let mut toml = fs::read_to_string(&toml_path).expect("toml");
    assert!(toml.contains("actor = \"sylver\""));
    toml.push_str("nickname = \"kept\"\n");
    fs::write(&toml_path, &toml).expect("edit toml");
    let ignore = fs::read(&ignore_path).expect("gitignore");
    let output = run(dir.path(), &["init", "--json", "--as", "other"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&toml_path).expect("toml"), toml);
    assert_eq!(fs::read(&ignore_path).expect("gitignore"), ignore);
    let body = json_value(&output);
    assert!(names(&body, "created").is_empty(), "{body}");
}

#[cfg(unix)]
#[test]
fn dangling_agents_symlink_is_skipped_and_unchanged() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("temp");
    let link = dir.path().join("AGENTS.md");
    symlink("missing-agents.md", &link).expect("symlink");
    let output = run(dir.path(), &["init", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(link
        .symlink_metadata()
        .expect("meta")
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_link(&link).expect("link"),
        PathBuf::from("missing-agents.md")
    );
    assert!(!dir.path().join("missing-agents.md").exists());
    let skipped = names(&json_value(&output), "skipped");
    assert!(
        skipped.iter().any(|path| path == "AGENTS.md"),
        "{skipped:?}"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_gitignore_needing_append_writes_nothing() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("temp");
    let target = dir.path().join("ignore-target");
    let original = b"*.tmp\n";
    fs::write(&target, original).expect("gitignore target");
    let link = dir.path().join(".gitignore");
    symlink("ignore-target", &link).expect("symlink");
    let output = run(dir.path(), &["init", "--json"]);
    assert_ne!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let body = json_value(&output);
    assert_eq!(body["ok"], false);
    assert!(body["data"].is_null());
    assert_eq!(body["error"]["code"], "IO");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(message.contains("symlink"), "{message}");
    assert!(message.contains(".gitignore"), "{message}");
    assert!(!dir.path().join("mnemic.toml").exists());
    assert!(!dir.path().join("AGENTS.md").exists());
    assert_eq!(fs::read(&target).expect("target"), original);
    assert!(link
        .symlink_metadata()
        .expect("meta")
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_link(&link).expect("link"),
        PathBuf::from("ignore-target")
    );
}

#[cfg(unix)]
#[test]
fn live_agents_symlink_is_skipped_and_target_unchanged() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("temp");
    let target = dir.path().join("agents-target");
    let original = b"kept agents\n";
    fs::write(&target, original).expect("agents target");
    let link = dir.path().join("AGENTS.md");
    symlink("agents-target", &link).expect("symlink");
    let output = run(dir.path(), &["init", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&target).expect("target"), original);
    assert!(link
        .symlink_metadata()
        .expect("meta")
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_link(&link).expect("link"),
        PathBuf::from("agents-target")
    );
    let skipped = names(&json_value(&output), "skipped");
    assert!(
        skipped.iter().any(|path| path == "AGENTS.md"),
        "{skipped:?}"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_gitignore_with_mnemic_line_is_skipped_unchanged() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("temp");
    let target = dir.path().join("ignore-target");
    let original = b"notes\n.mnemic/\n";
    fs::write(&target, original).expect("gitignore target");
    let link = dir.path().join(".gitignore");
    symlink("ignore-target", &link).expect("symlink");
    let output = run(dir.path(), &["init", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(&target).expect("target"), original);
    assert!(link
        .symlink_metadata()
        .expect("meta")
        .file_type()
        .is_symlink());
    let skipped = names(&json_value(&output), "skipped");
    assert!(
        skipped.iter().any(|path| path == ".gitignore"),
        "{skipped:?}"
    );
}

#[cfg(unix)]
#[test]
fn dangling_gitkeep_is_skipped() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("temp");
    let inbox = dir.path().join("inbox");
    fs::create_dir(&inbox).expect("inbox");
    let link = inbox.join(".gitkeep");
    symlink("missing-keep", &link).expect("symlink");
    let output = run(dir.path(), &["init", "--json"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(link
        .symlink_metadata()
        .expect("meta")
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_link(&link).expect("link"),
        PathBuf::from("missing-keep")
    );
    assert!(!inbox.join("missing-keep").exists());
    let skipped = names(&json_value(&output), "skipped");
    assert!(
        skipped.iter().any(|path| path == "inbox/.gitkeep"),
        "{skipped:?}"
    );
}

#[cfg(unix)]
#[test]
fn dangling_mnemic_toml_writes_nothing() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().expect("temp");
    let link = dir.path().join("mnemic.toml");
    symlink("missing.toml", &link).expect("symlink");
    let output = run(dir.path(), &["init", "--json"]);
    assert_ne!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let body = json_value(&output);
    assert_eq!(body["ok"], false);
    assert!(body["data"].is_null());
    assert_eq!(body["error"]["code"], "IO");
    assert!(!dir.path().join(".mnemic").exists());
    assert!(!dir.path().join("AGENTS.md").exists());
    assert!(!dir.path().join(".gitignore").exists());
    assert!(link
        .symlink_metadata()
        .expect("meta")
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_link(&link).expect("link"),
        PathBuf::from("missing.toml")
    );
}

#[test]
fn vault_flag_does_not_walk_and_does_not_init_the_cwd() {
    let dir = tempfile::tempdir().expect("temp");
    let target = dir.path().join("vault");
    fs::create_dir(&target).expect("vault");
    fs::write(dir.path().join("note.md"), b"outside\n").expect("note");
    let output = run(dir.path(), &["init", "--json", "--vault", "vault"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!dir.path().join("mnemic.toml").exists());
    assert!(target.join("mnemic.toml").is_file());
    assert_eq!(
        fs::read(dir.path().join("note.md")).expect("note"),
        b"outside\n"
    );
    let body = json_value(&output);
    let vault = body["data"]["vault"].as_str().expect("vault");
    assert_eq!(
        Path::new(vault).canonicalize().expect("canon"),
        target.canonicalize().expect("canon")
    );
}
