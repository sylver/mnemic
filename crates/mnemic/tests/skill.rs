use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;
use clap::CommandFactory;
use mnemic::cli::Cli;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn skill_paths() -> Vec<PathBuf> {
    let root = repo_root().join("plugins/mnemic/skills");
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&root).unwrap_or_else(|err| panic!("skills dir: {err}")) {
        let entry = entry.expect("skill entry");
        let path = entry.path().join("SKILL.md");
        if path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    assert!(
        paths.len() >= 2,
        "expected the mnemic and shadow skills, found {paths:?}"
    );
    paths
}

fn skill_text() -> String {
    let path = repo_root().join("plugins/mnemic/skills/mnemic/SKILL.md");
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("SKILL.md: {err}"))
}

fn read_skill(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
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

/// Subcommands `Cli::command()` exposes, skipping clap's built-in `help`.
fn subcommands() -> Vec<String> {
    let names: Vec<String> = Cli::command()
        .get_subcommands()
        .map(|command| command.get_name().to_string())
        .filter(|name| name != "help")
        .collect();
    assert!(!names.is_empty(), "no subcommands on Cli::command()");
    names
}

fn long_flags(subcommand: &str) -> Vec<String> {
    let command = Cli::command();
    let sub = command
        .find_subcommand(subcommand)
        .unwrap_or_else(|| panic!("verb guide missing {subcommand}"));
    let mut flags = Vec::new();
    for arg in sub.get_arguments() {
        let Some(name) = arg.get_long() else {
            continue;
        };
        if !flags.iter().any(|flag| flag == name) {
            flags.push(name.to_string());
        }
    }
    flags
}

fn run(args: &[&str]) -> Output {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut command = Command::cargo_bin("mnemic").expect("mnemic binary");
    command.current_dir(dir.path());
    command.args(args).output().expect("run mnemic")
}

fn reference_text(name: &str) -> String {
    let path = repo_root().join("plugins/mnemic/skills/mnemic").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{name}: {err}"))
}

#[test]
fn skill_outside_a_vault_prints_skill_md() {
    let output = run(&["skill"]);
    assert!(output.status.success(), "status {:?}", output.status);
    assert_eq!(output.stdout, skill_text().as_bytes());
}

#[test]
fn skill_reference_types_json_matches_embedded_file() {
    let output = run(&["skill", "--reference", "types", "--json"]);
    assert!(output.status.success(), "status {:?}", output.status);
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["ok"], true);
    assert!(value["error"].is_null());
    assert_eq!(value["data"]["name"], "references/types.md");
    assert_eq!(value["data"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        value["data"]["content"],
        reference_text("references/types.md")
    );
    insta::assert_snapshot!(stdout);
}

#[test]
fn skill_unknown_reference_is_usage() {
    let output = run(&["skill", "--reference", "nope", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["ok"], false);
    assert!(value["data"].is_null());
    assert_eq!(value["error"]["code"], "USAGE");
    insta::assert_snapshot!(stdout);
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
    for path in skill_paths() {
        let (front, _) = split_skill(&read_skill(&path));
        let name = frontmatter_value(&front, "name");
        assert!(valid_skill_name(&name), "{} name {name}", path.display());
        let dir = repo_root().join("plugins/mnemic/skills").join(&name);
        assert!(
            dir.join("SKILL.md") == path,
            "name must match the skill directory"
        );
        let description = folded_description(&front);
        assert!(
            description.len() <= 1024,
            "{} description length {}",
            path.display(),
            description.len()
        );
        assert_eq!(frontmatter_value(&front, "license"), "MIT");
        let compatibility = frontmatter_value(&front, "compatibility");
        assert!(
            compatibility.len() <= 500,
            "{} compatibility length {}",
            path.display(),
            compatibility.len()
        );
    }
}

#[test]
fn version_matches_workspace_and_release_please_marker() {
    let version = workspace_version();
    for path in skill_paths() {
        let text = read_skill(&path);
        let (front, _) = split_skill(&text);
        let line = front
            .lines()
            .find(|line| line.contains("mnemic-version:"))
            .unwrap_or_else(|| panic!("{}: missing mnemic-version", path.display()));
        assert!(
            line.contains(&format!("\"{version}\"")),
            "{} mnemic-version must equal workspace {version}",
            path.display()
        );
        assert!(
            line.contains("# x-release-please-version"),
            "{} mnemic-version line must carry the release-please marker",
            path.display()
        );
    }
}

#[test]
fn body_stays_within_500_lines() {
    for path in skill_paths() {
        let (_, body) = split_skill(&read_skill(&path));
        let lines = body.lines().count();
        assert!(lines <= 500, "{} body has {lines} lines", path.display());
    }
}

#[test]
fn mnemic_body_omits_removed_surface() {
    let (_, body) = split_skill(&skill_text());
    for banned in ["--lock-timeout", "index", "migrate", "mcp", "visibility"] {
        assert!(
            !body.contains(banned),
            "body contains banned string {banned}"
        );
    }
}

/// Fenced blocks and inline backtick spans. A fence is consumed whole, so
/// backticks inside it are not a second span.
fn code_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let fence_at = rest.find("```");
        let tick_at = rest.find('`');
        let next = match (fence_at, tick_at) {
            (Some(fence), Some(tick)) => fence.min(tick),
            (Some(fence), None) => fence,
            (None, Some(tick)) => tick,
            (None, None) => break,
        };
        let at_line_start = next == 0 || rest.as_bytes().get(next - 1) == Some(&b'\n');
        if fence_at == Some(next) && at_line_start {
            let after = &rest[next + 3..];
            let Some(newline) = after.find('\n') else {
                break;
            };
            let body = &after[newline + 1..];
            let end = body.find("\n```").unwrap_or(body.len());
            spans.push(body[..end].to_string());
            let consumed = if body[end..].starts_with("\n```") {
                end + 4
            } else {
                end
            };
            rest = &body[consumed..];
            continue;
        }
        if tick_at == Some(next) {
            let after = &rest[next + 1..];
            let Some(end) = after.find('`') else {
                break;
            };
            spans.push(after[..end].to_string());
            rest = &after[end + 1..];
            continue;
        }
        rest = &rest[next + 1..];
    }
    spans
}

fn invocation_flag_errors(text: &str) -> Vec<String> {
    let mut errors = Vec::new();
    for span in code_spans(text) {
        for line in span.lines() {
            let mut rest = line;
            while let Some(index) = rest.find("mnemic") {
                let boundary = index == 0
                    || (!rest.as_bytes()[index - 1].is_ascii_alphanumeric()
                        && rest.as_bytes()[index - 1] != b'-');
                let after = &rest[index + "mnemic".len()..];
                rest = if after.is_empty() { "" } else { &after[1..] };
                if !boundary {
                    continue;
                }
                let Some(after) = after.strip_prefix(|ch: char| ch.is_whitespace()) else {
                    continue;
                };
                let Some(verb) = after.split_whitespace().next() else {
                    continue;
                };
                if verb.starts_with('-')
                    || !verb
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                {
                    continue;
                }
                let line_rest = after.split('\n').next().unwrap_or("");
                let mut flags = Vec::new();
                for token in line_rest.split_whitespace().skip(1) {
                    if token == "mnemic" || token.ends_with("mnemic") {
                        break;
                    }
                    let token = token
                        .trim_matches(|ch: char| matches!(ch, '`' | ',' | '[' | ']' | '(' | ')'));
                    let Some(name) = token.strip_prefix("--") else {
                        continue;
                    };
                    let name = name
                        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-')
                        .next()
                        .unwrap_or("");
                    if name.is_empty() || flags.iter().any(|flag| flag == name) {
                        continue;
                    }
                    flags.push(name.to_string());
                }
                if !subcommands().iter().any(|name| name == verb) {
                    errors.push(format!("{verb} is not a mnemic subcommand"));
                    continue;
                }
                let allowed = long_flags(verb);
                for flag in flags {
                    if !allowed.iter().any(|item| item == &flag) {
                        errors.push(format!("--{flag} is not a {verb} flag"));
                    }
                }
            }
        }
    }
    errors
}

#[test]
fn invented_flag_in_a_code_span_is_rejected() {
    let errors = invocation_flag_errors("See `mnemic set --nope`.\n");
    assert!(
        errors.iter().any(|error| error.contains("--nope")),
        "{errors:?}"
    );
}

#[test]
fn every_skill_code_span_names_real_flags() {
    for path in skill_paths() {
        let errors = invocation_flag_errors(&read_skill(&path));
        assert!(errors.is_empty(), "{}: {errors:?}", path.display());
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
