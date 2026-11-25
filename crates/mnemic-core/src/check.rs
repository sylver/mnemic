use std::fs;
use std::path::{Path, PathBuf};

use chrono::DateTime;
use serde_json::Value;

use crate::contract::{Finding, FindingCode};
use crate::frontmatter::{self, FrontmatterError, ParsedNote};
use crate::ulid::is_ulid;
use crate::vault::Vault;

const REQUIRED: &[&str] = &[
    "id",
    "type",
    "schema_version",
    "title",
    "created",
    "updated",
    "created_by",
    "status",
];

const STATUSES: &[&str] = &["active", "archived", "superseded"];

#[derive(Debug, PartialEq, Eq)]
pub enum CheckFatal {
    Usage(String),
    Io(String),
}

pub fn check_path(vault: &Vault, path: &Path) -> Result<Vec<Finding>, CheckFatal> {
    let start = canonicalize(path)?;
    let root = canonicalize(&vault.root)?;
    if start != root && !start.starts_with(&root) {
        return Err(CheckFatal::Usage(format!(
            "{} is outside the vault",
            path.display()
        )));
    }
    let mut files = Vec::new();
    if start.is_file() {
        if is_note(&root, &start) {
            files.push(start);
        }
    } else {
        collect_notes(&root, &start, &mut files)?;
    }
    files.sort();
    let mut findings = Vec::new();
    for file in files {
        let rel = rel_path(&root, &file);
        let text = fs::read(&file).map_err(|err| CheckFatal::Io(err.to_string()))?;
        let source = match String::from_utf8(text) {
            Ok(source) => source,
            Err(_) => {
                findings.push(finding(
                    FindingCode::BadFrontmatter,
                    "note is not UTF-8",
                    rel,
                ));
                continue;
            }
        };
        match frontmatter::parse_note(&source) {
            Ok(note) => findings.extend(core_findings(&note, rel)),
            Err(err) => findings.push(frontmatter_finding(err, rel)),
        }
    }
    Ok(findings)
}

fn core_findings(note: &ParsedNote, path: String) -> Vec<Finding> {
    let mut findings = Vec::new();
    for field in REQUIRED {
        if !note.values.contains_key(*field) {
            findings.push(finding(
                FindingCode::MissingField,
                format!("missing {field}"),
                path.clone(),
            ));
        }
    }
    if let Some(id) = note.values.get("id").and_then(Value::as_str) {
        if !is_ulid(id) {
            findings.push(finding(
                FindingCode::BadId,
                "id must be a 26-character ULID",
                path.clone(),
            ));
        }
    } else if note.values.contains_key("id") {
        findings.push(finding(
            FindingCode::BadId,
            "id must be a 26-character ULID",
            path.clone(),
        ));
    }
    if let Some(value) = note.values.get("type") {
        if value.as_str().map(str::is_empty).unwrap_or(true) {
            findings.push(finding(
                FindingCode::BadFrontmatter,
                "type must be a non-empty string",
                path.clone(),
            ));
        }
    }
    if let Some(value) = note.values.get("title") {
        if value.as_str().map(str::is_empty).unwrap_or(true) {
            findings.push(finding(
                FindingCode::BadFrontmatter,
                "title must be a non-empty string",
                path.clone(),
            ));
        }
    }
    if let Some(value) = note.values.get("schema_version") {
        match value.as_i64() {
            Some(version) if version >= 1 => {}
            _ => findings.push(finding(
                FindingCode::BadFrontmatter,
                "schema_version must be an integer greater than or equal to 1",
                path.clone(),
            )),
        }
    }
    for field in ["created", "updated"] {
        if let Some(value) = note.values.get(field) {
            if !is_rfc3339(value) {
                findings.push(finding(
                    FindingCode::BadFrontmatter,
                    format!("{field} must be an RFC 3339 datetime with an offset"),
                    path.clone(),
                ));
            }
        }
    }
    if let Some(value) = note.values.get("created_by") {
        if value.as_str().map(str::is_empty).unwrap_or(true) {
            findings.push(finding(
                FindingCode::BadFrontmatter,
                "created_by must be a non-empty actor name or bot id",
                path.clone(),
            ));
        }
    }
    if let Some(value) = note.values.get("status") {
        let ok = value
            .as_str()
            .is_some_and(|status| STATUSES.contains(&status));
        if !ok {
            findings.push(finding(
                FindingCode::BadFrontmatter,
                "status must be active, archived, or superseded",
                path.clone(),
            ));
        }
    }
    findings
}

fn is_rfc3339(value: &Value) -> bool {
    value
        .as_str()
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .is_some()
}

fn frontmatter_finding(err: FrontmatterError, path: String) -> Finding {
    finding(FindingCode::BadFrontmatter, err.to_string(), path)
}

fn finding(code: FindingCode, message: impl Into<String>, path: String) -> Finding {
    Finding {
        code,
        message: message.into(),
        path,
    }
}

pub fn is_note(vault_root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(vault_root) else {
        return false;
    };
    let Some(name) = rel.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if !name.ends_with(".md") || name == "README.md" || name == "AGENTS.md" {
        return false;
    }
    if rel.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|text| text.starts_with('.'))
    }) {
        return false;
    }
    !rel.starts_with(Path::new("sources").join("raw"))
}

fn collect_notes(vault_root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), CheckFatal> {
    let entries = fs::read_dir(dir).map_err(|err| CheckFatal::Io(err.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|err| CheckFatal::Io(err.to_string()))?;
        let name = entry.file_name();
        let Some(text) = name.to_str() else {
            continue;
        };
        if text.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_notes(vault_root, &path, out)?;
        } else if is_note(vault_root, &path) {
            out.push(path);
        }
    }
    Ok(())
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn canonicalize(path: &Path) -> Result<PathBuf, CheckFatal> {
    fs::canonicalize(path).map_err(|err| CheckFatal::Io(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_set_skips_root_docs_dotfiles_and_raw_sources() {
        let root = Path::new("/vault");
        assert!(is_note(root, Path::new("/vault/log/welcome.md")));
        assert!(!is_note(root, Path::new("/vault/README.md")));
        assert!(!is_note(root, Path::new("/vault/AGENTS.md")));
        assert!(!is_note(root, Path::new("/vault/.secret.md")));
        assert!(!is_note(root, Path::new("/vault/.mnemic/note.md")));
        assert!(!is_note(
            root,
            Path::new("/vault/log/.welcome.mnemic-tmp-1.md")
        ));
        assert!(!is_note(root, Path::new("/vault/sources/raw/blob.md")));
    }
}
