mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dest");
    for entry in fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let dest = to.join(entry.file_name());
        let kind = entry.file_type().expect("file type");
        if kind.is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).expect("copy file");
        }
    }
}

fn fixture_vault(with_types: bool) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    let fixture = repo().join("fixtures/shadow");
    assert!(
        !fixture.join("types").exists(),
        "fixtures/shadow must not ship a types directory"
    );
    copy_tree(&fixture, dir.path());
    if with_types {
        let assets = repo().join("plugins/mnemic/skills/shadow/assets/types");
        copy_tree(&assets, &dir.path().join("types"));
    }
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    let mut command = common::mnemic();
    command.current_dir(dir);
    command.args(args).output().expect("run mnemic")
}

fn json_at(dir: &Path, args: &[&str], code: i32) -> Value {
    let output = run(dir, args);
    assert_eq!(
        output.status.code(),
        Some(code),
        "{args:?} stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json stdout")
}

fn error_codes(body: &Value) -> Vec<String> {
    body["data"]["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|item| item["code"].as_str().unwrap_or("").to_string())
        .collect()
}

fn paths_for(body: &Value, code: &str) -> Vec<String> {
    body["data"]["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .filter(|item| item["code"] == code)
        .map(|item| item["path"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn b1_fixture_plus_asset_types_checks_clean() {
    let dir = fixture_vault(true);
    let body = json_at(dir.path(), &["check", "--json"], 0);
    assert_eq!(body["ok"], true);
    assert!(error_codes(&body).is_empty());
}

#[test]
fn b2_deleted_cited_source_is_broken_link_on_the_shadow() {
    let dir = fixture_vault(true);
    fs::remove_file(dir.path().join("sources/ada-example-letter.md")).expect("delete source");
    let body = json_at(dir.path(), &["check", "--json"], 2);
    assert_eq!(body["error"]["code"], "CHECK_FAILED");
    let paths = paths_for(&body, "BROKEN_LINK");
    assert!(
        paths
            .iter()
            .any(|path| path == "shadows/shadow-ada-example.md"),
        "{paths:?}"
    );
}

#[test]
fn b3_channel_without_locator_is_missing_field() {
    let dir = fixture_vault(true);
    let path = dir.path().join("channels/x-adaexample.md");
    let text = fs::read_to_string(&path).expect("channel");
    let next = text.replace("locator: \"x:adaexample\"\n", "");
    assert_ne!(text, next, "locator line missing from fixture");
    fs::write(&path, next).expect("rewrite channel");
    let body = json_at(dir.path(), &["check", "--json"], 2);
    assert_eq!(body["error"]["code"], "CHECK_FAILED");
    let paths = paths_for(&body, "MISSING_FIELD");
    assert!(
        paths.iter().any(|path| path == "channels/x-adaexample.md"),
        "{paths:?}"
    );
}

#[test]
fn b4_changed_raw_byte_is_sha256_drift() {
    let dir = fixture_vault(true);
    let path = dir
        .path()
        .join("sources/raw/shadows/ada-example/x-adaexample/letter.txt");
    let mut bytes = fs::read(&path).expect("raw");
    bytes[0] ^= 0x01;
    fs::write(&path, bytes).expect("rewrite raw");
    let body = json_at(dir.path(), &["check", "--json"], 2);
    assert_eq!(body["error"]["code"], "CHECK_FAILED");
    assert!(
        error_codes(&body).iter().any(|code| code == "SHA256_DRIFT"),
        "{body}"
    );
}

#[test]
fn b5_new_shadow_writes_the_seven_headings_and_checks() {
    let dir = fixture_vault(true);
    let created = run(
        dir.path(),
        &[
            "new",
            "shadow",
            "Shadow: Bob Example",
            "--as",
            "t",
            "--json",
        ],
    );
    assert_eq!(
        created.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let note = dir.path().join("shadows/shadow-bob-example.md");
    let text = fs::read_to_string(&note).expect("created shadow");
    for heading in [
        "## Frame",
        "## Questions",
        "## Tests",
        "## Refusals",
        "## Distinctions",
        "## Updates",
        "## Claims",
    ] {
        assert!(text.contains(heading), "missing {heading}\n{text}");
    }
    let body = json_at(dir.path(), &["check", "--json"], 0);
    assert_eq!(body["ok"], true);
}

#[test]
fn install_copy_create_set_log_checks_clean() {
    let dir = fixture_vault(false);
    let assets = repo().join("plugins/mnemic/skills/shadow/assets/types");
    copy_tree(&assets, &dir.path().join("types"));

    let created = run(
        dir.path(),
        &[
            "new",
            "shadow",
            "Shadow: Bob Example",
            "--as",
            "t",
            "--json",
        ],
    );
    assert_eq!(
        created.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    let channel = run(
        dir.path(),
        &[
            "new",
            "channel",
            "x:bobexample",
            "--as",
            "t",
            "--field",
            "shadow=shadow-bob-example",
            "--field",
            "locator=x:bobexample",
            "--field",
            "url=https://x.com/bobexample",
            "--json",
        ],
    );
    assert_eq!(
        channel.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&channel.stderr)
    );

    let raw_rel = "sources/raw/shadows/bob-example/x-bobexample/2026-11-26.jsonl";
    let raw = dir.path().join(raw_rel);
    fs::create_dir_all(raw.parent().expect("raw parent")).expect("raw dir");
    fs::write(&raw, "{\"id\":\"1\",\"time\":\"2026-11-26T00:00:00Z\",\"url\":\"https://x.com/bobexample/status/1\",\"text\":\"hello\"}\n").expect("raw");
    let digest = sha256_hex(&raw);

    let source = run(
        dir.path(),
        &[
            "new",
            "source",
            "x:bobexample 2026-11-26",
            "--as",
            "t",
            "--field",
            "url=https://x.com/bobexample/status/1",
            "--field",
            &format!("path={raw_rel}"),
            "--field",
            &format!("sha256={digest}"),
            "--field",
            "tags=shadow/bob-example",
            "--field",
            "tags=domain/example",
            "--json",
        ],
    );
    assert_eq!(
        source.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );

    let updated = run(
        dir.path(),
        &[
            "set",
            "x-bobexample",
            "--field",
            "cursor=2026-11-26",
            "--field",
            "last_run_at=2026-11-26",
            "--field",
            "state=ready",
            "--json",
        ],
    );
    assert_eq!(
        updated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&updated.stderr)
    );

    let logged = run(
        dir.path(),
        &[
            "log",
            "x-bobexample",
            "2026-11-26 | x:bobexample | stored 1 items, cursor 2026-11-26",
            "--json",
        ],
    );
    assert_eq!(
        logged.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&logged.stderr)
    );

    let body = json_at(dir.path(), &["check", "--json"], 0);
    assert_eq!(body["ok"], true);
    assert!(error_codes(&body).is_empty(), "{body}");
}

fn sha256_hex(path: &Path) -> String {
    let output = std::process::Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum");
    assert!(output.status.success(), "sha256sum failed");
    String::from_utf8(output.stdout)
        .expect("utf-8")
        .split_whitespace()
        .next()
        .expect("digest")
        .to_string()
}

fn frontmatter_tags(text: &str) -> Vec<String> {
    let Some(rest) = text.strip_prefix("---\n") else {
        return Vec::new();
    };
    let Some((front, _)) = rest.split_once("\n---\n") else {
        return Vec::new();
    };
    for line in front.lines() {
        let Some(value) = line.trim().strip_prefix("tags:") else {
            continue;
        };
        let value = value.trim();
        let Some(inner) = value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        else {
            continue;
        };
        return inner
            .split(',')
            .map(|item| item.trim().trim_matches('"').to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }
    Vec::new()
}

fn data_hits(body: &Value) -> Vec<Value> {
    body["data"]["hits"].as_array().expect("hits").clone()
}

/// Domain members the council skill would keep: tagged shadows, a `domains`
/// entry, and `shadow/<slug>` tags on sources in that domain.
fn domain_members(dir: &Path, domain: &str) -> Vec<String> {
    let mut slugs = std::collections::BTreeSet::new();
    let tag = format!("domain/{domain}");
    let tagged = json_at(
        dir,
        &[
            "search", "shadow", "--type", "shadow", "--tag", &tag, "--limit", "10000", "--json",
        ],
        0,
    );
    for hit in data_hits(&tagged) {
        slugs.insert(hit["path"].as_str().expect("path").to_string());
    }
    let shadows = json_at(
        dir,
        &[
            "search", "shadow", "--type", "shadow", "--limit", "10000", "--json",
        ],
        0,
    );
    for hit in data_hits(&shadows) {
        let query = hit["path"].as_str().expect("path");
        let note = json_at(dir, &["get", query, "--json"], 0);
        let domains = note["data"]["frontmatter"]["domains"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if domains.iter().any(|item| item.as_str() == Some(domain)) {
            slugs.insert(query.to_string());
        }
    }
    let sources_dir = dir.join("sources");
    for entry in fs::read_dir(&sources_dir).expect("sources") {
        let entry = entry.expect("source entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("source note");
        let tags = frontmatter_tags(&text);
        if !tags.iter().any(|item| item == &tag) {
            continue;
        }
        for item in tags {
            let Some(slug) = item.strip_prefix("shadow/") else {
                continue;
            };
            let shadow = format!("shadow-{slug}");
            let member = json_at(dir, &["get", &shadow, "--json"], 0);
            if member["data"]["type"] == "shadow" {
                slugs.insert(member["data"]["slug"].as_str().expect("slug").to_string());
            }
        }
    }
    // Paths from the tagged search are not slugs yet.
    let mut members = std::collections::BTreeSet::new();
    for key in slugs {
        let note = json_at(dir, &["get", &key, "--json"], 0);
        assert_eq!(note["data"]["type"], "shadow");
        members.insert(note["data"]["slug"].as_str().expect("slug").to_string());
    }
    members.into_iter().collect()
}

#[test]
fn domain_council_finds_domains_field_and_source_tags() {
    let dir = fixture_vault(true);
    let set_ada = run(
        dir.path(),
        &[
            "set",
            "shadow-ada-example",
            "--field",
            "domains=engines",
            "--json",
        ],
    );
    assert_eq!(
        set_ada.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&set_ada.stderr)
    );
    let bob = run(
        dir.path(),
        &[
            "new",
            "shadow",
            "Shadow: Bob Example",
            "--as",
            "t",
            "--json",
        ],
    );
    assert_eq!(
        bob.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&bob.stderr)
    );
    let cara = run(
        dir.path(),
        &[
            "new",
            "shadow",
            "Shadow: Cara Example",
            "--as",
            "t",
            "--json",
        ],
    );
    assert_eq!(
        cara.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&cara.stderr)
    );
    let source = run(
        dir.path(),
        &[
            "new",
            "source",
            "Bob engine note",
            "--as",
            "t",
            "--url",
            "https://example.com/bob",
            "--field",
            "tags=shadow/bob-example",
            "--field",
            "tags=domain/engines",
            "--json",
        ],
    );
    assert_eq!(
        source.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let created: Value = serde_json::from_slice(&source.stdout).expect("source json");
    let source_rel = created["data"]["path"].as_str().expect("path");
    let source_path = dir.path().join(source_rel);
    let source_text = fs::read_to_string(&source_path).expect("source note");
    let (front, _) = source_text.split_once("\n---\n").expect("frontmatter");
    let body = "The engine is unfinished.\n";
    assert!(!body.to_lowercase().contains("source"));
    assert!(!source_rel
        .rsplit('/')
        .next()
        .unwrap_or(source_rel)
        .to_lowercase()
        .contains("source"));
    fs::write(&source_path, format!("{front}\n---\n{body}")).expect("rewrite body");
    let missed = json_at(
        dir.path(),
        &[
            "search",
            "source",
            "--type",
            "source",
            "--tag",
            "domain/engines",
            "--limit",
            "10000",
            "--json",
        ],
        0,
    );
    assert!(
        data_hits(&missed)
            .iter()
            .all(|hit| hit["path"] != source_rel),
        "search must miss the edited source: {missed}"
    );

    let capped = json_at(
        dir.path(),
        &[
            "search", "shadow", "--type", "shadow", "--limit", "1", "--json",
        ],
        0,
    );
    assert_eq!(
        data_hits(&capped).len(),
        1,
        "a short page is not the whole shadow set"
    );

    let members = domain_members(dir.path(), "engines");
    assert_eq!(
        members,
        vec![
            "shadow-ada-example".to_string(),
            "shadow-bob-example".to_string()
        ],
        "{members:?}"
    );
}

#[test]
fn council_record_links_both_shadows_and_logs() {
    let dir = fixture_vault(true);
    let created = run(
        dir.path(),
        &[
            "new",
            "shadow",
            "Shadow: Bob Example",
            "--as",
            "t",
            "--json",
        ],
    );
    assert_eq!(
        created.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    let question = "Is the engine unfinished";
    let council = run(
        dir.path(),
        &[
            "new",
            "note",
            &format!("Council: {question}"),
            "--as",
            "t",
            "--field",
            "tags=council",
            "--json",
        ],
    );
    assert_eq!(
        council.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&council.stderr)
    );
    let created_body: Value = serde_json::from_slice(&council.stdout).expect("council json");
    let rel = created_body["data"]["path"].as_str().expect("path");
    assert!(
        rel.ends_with("council-is-the-engine-unfinished.md"),
        "{rel}"
    );
    let note_path = dir.path().join(rel);
    let note = fs::read_to_string(&note_path).expect("council note");
    assert!(note.contains("tags:") && note.contains("council"), "{note}");
    let body = "\
## Question

Is the engine unfinished

## Positions

### Ada Example

The letter says the engine is unfinished [[ada-example-letter]].

### Bob Example

The letter says the engine is unfinished [[ada-example-letter]].

## Agreements

- The engine is unfinished ([[shadow-ada-example]], [[shadow-bob-example]]).

## Disagreements

- No disagreement.

## Cruxes

- No crux yet.

## What would move each shadow

- [[shadow-ada-example]]: a later source that says the engine shipped.
- [[shadow-bob-example]]: a later source that says the engine shipped.

## Extrapolation flags

- None.
";
    let (front, _) = note.split_once("\n---\n").expect("frontmatter");
    fs::write(&note_path, format!("{front}\n---\n{body}")).expect("write body");

    let line =
        "2026-11-27 | [[council-is-the-engine-unfinished]] | council: Is the engine unfinished";
    for shadow in ["shadow-ada-example", "shadow-bob-example"] {
        let logged = run(dir.path(), &["log", shadow, line, "--json"]);
        assert_eq!(
            logged.status.code(),
            Some(0),
            "{shadow} {}",
            String::from_utf8_lossy(&logged.stderr)
        );
    }

    let checked = json_at(dir.path(), &["check", "--json"], 0);
    assert_eq!(checked["ok"], true);
    assert!(error_codes(&checked).is_empty(), "{checked}");

    for shadow in ["shadow-ada-example", "shadow-bob-example"] {
        let path = dir.path().join(format!("shadows/{shadow}.md"));
        let text = fs::read_to_string(&path).expect("shadow");
        let timeline = text
            .split("## Timeline")
            .nth(1)
            .unwrap_or_else(|| panic!("{shadow} has no timeline\n{text}"));
        let hits = timeline.matches(line).count();
        assert_eq!(hits, 1, "{shadow} timeline\n{timeline}");
    }
}

#[test]
fn add_channel_on_ada_links_both_channels() {
    let dir = fixture_vault(true);
    let channel = run(
        dir.path(),
        &[
            "new",
            "channel",
            "patreon:adaexample",
            "--as",
            "t",
            "--field",
            "shadow=shadow-ada-example",
            "--field",
            "locator=patreon:adaexample",
            "--json",
        ],
    );
    assert_eq!(
        channel.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&channel.stderr)
    );

    let raw_rel = "sources/raw/shadows/ada-example/patreon-adaexample/p1.txt";
    let raw = dir.path().join(raw_rel);
    fs::create_dir_all(raw.parent().expect("raw parent")).expect("raw dir");
    fs::write(&raw, "a patreon post\n").expect("raw");
    let digest = sha256_hex(&raw);

    let source = run(
        dir.path(),
        &[
            "new",
            "source",
            "patreon:adaexample 2026-11-26 p1",
            "--as",
            "t",
            "--field",
            &format!("path={raw_rel}"),
            "--field",
            &format!("sha256={digest}"),
            "--field",
            "tags=shadow/ada-example",
            "--json",
        ],
    );
    assert_eq!(
        source.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );

    let logged = run(
        dir.path(),
        &[
            "log",
            "shadow-ada-example",
            "2026-11-26 | [[patreon-adaexample]] | channel added",
            "--json",
        ],
    );
    assert_eq!(
        logged.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&logged.stderr)
    );

    let checked = json_at(dir.path(), &["check", "--json"], 0);
    assert_eq!(checked["ok"], true);
    assert!(error_codes(&checked).is_empty(), "{checked}");

    let links = json_at(dir.path(), &["links", "shadow-ada-example", "--json"], 0);
    let mut channels = Vec::new();
    for edge in links["data"]["backlinks"].as_array().expect("backlinks") {
        let id = edge["source"].as_str().expect("source");
        let note = json_at(dir.path(), &["get", id, "--json"], 0);
        if note["data"]["type"] == "channel" {
            channels.push(note["data"]["slug"].as_str().expect("slug").to_string());
        }
    }
    channels.sort();
    assert_eq!(
        channels,
        vec!["patreon-adaexample".to_string(), "x-adaexample".to_string()]
    );
}

#[test]
fn blocked_channel_keeps_cursor() {
    let dir = fixture_vault(true);
    let stamped = json_at(
        dir.path(),
        &[
            "set",
            "x-adaexample",
            "--field",
            "cursor=2026-01-01",
            "--json",
        ],
        0,
    );
    assert_eq!(stamped["ok"], true);
    let blocked = json_at(
        dir.path(),
        &[
            "set",
            "x-adaexample",
            "--field",
            "state=blocked",
            "--field",
            "blocked=no access",
            "--json",
        ],
        0,
    );
    assert_eq!(blocked["ok"], true);
    let note = json_at(dir.path(), &["get", "x-adaexample", "--json"], 0);
    let front = &note["data"]["frontmatter"];
    assert_eq!(front["cursor"], "2026-01-01");
    assert_eq!(front["state"], "blocked");
    assert_eq!(front["blocked"], "no access");
}

#[test]
fn b6_fixture_without_types_is_unknown_type() {
    let dir = fixture_vault(false);
    let body = json_at(dir.path(), &["check", "--json"], 2);
    assert_eq!(body["error"]["code"], "CHECK_FAILED");
    let paths = paths_for(&body, "UNKNOWN_TYPE");
    assert!(
        paths
            .iter()
            .any(|path| path == "shadows/shadow-ada-example.md"),
        "{paths:?}"
    );
    assert!(
        paths.iter().any(|path| path == "channels/x-adaexample.md"),
        "{paths:?}"
    );
}
