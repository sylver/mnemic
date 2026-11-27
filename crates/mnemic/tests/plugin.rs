use std::path::{Path, PathBuf};

use serde_json::Value;

const AGENT_KEYS: &[&str] = &[
    "$schema",
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
];

const HARNESS_KEYS: &[&str] = &[
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(path: &Path) -> Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn require_str<'a>(value: &'a Value, key: &str, path: &Path) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{}: missing {key}", path.display()))
}

fn closed_keys(value: &Value, allowed: &[&str], path: &Path) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{}: expected object", path.display()));
    for key in object.keys() {
        assert!(
            allowed.contains(&key.as_str()),
            "{}: unexpected key {key}",
            path.display()
        );
    }
}

fn assert_version(value: &Value, path: &Path) {
    let version = require_str(value, "version", path);
    assert_eq!(
        version,
        env!("CARGO_PKG_VERSION"),
        "{}: version {version} != {}",
        path.display(),
        env!("CARGO_PKG_VERSION")
    );
}

fn plugin_entries<'a>(value: &'a Value, path: &Path) -> &'a Vec<Value> {
    value
        .get("plugins")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{}: missing plugins", path.display()))
}

fn entry_source(entry: &Value, path: &Path) -> String {
    let source = entry
        .get("source")
        .unwrap_or_else(|| panic!("{}: missing source", path.display()));
    if let Some(text) = source.as_str() {
        return text.to_string();
    }
    source
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{}: missing source path", path.display()))
        .to_string()
}

fn assert_source_exists(source: &str, path: &Path) {
    assert!(
        !source.split(['/', '\\']).any(|part| part == ".."),
        "{source} contains .."
    );
    let relative = source.strip_prefix("./").unwrap_or(source);
    let target = repo_root().join(relative);
    assert!(
        target.is_dir(),
        "{source} does not exist ({})",
        path.display()
    );
}

#[test]
fn manifests_and_marketplaces_match_the_contract() {
    let root = repo_root();
    let agent = root.join("plugins/mnemic/plugin.json");
    let cursor = root.join("plugins/mnemic/.cursor-plugin/plugin.json");
    let claude = root.join("plugins/mnemic/.claude-plugin/plugin.json");
    let claude_market = root.join(".claude-plugin/marketplace.json");
    let cursor_market = root.join(".cursor-plugin/marketplace.json");
    let codex_market = root.join(".agents/plugins/marketplace.json");

    let files = [
        &agent,
        &cursor,
        &claude,
        &claude_market,
        &cursor_market,
        &codex_market,
    ];
    let parsed: Vec<Value> = files.iter().map(|path| read_json(path)).collect();

    closed_keys(&parsed[0], AGENT_KEYS, &agent);
    assert!(
        parsed[0].get("$schema").and_then(Value::as_str).is_some(),
        "{}: missing $schema",
        agent.display()
    );
    closed_keys(&parsed[1], HARNESS_KEYS, &cursor);
    closed_keys(&parsed[2], HARNESS_KEYS, &claude);

    let mut names = Vec::new();
    for (value, path) in parsed.iter().take(3).zip([&agent, &cursor, &claude]) {
        names.push(require_str(value, "name", path).to_string());
        assert_version(value, path);
    }

    for (value, path) in parsed
        .iter()
        .skip(3)
        .zip([&claude_market, &cursor_market, &codex_market])
    {
        names.push(require_str(value, "name", path).to_string());
        assert!(
            value.get("owner").is_some(),
            "{}: missing owner",
            path.display()
        );
        assert!(
            value.get("plugins").is_some(),
            "{}: missing plugins",
            path.display()
        );
        for entry in plugin_entries(value, path) {
            assert!(
                entry.get("version").is_none(),
                "{}: marketplace entry has version",
                path.display()
            );
            names.push(require_str(entry, "name", path).to_string());
            let source = entry_source(entry, path);
            assert_source_exists(&source, path);
        }
    }

    assert!(
        names.iter().all(|name| name == "mnemic"),
        "names must all be mnemic: {names:?}"
    );

    let claude_source = entry_source(
        &plugin_entries(&parsed[3], &claude_market)[0],
        &claude_market,
    );
    let cursor_source = entry_source(
        &plugin_entries(&parsed[4], &cursor_market)[0],
        &cursor_market,
    );
    let codex_entry = &plugin_entries(&parsed[5], &codex_market)[0];
    let codex_path = codex_entry["source"]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("{}: missing source path", codex_market.display()));
    assert_eq!(claude_source, "./plugins/mnemic");
    assert_eq!(cursor_source, "plugins/mnemic");
    assert_eq!(codex_path, "./plugins/mnemic");
    assert_eq!(codex_entry["source"]["source"], "local");
}
