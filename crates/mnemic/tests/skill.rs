use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn skill_text() -> String {
    let path = repo_root().join("plugins/mnemic/skills/mnemic/SKILL.md");
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("SKILL.md: {err}"))
}

fn split_skill(text: &str) -> (String, String) {
    let rest = text
        .strip_prefix("---\n")
        .unwrap_or_else(|| panic!("SKILL.md: missing opening frontmatter"));
    let (front, body) = rest
        .split_once("\n---\n")
        .unwrap_or_else(|| panic!("SKILL.md: missing closing frontmatter"));
    (front.to_string(), body.to_string())
}

fn frontmatter_value(front: &str, key: &str) -> String {
    let prefix = format!("{key}:");
    for line in front.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix(&prefix) {
            return value.trim().trim_matches('"').to_string();
        }
    }
    panic!("SKILL.md: missing {key}");
}

fn folded_description(front: &str) -> String {
    let mut lines = front.lines();
    let header = lines
        .find(|line| line.trim() == "description: >-")
        .unwrap_or_else(|| panic!("SKILL.md: description must use >-"));
    let _ = header;
    let mut parts = Vec::new();
    for line in lines {
        if line.starts_with(' ') || line.starts_with('\t') {
            parts.push(line.trim());
        } else {
            break;
        }
    }
    parts.join(" ")
}

fn workspace_version() -> String {
    let cargo = std::fs::read_to_string(repo_root().join("Cargo.toml")).expect("Cargo.toml");
    let mut in_package = false;
    for line in cargo.lines() {
        if line.trim() == "[workspace.package]" {
            in_package = true;
            continue;
        }
        if in_package && line.starts_with('[') {
            break;
        }
        if in_package {
            if let Some(value) = line.trim().strip_prefix("version = ") {
                return value.trim_matches('"').to_string();
            }
        }
    }
    panic!("workspace package version");
}

fn valid_skill_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn mnemic(args: &[&str]) -> Output {
    let mut command = Command::cargo_bin("mnemic").expect("mnemic binary");
    command.args(args).output().expect("run mnemic")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8 stdout")
}

/// Subcommands `Cli::command()` exposes, skipping clap's built-in `help`.
fn subcommands() -> Vec<String> {
    let help = stdout(&mnemic(&["--help"]));
    let mut names = Vec::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if !in_commands {
            continue;
        }
        if line.starts_with("Options:") {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let name = trimmed.split_whitespace().next().expect("command name");
        if name != "help" {
            names.push(name.to_string());
        }
    }
    assert!(!names.is_empty(), "no subcommands in mnemic --help");
    names
}

fn long_flags(subcommand: &str) -> Vec<String> {
    let help = stdout(&mnemic(&[subcommand, "--help"]));
    let mut flags = Vec::new();
    for token in help.split_whitespace() {
        let token = token.trim_matches(',');
        if let Some(name) = token.strip_prefix("--") {
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                && !flags.iter().any(|flag| flag == name)
            {
                flags.push(name.to_string());
            }
        }
    }
    flags
}

fn verb_section<'a>(body: &'a str, name: &str) -> &'a str {
    let guide = section(body, "Verb guide");
    let marker = format!("### {name}\n");
    let start = guide
        .find(&marker)
        .unwrap_or_else(|| panic!("verb guide missing {name}"));
    let rest = &guide[start + marker.len()..];
    let end = rest.find("\n### ").unwrap_or(rest.len());
    &rest[..end]
}

fn section<'a>(body: &'a str, heading: &str) -> &'a str {
    let marker = format!("## {heading}\n");
    let start = body
        .find(&marker)
        .unwrap_or_else(|| panic!("SKILL.md: missing {heading}"));
    let rest = &body[start + marker.len()..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    &rest[..end]
}

fn synopsis_flags(section: &str) -> Vec<String> {
    let mut flags = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("Synopsis:") {
            continue;
        }
        for token in trimmed.split_whitespace() {
            let token = token.trim_matches('`').trim_matches(',');
            if let Some(name) = token.strip_prefix("--") {
                let name = name.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
                if !name.is_empty() && !flags.iter().any(|flag| flag == name) {
                    flags.push(name.to_string());
                }
            }
        }
    }
    flags
}

#[test]
fn frontmatter_matches_agent_skills_rules() {
    let (front, _) = split_skill(&skill_text());
    let name = frontmatter_value(&front, "name");
    assert!(valid_skill_name(&name), "name {name}");
    let dir = repo_root().join("plugins/mnemic/skills").join(&name);
    assert!(dir.is_dir(), "name must match the skill directory");
    let description = folded_description(&front);
    assert!(
        description.len() <= 1024,
        "description length {}",
        description.len()
    );
    let compatibility = frontmatter_value(&front, "compatibility");
    assert!(
        compatibility.len() <= 500,
        "compatibility length {}",
        compatibility.len()
    );
}

#[test]
fn version_matches_workspace_and_release_please_marker() {
    let text = skill_text();
    let (front, _) = split_skill(&text);
    let version = workspace_version();
    let line = front
        .lines()
        .find(|line| line.contains("mnemic-version:"))
        .unwrap_or_else(|| panic!("SKILL.md: missing mnemic-version"));
    assert!(
        line.contains(&format!("\"{version}\"")),
        "mnemic-version must equal workspace {version}"
    );
    assert!(
        line.contains("# x-release-please-version"),
        "mnemic-version line must carry the release-please marker"
    );
}

#[test]
fn body_stays_short_and_omits_removed_surface() {
    let (_, body) = split_skill(&skill_text());
    let lines = body.lines().count();
    assert!(lines < 500, "body has {lines} lines");
    for banned in ["--lock-timeout", "index", "migrate", "mcp", "visibility"] {
        assert!(
            !body.contains(banned),
            "body contains banned string {banned}"
        );
    }
}

#[test]
fn verb_guide_covers_every_subcommand_and_only_real_flags() {
    let (_, body) = split_skill(&skill_text());
    for name in subcommands() {
        let section = verb_section(&body, &name);
        assert!(
            section.contains(&format!("mnemic {name}")),
            "synopsis missing for {name}"
        );
        let allowed = long_flags(&name);
        for flag in synopsis_flags(section) {
            assert!(
                allowed.iter().any(|item| item == &flag),
                "--{flag} is not a {name} flag; clap has {allowed:?}"
            );
        }
    }
}
