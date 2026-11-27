mod common;

use serde_json::Value;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Output;

fn output(dir: &Path, args: &[&str], config: &Path) -> Output {
    let mut command = common::mnemic();
    command
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", config)
        .env_remove("HOME")
        .args(args)
        .output()
        .expect("run mnemic")
}

fn json_of(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("json")
}

fn init_vault(path: &Path, config: &Path) {
    let output = output(
        path,
        &[
            "init",
            "--as",
            "alice",
            "--vault",
            &path.display().to_string(),
            "--json",
        ],
        config,
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn redact(text: &str, pairs: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(&str, &str)> = pairs.to_vec();
    pairs.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    let mut text = text.to_string();
    for (from, to) in pairs {
        text = text.replace(from, to);
    }
    text
}

fn config_file(config: &Path) -> PathBuf {
    config.join("mnemic/config.toml")
}

#[test]
fn b1_use_binds_the_vault_and_b2_is_idempotent() {
    let config = tempfile::tempdir().expect("config");
    let outside = tempfile::tempdir().expect("outside");
    let vault = tempfile::tempdir().expect("vault");
    init_vault(vault.path(), config.path());
    let canonical = vault.path().canonicalize().unwrap().display().to_string();
    let config_path = config_file(config.path()).display().to_string();
    let first = output(
        outside.path(),
        &["use", &canonical, "--json"],
        config.path(),
    );
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    let body = json_of(&first);
    assert_eq!(body["data"]["changed"], true);
    assert_eq!(body["data"]["default"], canonical);
    let file = fs::read_to_string(config_file(config.path())).unwrap();
    assert_eq!(file, format!("vault = \"{canonical}\"\n"));
    let redacted = redact(
        &String::from_utf8(first.stdout).unwrap(),
        &[(&canonical, "<vault-a>"), (&config_path, "<config>")],
    );
    insta::assert_snapshot!("b1_use_json", redacted);

    let before = fs::metadata(config_file(config.path()))
        .unwrap()
        .modified()
        .unwrap();
    let second = output(
        outside.path(),
        &["use", &canonical, "--json"],
        config.path(),
    );
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    assert_eq!(json_of(&second)["data"]["changed"], false);
    let after = fs::metadata(config_file(config.path()))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn b3_search_outside_a_vault_uses_the_default() {
    let config = tempfile::tempdir().expect("config");
    let outside = tempfile::tempdir().expect("outside");
    let vault = tempfile::tempdir().expect("vault");
    init_vault(vault.path(), config.path());
    let created = output(
        vault.path(),
        &["new", "person", "Ada Lovelace", "--as", "alice", "--json"],
        config.path(),
    );
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    let bound = output(
        outside.path(),
        &["use", &vault.path().display().to_string(), "--json"],
        config.path(),
    );
    assert_eq!(bound.status.code(), Some(0), "{}", stderr(&bound));
    let found = output(outside.path(), &["search", "Ada", "--json"], config.path());
    assert_eq!(found.status.code(), Some(0), "{}", stderr(&found));
    let hits = &json_of(&found)["data"]["hits"];
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["title"], "Ada Lovelace");
}

#[test]
fn b4_directory_walk_beats_the_default() {
    let config = tempfile::tempdir().expect("config");
    let vault_a = tempfile::tempdir().expect("a");
    let vault_b = tempfile::tempdir().expect("b");
    init_vault(vault_a.path(), config.path());
    init_vault(vault_b.path(), config.path());
    let bound = output(
        vault_a.path(),
        &["use", &vault_a.path().display().to_string(), "--json"],
        config.path(),
    );
    assert_eq!(bound.status.code(), Some(0), "{}", stderr(&bound));
    let people = vault_b.path().join("people");
    let created = output(
        &people,
        &["new", "person", "Eve", "--as", "alice", "--json"],
        config.path(),
    );
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    assert!(people.join("eve.md").is_file());
    assert!(!vault_a.path().join("people/eve.md").exists());
}

#[test]
fn b5_show_reports_cwd_over_the_default() {
    let config = tempfile::tempdir().expect("config");
    let vault_a = tempfile::tempdir().expect("a");
    let vault_b = tempfile::tempdir().expect("b");
    init_vault(vault_a.path(), config.path());
    init_vault(vault_b.path(), config.path());
    let canonical_a = vault_a.path().canonicalize().unwrap().display().to_string();
    let canonical_b = vault_b.path().canonicalize().unwrap().display().to_string();
    let config_path = config_file(config.path()).display().to_string();
    let bound = output(
        vault_a.path(),
        &["use", &canonical_a, "--json"],
        config.path(),
    );
    assert_eq!(bound.status.code(), Some(0), "{}", stderr(&bound));
    let shown = output(
        &vault_b.path().join("people"),
        &["use", "--json"],
        config.path(),
    );
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
    let body = json_of(&shown);
    assert_eq!(body["data"]["vault"], canonical_b);
    assert_eq!(body["data"]["source"], "cwd");
    assert_eq!(body["data"]["default"], canonical_a);
    assert_eq!(body["data"]["changed"], false);
    let redacted = redact(
        &String::from_utf8(shown.stdout).unwrap(),
        &[
            (&canonical_b, "<vault-b>"),
            (&canonical_a, "<vault-a>"),
            (&config_path, "<config>"),
        ],
    );
    insta::assert_snapshot!("b5_use_json", redacted);
}

#[test]
fn b6_use_dot_binds_the_root() {
    let config = tempfile::tempdir().expect("config");
    let vault = tempfile::tempdir().expect("vault");
    init_vault(vault.path(), config.path());
    let canonical = vault.path().canonicalize().unwrap().display().to_string();
    let bound = output(
        &vault.path().join("people"),
        &["use", ".", "--json"],
        config.path(),
    );
    assert_eq!(bound.status.code(), Some(0), "{}", stderr(&bound));
    assert_eq!(json_of(&bound)["data"]["default"], canonical);
    assert!(!canonical.ends_with("/people"));
}

#[test]
fn b7_clear_is_idempotent_and_b7b_creates_nothing() {
    let config = tempfile::tempdir().expect("config");
    let vault = tempfile::tempdir().expect("vault");
    init_vault(vault.path(), config.path());
    let outside = tempfile::tempdir().expect("outside");
    let bound = output(
        outside.path(),
        &["use", &vault.path().display().to_string(), "--json"],
        config.path(),
    );
    assert_eq!(bound.status.code(), Some(0), "{}", stderr(&bound));
    let first = output(outside.path(), &["use", "--clear", "--json"], config.path());
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    assert_eq!(json_of(&first)["data"]["changed"], true);
    assert!(json_of(&first)["data"]["default"].is_null());
    assert_eq!(fs::read_to_string(config_file(config.path())).unwrap(), "");
    let second = output(outside.path(), &["use", "--clear", "--json"], config.path());
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    assert_eq!(json_of(&second)["data"]["changed"], false);
    assert_eq!(fs::read_to_string(config_file(config.path())).unwrap(), "");

    let empty = tempfile::tempdir().expect("empty config");
    let cleared = output(outside.path(), &["use", "--clear", "--json"], empty.path());
    assert_eq!(cleared.status.code(), Some(0), "{}", stderr(&cleared));
    assert_eq!(json_of(&cleared)["data"]["changed"], false);
    assert!(!empty.path().join("mnemic").exists());
}

#[test]
fn b8_missing_default_toml_is_vault_not_found() {
    let config = tempfile::tempdir().expect("config");
    let outside = tempfile::tempdir().expect("outside");
    let vault = tempfile::tempdir().expect("vault");
    init_vault(vault.path(), config.path());
    let canonical = vault.path().canonicalize().unwrap().display().to_string();
    let config_path = config_file(config.path()).display().to_string();
    let bound = output(
        outside.path(),
        &["use", &canonical, "--json"],
        config.path(),
    );
    assert_eq!(bound.status.code(), Some(0), "{}", stderr(&bound));
    fs::remove_file(vault.path().join("mnemic.toml")).unwrap();
    let got = output(outside.path(), &["get", "x", "--json"], config.path());
    assert_eq!(got.status.code(), Some(2), "{}", stderr(&got));
    let body = json_of(&got);
    assert_eq!(body["error"]["code"], "VAULT_NOT_FOUND");
    assert_eq!(body["error"]["details"]["default"], canonical);
    assert_eq!(body["error"]["details"]["config"], config_path);
    let searched = outside.path().canonicalize().unwrap().display().to_string();
    let redacted = redact(
        &String::from_utf8(got.stdout).unwrap(),
        &[
            (&canonical, "<vault-a>"),
            (&config_path, "<config>"),
            (&searched, "<cwd>"),
        ],
    );
    insta::assert_snapshot!("b8_stale_default", redacted);
}

#[test]
fn b9_unknown_key_is_bad_config_and_b10_is_not_read_inside_a_vault() {
    let config = tempfile::tempdir().expect("config");
    let outside = tempfile::tempdir().expect("outside");
    let vault = tempfile::tempdir().expect("vault");
    init_vault(vault.path(), config.path());
    fs::create_dir_all(config.path().join("mnemic")).unwrap();
    let config_path = config_file(config.path());
    fs::write(&config_path, "vualt = \"/x\"\n").unwrap();
    let broken = output(outside.path(), &["search", "x", "--json"], config.path());
    assert_eq!(broken.status.code(), Some(2), "{}", stderr(&broken));
    let body = json_of(&broken);
    assert_eq!(body["error"]["code"], "BAD_CONFIG");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.starts_with(&config_path.display().to_string()));
    let searched = outside.path().canonicalize().unwrap().display().to_string();
    let redacted = redact(
        &String::from_utf8(broken.stdout).unwrap(),
        &[
            (&config_path.display().to_string(), "<config>"),
            (&searched, "<cwd>"),
        ],
    );
    insta::assert_snapshot!("b9_bad_config", redacted);

    let inside = output(vault.path(), &["search", "Ada", "--json"], config.path());
    assert_eq!(inside.status.code(), Some(0), "{}", stderr(&inside));
    assert_eq!(json_of(&inside)["ok"], true);
}

#[test]
fn b11_use_missing_vault_leaves_config_untouched() {
    let config = tempfile::tempdir().expect("config");
    let empty = tempfile::tempdir().expect("empty");
    let target = empty.path().join("x");
    fs::create_dir(&target).unwrap();
    let output = output(
        empty.path(),
        &["use", &target.display().to_string(), "--json"],
        config.path(),
    );
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let body = json_of(&output);
    assert_eq!(body["error"]["code"], "VAULT_NOT_FOUND");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains(&target.display().to_string()));
    assert!(!config.path().join("mnemic").exists());
}

#[test]
fn b12_symlink_config_is_rewritten_in_the_target_directory() {
    let config = tempfile::tempdir().expect("config");
    let vault = tempfile::tempdir().expect("vault");
    init_vault(vault.path(), config.path());
    let dotfiles = tempfile::tempdir().expect("dotfiles");
    let target = dotfiles.path().join("mnemic.toml");
    fs::write(&target, "vault = \"/old\"\n").unwrap();
    let link_dir = config.path().join("mnemic");
    fs::create_dir_all(&link_dir).unwrap();
    symlink(&target, link_dir.join("config.toml")).unwrap();
    let mut perms = fs::metadata(&link_dir).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&link_dir, perms).unwrap();

    let output = output(
        vault.path(),
        &["use", &vault.path().display().to_string()],
        config.path(),
    );
    let mut perms = fs::metadata(&link_dir).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(&link_dir, perms).unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(link_dir
        .join("config.toml")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
    let canonical = vault.path().canonicalize().unwrap().display().to_string();
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        format!("vault = \"{canonical}\"\n")
    );
    assert!(fs::read_dir(dotfiles.path())
        .unwrap()
        .all(|entry| entry.unwrap().file_name() == "mnemic.toml"));
}

#[test]
fn b12b_dangling_symlink_creates_the_target() {
    let config = tempfile::tempdir().expect("config");
    let vault = tempfile::tempdir().expect("vault");
    init_vault(vault.path(), config.path());
    let dotfiles = config.path().join("dotfiles");
    fs::create_dir_all(&dotfiles).unwrap();
    let target = dotfiles.join("mnemic.toml");
    let link_dir = config.path().join("mnemic");
    fs::create_dir_all(&link_dir).unwrap();
    symlink(&target, link_dir.join("config.toml")).unwrap();
    assert!(!target.exists());
    let output = output(
        vault.path(),
        &["use", &vault.path().display().to_string(), "--json"],
        config.path(),
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(json_of(&output)["data"]["changed"].as_bool().unwrap());
    assert!(link_dir
        .join("config.toml")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(target.is_file());
    let canonical = vault.path().canonicalize().unwrap().display().to_string();
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        format!("vault = \"{canonical}\"\n")
    );
}

#[test]
fn b13_set_without_home_or_xdg_is_usage() {
    let vault = tempfile::tempdir().expect("vault");
    let config = tempfile::tempdir().expect("config");
    init_vault(vault.path(), config.path());
    let mut command = common::mnemic();
    let output = command
        .current_dir(vault.path())
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("HOME")
        .args(["use", &vault.path().display().to_string(), "--json"])
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let body = json_of(&output);
    assert_eq!(body["error"]["code"], "USAGE");
    assert_eq!(body["error"]["message"], "set HOME or XDG_CONFIG_HOME");
}

#[test]
fn b14_check_uses_the_default_but_a_positional_path_does_not() {
    let config = tempfile::tempdir().expect("config");
    let outside = tempfile::tempdir().expect("outside");
    let vault = tempfile::tempdir().expect("vault");
    init_vault(vault.path(), config.path());
    let bound = output(
        outside.path(),
        &["use", &vault.path().display().to_string(), "--json"],
        config.path(),
    );
    assert_eq!(bound.status.code(), Some(0), "{}", stderr(&bound));
    let checked = output(outside.path(), &["check", "--json"], config.path());
    assert_eq!(checked.status.code(), Some(0), "{}", stderr(&checked));
    let elsewhere = tempfile::tempdir().expect("elsewhere");
    let missed = output(
        outside.path(),
        &["check", &elsewhere.path().display().to_string(), "--json"],
        config.path(),
    );
    assert_eq!(missed.status.code(), Some(2), "{}", stderr(&missed));
    assert_eq!(json_of(&missed)["error"]["code"], "VAULT_NOT_FOUND");
    assert!(json_of(&missed)["error"]["details"]
        .get("default")
        .is_none());
}

#[test]
fn b15_init_does_not_change_the_default() {
    let config = tempfile::tempdir().expect("config");
    let vault = tempfile::tempdir().expect("vault");
    let fresh = tempfile::tempdir().expect("fresh");
    init_vault(vault.path(), config.path());
    let canonical = vault.path().canonicalize().unwrap().display().to_string();
    let bound = output(vault.path(), &["use", &canonical, "--json"], config.path());
    assert_eq!(bound.status.code(), Some(0), "{}", stderr(&bound));
    let created = output(
        fresh.path(),
        &[
            "init",
            "--as",
            "alice",
            "--vault",
            &fresh.path().display().to_string(),
            "--json",
        ],
        config.path(),
    );
    assert_eq!(created.status.code(), Some(0), "{}", stderr(&created));
    assert_eq!(
        fs::read_to_string(config_file(config.path())).unwrap(),
        format!("vault = \"{canonical}\"\n")
    );
}

#[test]
fn use_path_and_clear_conflict_and_a_missing_path_is_io() {
    let config = tempfile::tempdir().expect("config");
    let dir = tempfile::tempdir().expect("dir");
    let conflict = output(
        dir.path(),
        &["use", dir.path().to_str().unwrap(), "--clear"],
        config.path(),
    );
    assert_eq!(conflict.status.code(), Some(2));
    let missing = dir.path().join("missing");
    let output = output(
        dir.path(),
        &["use", &missing.display().to_string(), "--json"],
        config.path(),
    );
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(json_of(&output)["error"]["code"], "IO");
}

#[test]
fn non_utf8_path_is_usage() {
    let config = tempfile::tempdir().expect("config");
    let dir = tempfile::tempdir().expect("dir");
    let path = std::ffi::OsString::from_vec(vec![0xff, 0xfe]);
    let mut command = common::mnemic();
    let output = command
        .current_dir(dir.path())
        .env("XDG_CONFIG_HOME", config.path())
        .env_remove("HOME")
        .arg("use")
        .arg(path)
        .arg("--json")
        .output()
        .expect("run");
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(
        json_of(&output)["error"]["message"],
        "vault path must be UTF-8"
    );
}

#[test]
fn show_outside_with_no_default_exits_zero() {
    let config = tempfile::tempdir().expect("config");
    let outside = tempfile::tempdir().expect("outside");
    let output = output(outside.path(), &["use", "--json"], config.path());
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let body = json_of(&output);
    assert!(body["data"]["vault"].is_null());
    assert!(body["data"]["source"].is_null());
    assert!(body["data"]["default"].is_null());
    assert_eq!(body["data"]["changed"], false);
}
