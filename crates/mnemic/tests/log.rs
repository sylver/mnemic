mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use assert_cmd::cargo::cargo_bin;
use mnemic_core::{log_note, FixedClock, LogRequest};
use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_vault() -> PathBuf {
    let dir = tempfile::tempdir().expect("temp").keep();
    copy_dir(&repo().join("fixtures/log"), &dir);
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy");
        }
    }
}

fn run(vault: &Path, args: &[&str]) -> std::process::Output {
    let mut command = common::mnemic();
    command.current_dir(vault);
    command.args(args).output().expect("run")
}

#[test]
fn log_inserts_the_newest_line_under_the_heading() {
    let vault = copy_vault();
    let output = run(
        &vault,
        &[
            "log",
            "people/ada.md",
            "2026-11-23 | test | hello",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let body = fs::read_to_string(vault.join("people/ada.md")).unwrap();
    let timeline = body.split("## Timeline").nth(1).unwrap();
    let bullets: Vec<_> = timeline
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect();
    assert_eq!(
        bullets,
        vec![
            "- 2026-11-23 | test | hello",
            "- 2026-11-01 | archive | kept"
        ]
    );
    assert!(timeline.starts_with("\n\n- 2026-11-23 | test | hello\n"));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["noop"], false);
    assert_eq!(json["data"]["heading_created"], false);
    assert_eq!(json["data"]["line"], "- 2026-11-23 | test | hello");
    assert!(json["error"].is_null());
}

#[test]
fn repeating_the_same_line_is_a_noop() {
    let vault = copy_vault();
    let args = ["log", "ada", "2026-11-23 | test | hello", "--json"];
    assert_eq!(run(&vault, &args).status.code(), Some(0));
    let before = fs::read(vault.join("people/ada.md")).unwrap();
    let again = run(&vault, &args);
    assert_eq!(again.status.code(), Some(0), "{}", stderr(&again));
    let json: Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(json["data"]["noop"], true);
    assert_eq!(fs::read(vault.join("people/ada.md")).unwrap(), before);
}

#[test]
fn disabled_pipe_and_bad_date_leave_the_file() {
    let vault = copy_vault();
    let source_before = fs::read(vault.join("sources/blob.md")).unwrap();
    let disabled = run(
        &vault,
        &["log", "sources/blob.md", "2026-11-23 | hello", "--json"],
    );
    assert_eq!(disabled.status.code(), Some(2), "{}", stderr(&disabled));
    let json: Value = serde_json::from_slice(&disabled.stdout).unwrap();
    assert_eq!(json["error"]["code"], "TIMELINE_DISABLED");
    assert_eq!(
        fs::read(vault.join("sources/blob.md")).unwrap(),
        source_before
    );

    let ada_before = fs::read(vault.join("people/ada.md")).unwrap();
    let pipe = run(
        &vault,
        &["log", "ada", "2026-11-23 | hello | there | extra", "--json"],
    );
    assert_eq!(pipe.status.code(), Some(2));
    let json: Value = serde_json::from_slice(&pipe.stdout).unwrap();
    assert_eq!(json["error"]["code"], "USAGE");

    let bad = run(&vault, &["log", "ada", "2026-13-40 | hello", "--json"]);
    assert_eq!(bad.status.code(), Some(2));
    let json: Value = serde_json::from_slice(&bad.stdout).unwrap();
    assert_eq!(json["error"]["code"], "USAGE");
    assert_eq!(fs::read(vault.join("people/ada.md")).unwrap(), ada_before);

    let two_before = fs::read(vault.join("people/two.md")).unwrap();
    let two = run(&vault, &["log", "two", "2026-11-23 | hello", "--json"]);
    assert_eq!(two.status.code(), Some(2), "{}", stderr(&two));
    let json: Value = serde_json::from_slice(&two.stdout).unwrap();
    assert_eq!(json["error"]["code"], "TIMELINE_HEADING_AMBIGUOUS");
    insta::assert_snapshot!(String::from_utf8(two.stdout).unwrap());
    assert_eq!(fs::read(vault.join("people/two.md")).unwrap(), two_before);
}

#[test]
fn updated_comes_from_the_injected_clock() {
    let vault = copy_vault();
    let clock = FixedClock {
        unix_millis: 1_795_435_200_000,
    };
    let result = log_note(LogRequest {
        vault: &vault,
        note: "ada",
        text: "2026-11-23 | test | hello",
        clock: &clock,
        lock_timeout: mnemic_core::write::DEFAULT_LOCK_TIMEOUT,
    })
    .expect("log");
    assert!(!result.noop);
    let body = fs::read_to_string(vault.join("people/ada.md")).unwrap();
    assert!(body.contains("updated: 2026-11-23T12:00:00Z"));
    assert!(body.contains("created: 2026-11-23T00:00:00Z"));
    assert!(body.contains("title: Ada"));
    let again = log_note(LogRequest {
        vault: &vault,
        note: "ada",
        text: "- 2026-11-23 | test | hello",
        clock: &FixedClock {
            unix_millis: 1_700_000_000_000,
        },
        lock_timeout: mnemic_core::write::DEFAULT_LOCK_TIMEOUT,
    })
    .expect("noop");
    assert!(again.noop);
    let after = fs::read_to_string(vault.join("people/ada.md")).unwrap();
    assert_eq!(after, body);
}

#[test]
fn backdated_line_keeps_date_order_and_same_day_order() {
    let vault = copy_vault();
    let output = run(
        &vault,
        &["log", "backdated", "2026-11-20 | mid | middle", "--json"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let body = fs::read_to_string(vault.join("people/backdated.md")).unwrap();
    let bullets = bullets_after(&body, "## Timeline");
    assert_eq!(
        bullets,
        vec![
            "- 2026-11-23 | later | newest",
            "- 2026-11-20 | mid | middle",
            "- 2026-11-01 | earlier | oldest",
        ]
    );
    let same = run(
        &vault,
        &[
            "log",
            "backdated",
            "2026-11-20 | mid | later same day",
            "--json",
        ],
    );
    assert_eq!(same.status.code(), Some(0), "{}", stderr(&same));
    let body = fs::read_to_string(vault.join("people/backdated.md")).unwrap();
    let bullets = bullets_after(&body, "## Timeline");
    assert_eq!(
        bullets,
        vec![
            "- 2026-11-23 | later | newest",
            "- 2026-11-20 | mid | later same day",
            "- 2026-11-20 | mid | middle",
            "- 2026-11-01 | earlier | oldest",
        ]
    );
}

#[test]
fn missing_heading_is_created_at_the_end() {
    let vault = copy_vault();
    let output = run(
        &vault,
        &["log", "fresh", "2026-11-23 | test | hello", "--json"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["data"]["heading_created"], true);
    let body = fs::read_to_string(vault.join("people/fresh.md")).unwrap();
    assert!(body.contains(
        "A person with no timeline heading.\n\n## Timeline\n\n- 2026-11-23 | test | hello\n"
    ));
    let fence = run(
        &vault,
        &["log", "fence", "2026-11-23 | test | hello", "--json"],
    );
    assert_eq!(fence.status.code(), Some(0), "{}", stderr(&fence));
    let fenced = fs::read_to_string(vault.join("people/fence.md")).unwrap();
    assert!(fenced.contains("```\n## Timeline\n```"));
    assert!(fenced
        .trim_end()
        .ends_with("## Timeline\n\n- 2026-11-23 | test | hello"));
}

#[test]
fn a_symlink_outside_the_vault_is_refused() {
    let vault = copy_vault();
    let outside = tempfile::tempdir().unwrap();
    let outside_note = outside.path().join("outside.md");
    fs::write(
        &outside_note,
        fs::read(vault.join("people/ada.md")).unwrap(),
    )
    .unwrap();
    fs::rename(
        vault.join("people/ada.md"),
        vault.join("people/ada.md.real"),
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside_note, vault.join("people/ada.md")).unwrap();
    let before = fs::read(&outside_note).unwrap();
    let output = run(
        &vault,
        &[
            "log",
            "people/ada.md",
            "2026-11-23 | test | hello",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["error"]["code"], "OUTSIDE_VAULT");
    assert_eq!(fs::read(&outside_note).unwrap(), before);
}

#[test]
fn twenty_processes_do_not_drop_an_ok_line() {
    let vault = copy_vault();
    let bin = cargo_bin("mnemic");
    let mut children = Vec::new();
    for index in 1..=20 {
        let text = format!("2026-01-{index:02} | p{index} | line {index}");
        let child = Command::new(&bin)
            .current_dir(&vault)
            .env("XDG_CONFIG_HOME", common::config_home())
            .env_remove("HOME")
            .args(["log", "ada", &text, "--json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");
        children.push((text, child));
    }
    let mut ok_lines = Vec::new();
    for (text, child) in children {
        let output = child.wait_with_output().expect("wait");
        let code = output.status.code();
        assert!(
            code == Some(0) || code == Some(75),
            "status {code:?} stderr {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if code == Some(0) {
            let json: Value = serde_json::from_slice(&output.stdout).expect("json");
            assert_eq!(json["ok"], true);
            ok_lines.push(format!("- {text}"));
        }
    }
    let body = fs::read_to_string(vault.join("people/ada.md")).unwrap();
    for line in ok_lines {
        assert!(body.contains(&line), "missing {line}\n{body}");
    }
}

#[test]
fn a_non_grammar_bullet_does_not_stop_date_order() {
    let vault = copy_vault();
    let path = vault.join("people/ada.md");
    let body = fs::read_to_string(&path).unwrap().replace(
        "- 2026-11-01 | archive | kept\n",
        "- not grammar\n- 2026-11-01 | archive | kept\n",
    );
    fs::write(&path, body).unwrap();
    let output = run(
        &vault,
        &["log", "ada", "2026-10-01 | old | older", "--json"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(
        bullets_after(&written, "## Timeline"),
        vec![
            "- not grammar",
            "- 2026-11-01 | archive | kept",
            "- 2026-10-01 | old | older",
        ]
    );

    let between = written.replace(
        "- 2026-11-01 | archive | kept\n- 2026-10-01 | old | older\n",
        "- 2026-11-01 | archive | kept\n- not grammar\n- 2026-09-01 | archive | early\n",
    );
    fs::write(&path, between).unwrap();
    let middle = run(
        &vault,
        &["log", "ada", "2026-10-01 | old | middle", "--json"],
    );
    assert_eq!(middle.status.code(), Some(0), "{}", stderr(&middle));
    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(
        bullets_after(&written, "## Timeline"),
        vec![
            "- not grammar",
            "- 2026-11-01 | archive | kept",
            "- not grammar",
            "- 2026-10-01 | old | middle",
            "- 2026-09-01 | archive | early",
        ]
    );
}

#[cfg(unix)]
#[test]
fn a_directory_symlink_outside_the_vault_is_not_indexed() {
    use std::os::unix::fs::symlink;

    let vault = copy_vault();
    let outside = tempfile::tempdir().unwrap();
    let outside_note = outside.path().join("ada.md");
    fs::copy(vault.join("people/ada.md"), &outside_note).unwrap();
    let before = fs::read(&outside_note).unwrap();
    symlink(outside.path(), vault.join("escape")).unwrap();

    let index = mnemic_core::index_vault(&vault).expect("index");
    match index.resolve("ada") {
        Ok(mnemic_core::Resolved::Note { path, .. }) => {
            assert_eq!(path, "people/ada.md");
        }
        other => panic!("expected the real note, got {other:?}"),
    }

    let output = run(
        &vault,
        &["log", "ada", "2026-10-01 | old | older", "--json"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(fs::read(&outside_note).unwrap(), before);
    assert!(fs::read_to_string(vault.join("people/ada.md"))
        .unwrap()
        .contains("- 2026-10-01 | old | older"));

    let mut check = common::mnemic();
    check.current_dir(&vault);
    let check = check.args(["check", "--json"]).output().expect("check");
    let stdout = String::from_utf8(check.stdout).unwrap();
    assert_eq!(check.status.code(), Some(0), "{stdout}");
    assert!(!stdout.contains("DUPLICATE_ID"), "{stdout}");
    assert!(!stdout.contains("escape/ada.md"), "{stdout}");
    assert!(!stdout.contains("escape"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn a_fifo_behind_a_file_symlink_is_not_opened() {
    let vault = copy_vault();
    let fifo = vault.join("pipe.fifo");
    mkfifo(&fifo);
    std::os::unix::fs::symlink(&fifo, vault.join("people/linked.md")).unwrap();
    assert_scan_finishes(&vault);
}

#[cfg(unix)]
#[test]
fn a_fifo_inside_an_outside_directory_symlink_is_not_opened() {
    let vault = copy_vault();
    let outside = tempfile::tempdir().unwrap();
    mkfifo(&outside.path().join("trap.md"));
    std::os::unix::fs::symlink(outside.path(), vault.join("escape")).unwrap();
    assert_scan_finishes(&vault);
}

#[cfg(unix)]
#[test]
fn a_fifo_in_the_vault_is_not_opened() {
    let vault = copy_vault();
    mkfifo(&vault.join("people/direct.md"));
    assert_scan_finishes(&vault);
}

#[cfg(unix)]
#[test]
fn parent_and_absolute_paths_are_outside_vault() {
    let vault = copy_vault();
    let outside = "/tmp/mnemic-outside-note.md";
    for query in ["../x.md", outside] {
        let output = run(
            &vault,
            &["log", query, "2026-11-23 | test | hello", "--json"],
        );
        assert_eq!(output.status.code(), Some(2), "{query}: {output:?}");
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["error"]["code"], "OUTSIDE_VAULT");
    }
}

#[cfg(unix)]
#[test]
fn log_on_a_symlink_to_an_outside_fifo_is_outside_vault() {
    let vault = copy_vault();
    let outside = tempfile::tempdir().unwrap();
    let fifo = outside.path().join("trap");
    mkfifo(&fifo);
    std::os::unix::fs::symlink(&fifo, vault.join("people/pipe.md")).unwrap();
    let mut command = common::mnemic();
    let output = command
        .current_dir(&vault)
        .args([
            "log",
            "people/pipe.md",
            "2026-11-23 | test | hello",
            "--json",
        ])
        .timeout(std::time::Duration::from_secs(2))
        .output()
        .expect("log returns before the fifo timeout");
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["error"]["code"], "OUTSIDE_VAULT");
}

fn mkfifo(path: &Path) {
    let status = Command::new("mkfifo").arg(path).status().expect("mkfifo");
    assert!(status.success(), "mkfifo {path:?}");
}

#[cfg(unix)]
fn assert_scan_finishes(vault: &Path) {
    use std::sync::mpsc;
    use std::time::Duration;

    let limit = Duration::from_secs(2);
    let indexed = vault.to_path_buf();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let loaded = mnemic_core::index_vault(&indexed);
        let _ = tx.send(loaded.map(|_| ()));
    });
    let loaded = rx.recv_timeout(limit).expect("index load hung");
    loaded.expect("index");

    let child = Command::new(cargo_bin("mnemic"))
        .current_dir(vault)
        .env("XDG_CONFIG_HOME", common::config_home())
        .env_remove("HOME")
        .args(["check", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn check");
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = match rx.recv_timeout(limit) {
        Ok(output) => output.expect("wait"),
        Err(_) => {
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            panic!("check hung");
        }
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn bullets_after<'a>(body: &'a str, heading: &str) -> Vec<&'a str> {
    body.split(heading)
        .nth(1)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
