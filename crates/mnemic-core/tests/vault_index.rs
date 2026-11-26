use std::fs;
use std::path::{Path, PathBuf};

use mnemic_core::{index_vault, CheckFatal, ResolveError, Resolved, Vault};

fn note(id: &str, typ: &str, extra: &str, body: &str) -> String {
    format!(
        "---\nid: {id}\ntype: {typ}\nschema_version: 1\ntitle: T\ncreated: 2026-11-23T00:00:00Z\nupdated: 2026-11-23T00:00:00Z\ncreated_by: sylver\nstatus: active\n{extra}---\n\n{body}"
    )
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn vault(dir: &Path) -> Vault {
    Vault {
        root: dir.to_path_buf(),
        schema_version: 1,
        actor: "sylver".to_string(),
    }
}

#[test]
fn resolves_path_id_slug_alias_and_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "people/ada.md",
        &note(
            "01ARZ3NDEKTSV4RRFFQ69G5FB1",
            "person",
            "aliases:\n  - Countess\n",
            "",
        ),
    );
    write(root, "pic.png", "png");
    let index = index_vault(root).expect("index");
    assert_eq!(
        index.resolve("people/ada").unwrap(),
        Resolved::Note {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string(),
            path: "people/ada.md".to_string(),
        }
    );
    assert_eq!(
        index.resolve("people/ada.md#Missing|Ada").unwrap(),
        Resolved::Note {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string(),
            path: "people/ada.md".to_string(),
        }
    );
    assert_eq!(
        index.resolve("01ARZ3NDEKTSV4RRFFQ69G5FB1").unwrap(),
        Resolved::Note {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string(),
            path: "people/ada.md".to_string(),
        }
    );
    assert_eq!(
        index.resolve("ADA").unwrap(),
        Resolved::Note {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string(),
            path: "people/ada.md".to_string(),
        }
    );
    assert_eq!(
        index.resolve("countess").unwrap(),
        Resolved::Note {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FB1".to_string(),
            path: "people/ada.md".to_string(),
        }
    );
    assert_eq!(
        index.resolve("pic.png").unwrap(),
        Resolved::File {
            path: "pic.png".to_string(),
        }
    );
    assert_eq!(index.resolve("missing.png"), Err(ResolveError::Missing));
}

#[test]
fn ambiguous_slug_lists_candidates_and_path_is_exact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "people/sam.md",
        &note("01ARZ3NDEKTSV4RRFFQ69G5FE1", "note", "", ""),
    );
    write(
        root,
        "orgs/sam.md",
        &note("01ARZ3NDEKTSV4RRFFQ69G5FE2", "note", "", ""),
    );
    let index = index_vault(root).expect("index");
    assert_eq!(
        index.resolve("sam"),
        Err(ResolveError::Ambiguous {
            candidates: vec!["orgs/sam.md".to_string(), "people/sam.md".to_string(),],
        })
    );
    assert!(matches!(
        index.resolve("people/sam"),
        Ok(Resolved::Note { path, .. }) if path == "people/sam.md"
    ));
}

#[test]
fn nfc_slug_and_alias_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let nfd = "Nai\u{0308}ve";
    write(
        root,
        "topics/caf\u{00e9}.md",
        &note("01ARZ3NDEKTSV4RRFFQ69G5FE4", "topic", "", ""),
    );
    write(
        root,
        "notes/nick.md",
        &note(
            "01ARZ3NDEKTSV4RRFFQ69G5FE6",
            "note",
            &format!("aliases:\n  - {nfd}\n"),
            "",
        ),
    );
    let index = index_vault(root).expect("index");
    assert!(matches!(
        index.resolve("CAF\u{00c9}"),
        Ok(Resolved::Note { path, .. }) if path.ends_with("caf\u{00e9}.md")
    ));
    assert_eq!(
        index.resolve("NA\u{00cf}VE").unwrap(),
        Resolved::Note {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FE6".to_string(),
            path: "notes/nick.md".to_string(),
        }
    );
}

#[test]
fn code_fences_are_not_links_and_check_does_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let body = "See [[01ARZ3NDEKTSV4RRFFQ69G5FE5]].\n\n`[[missing-inline]]`\n\n```\n[[missing-fence]]\n```\n";
    write(
        root,
        "notes/pointer.md",
        &note("01ARZ3NDEKTSV4RRFFQ69G5FE5", "note", "", body),
    );
    let before = fs::read(root.join("notes/pointer.md")).unwrap();
    let report = mnemic_core::check_path(&vault(root), root).expect("check");
    let after = fs::read(root.join("notes/pointer.md")).unwrap();
    assert_eq!(before, after);
    assert!(
        report
            .findings
            .iter()
            .all(|finding| finding.code != mnemic_core::FindingCode::BrokenLink),
        "{:?}",
        report.findings
    );
}

#[test]
fn typed_link_uses_the_same_resolver() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "orgs/rivendell.md",
        &note("01ARZ3NDEKTSV4RRFFQ69G5FB2", "org", "", ""),
    );
    write(
        root,
        "people/ada.md",
        &note(
            "01ARZ3NDEKTSV4RRFFQ69G5FB1",
            "person",
            "org: rivendell\n",
            "",
        ),
    );
    write(
        root,
        "people/nope.md",
        &note(
            "01ARZ3NDEKTSV4RRFFQ69G5FE7",
            "person",
            "org: missing-org\n",
            "",
        ),
    );
    let report = mnemic_core::check_path(&vault(root), root).expect("check");
    let broken: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.code == mnemic_core::FindingCode::BrokenLink)
        .map(|finding| finding.path.clone())
        .collect();
    assert_eq!(broken, vec!["people/nope.md".to_string()]);
}

#[test]
fn load_reports_io_for_a_missing_root() {
    let missing = PathBuf::from("/tmp/mnemic-missing-vault-index");
    let _ = fs::remove_dir_all(&missing);
    let err = index_vault(&missing).expect_err("missing");
    assert!(matches!(err, CheckFatal::Io(_)));
}
