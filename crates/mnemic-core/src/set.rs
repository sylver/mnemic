//! Set declared frontmatter fields on one note.
//!
//! Field values are checked before the write lock. The note is read again
//! inside the lock. A no-op does not stamp `updated` and does not write.

use std::path::Path;
use std::time::Duration;

use chrono::DateTime;
use serde::Serialize;

use crate::clock::Clock;
use crate::edit::{EditError, FrontmatterEdit};
use crate::fields::{self, Assignment, FieldEdit};
use crate::frontmatter::{self, FrontmatterError};
use crate::index::{ResolveError, Resolved, VaultIndex};
use crate::pack::{self, PackError};
use crate::vault::{self, ConfineError, VaultError};
use crate::write::{self, WriteTxn};
use crate::Error;

pub struct SetRequest<'a> {
    pub vault: &'a Path,
    pub note: &'a str,
    pub fields: &'a [String],
    pub adds: &'a [String],
    pub clock: &'a dyn Clock,
    pub lock_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetResult {
    pub id: String,
    pub path: String,
    pub noop: bool,
    pub changed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetSuccess {
    pub ok: bool,
    pub data: SetData,
    pub error: Option<()>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetData {
    pub id: String,
    pub path: String,
    pub noop: bool,
    pub changed: Vec<String>,
}

impl SetResult {
    pub fn success(&self) -> SetSuccess {
        SetSuccess {
            ok: true,
            data: SetData {
                id: self.id.clone(),
                path: self.path.clone(),
                noop: self.noop,
                changed: self.changed.clone(),
            },
            error: None,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SetError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    NotFound(String),
    #[error("note matches more than one path: {}", candidates.join(", "))]
    Ambiguous { candidates: Vec<String> },
    #[error("{0}")]
    OutsideVault(String),
    #[error("{0}")]
    UnknownType(String),
    #[error("{0}")]
    Vault(String),
    #[error("lock held by pid {pid} since {}", since_secs(*since))]
    LockHeld {
        pid: u32,
        since: std::time::SystemTime,
    },
    #[error("{0}")]
    Io(String),
}

fn since_secs(since: std::time::SystemTime) -> u64 {
    since
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub fn set_fields(request: SetRequest<'_>) -> Result<SetResult, SetError> {
    let inputs = assignments(request.fields, request.adds)?;
    if inputs.is_empty() {
        return Err(SetError::Usage("set needs --field or --add".to_string()));
    }
    let (root, _) = vault::open_config(request.vault, Some(request.vault)).map_err(vault_error)?;
    let pack = pack::resolve_pack(&root).map_err(pack_error)?;
    let index = VaultIndex::load(&root).map_err(index_error)?;
    let (id, path) = resolve_note(&root, &index, request.note)?;
    if path.starts_with("sources/raw/") {
        return Err(SetError::Usage("sources/raw is not rewritten".to_string()));
    }
    let source = read_note(&root, &path)?;
    let type_name = note_type(&source)?;
    let def = pack
        .get(&type_name)
        .ok_or_else(|| SetError::UnknownType(format!("unknown type {type_name}")))?;
    let edits = fields::edits_for(&type_name, def, &inputs).map_err(SetError::Usage)?;
    match write::with_write_lock(&root, request.lock_timeout, |txn| {
        Ok(set_locked(&root, &id, &edits, request.clock, txn))
    }) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(lock_error(err)),
    }
}

fn set_locked(
    root: &Path,
    id: &str,
    edits: &[FieldEdit],
    clock: &dyn Clock,
    txn: &WriteTxn,
) -> Result<SetResult, SetError> {
    let index = VaultIndex::load(root).map_err(index_error)?;
    let (id, path) = match index.resolve(id) {
        Ok(Resolved::Note { id, path }) => (id, path),
        Ok(Resolved::File { path }) => {
            return Err(SetError::NotFound(format!("no note matches {path}")));
        }
        Err(ResolveError::Missing) => {
            return Err(SetError::NotFound(format!("no note matches {id}")));
        }
        Err(ResolveError::Ambiguous { candidates }) => {
            return Err(SetError::Ambiguous { candidates });
        }
        Err(ResolveError::Outside) => {
            return Err(SetError::OutsideVault(format!("{id} is outside the vault")));
        }
    };
    if path.starts_with("sources/raw/") {
        return Err(SetError::Usage("sources/raw is not rewritten".to_string()));
    }
    let full = confine(root, &path)?;
    let source = read_at(&full, &path)?;
    let type_name = note_type(&source)?;
    let pack = pack::resolve_pack(root).map_err(pack_error)?;
    let def = pack
        .get(&type_name)
        .ok_or_else(|| SetError::UnknownType(format!("unknown type {type_name}")))?;
    for edit in edits {
        fields::require_key(&type_name, def, edit_key(edit)).map_err(SetError::Usage)?;
    }
    let mut text = source.clone();
    let mut changed = Vec::new();
    for edit in edits {
        let next = apply_edit(&text, edit)?;
        if next != text {
            let key = edit_key(edit);
            if !changed.iter().any(|item| item == key) {
                changed.push(key.to_string());
            }
            text = next;
        }
    }
    if text == source {
        return Ok(SetResult {
            id,
            path,
            noop: true,
            changed,
        });
    }
    let stamped = FrontmatterEdit::replace(&text, "updated", &stamp(clock))
        .map_err(|err| SetError::Io(format!("updated: {err}")))?;
    txn.atomic_write(&full, stamped.as_bytes())
        .map_err(|err| SetError::Io(err.to_string()))?;
    Ok(SetResult {
        id,
        path,
        noop: false,
        changed,
    })
}

fn apply_edit(source: &str, edit: &FieldEdit) -> Result<String, SetError> {
    let result = match edit {
        FieldEdit::Add { key, item } => FrontmatterEdit::append_sequence_item(source, key, item),
        FieldEdit::Replace {
            key,
            yaml,
            sequence,
        } => {
            let parsed = frontmatter::parse_note(source).map_err(frontmatter_error)?;
            let present = parsed.values.contains_key(key);
            if *sequence {
                if present {
                    FrontmatterEdit::replace_sequence(source, key, yaml)
                } else {
                    FrontmatterEdit::insert(source, key, yaml)
                }
            } else if present {
                FrontmatterEdit::replace(source, key, yaml)
            } else {
                FrontmatterEdit::insert(source, key, yaml)
            }
        }
    };
    result.map_err(|err| match err {
        EditError::MultilineValue { key } => {
            SetError::Usage(format!("{key} value spans multiple lines"))
        }
        other => SetError::Io(other.to_string()),
    })
}

fn edit_key(edit: &FieldEdit) -> &str {
    match edit {
        FieldEdit::Replace { key, .. } | FieldEdit::Add { key, .. } => key,
    }
}

fn assignments(fields: &[String], adds: &[String]) -> Result<Vec<Assignment>, SetError> {
    let mut inputs = Vec::with_capacity(fields.len() + adds.len());
    for raw in fields {
        inputs.push(fields::parse_assignment(raw, false).map_err(SetError::Usage)?);
    }
    for raw in adds {
        inputs.push(fields::parse_assignment(raw, true).map_err(SetError::Usage)?);
    }
    Ok(inputs)
}

fn resolve_note(root: &Path, index: &VaultIndex, note: &str) -> Result<(String, String), SetError> {
    match index.resolve_argument(note) {
        Ok(Resolved::Note { id, path }) => Ok((id, path)),
        Ok(Resolved::File { .. }) => bare_or_missing(root, note),
        Err(ResolveError::Missing) => bare_or_missing(root, note),
        Err(ResolveError::Ambiguous { candidates }) => Err(SetError::Ambiguous { candidates }),
        Err(ResolveError::Outside) => Err(SetError::OutsideVault(format!(
            "{note} is outside the vault"
        ))),
    }
}

fn bare_or_missing(root: &Path, note: &str) -> Result<(String, String), SetError> {
    let rel = note.trim_start_matches("./");
    if rel.ends_with(".md") {
        if let Ok(source) = read_note(root, rel) {
            return match frontmatter::parse_note(&source) {
                Err(FrontmatterError::Missing) => {
                    Err(SetError::Usage("note has no frontmatter".to_string()))
                }
                Err(err) => Err(SetError::Io(err.to_string())),
                Ok(_) => Err(SetError::NotFound(format!("no note matches {note}"))),
            };
        }
    }
    Err(SetError::NotFound(format!("no note matches {note}")))
}

fn note_type(source: &str) -> Result<String, SetError> {
    let parsed = frontmatter::parse_note(source).map_err(frontmatter_error)?;
    parsed
        .values
        .get("type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| SetError::Usage("note has no type".to_string()))
}

fn frontmatter_error(err: FrontmatterError) -> SetError {
    match err {
        FrontmatterError::Missing => SetError::Usage("note has no frontmatter".to_string()),
        other => SetError::Io(other.to_string()),
    }
}

fn read_note(root: &Path, rel: &str) -> Result<String, SetError> {
    let full = confine(root, rel)?;
    read_at(&full, rel)
}

fn read_at(full: &Path, rel: &str) -> Result<String, SetError> {
    let bytes = std::fs::read(full).map_err(|err| SetError::Io(err.to_string()))?;
    String::from_utf8(bytes).map_err(|_| SetError::Io(format!("{rel} is not UTF-8")))
}

fn confine(root: &Path, rel: &str) -> Result<std::path::PathBuf, SetError> {
    match vault::canonicalize_within(root, &root.join(rel)) {
        Ok((_, full)) => Ok(full),
        Err(ConfineError::Outside(message)) => Err(SetError::OutsideVault(message)),
        Err(ConfineError::Io(message)) => Err(SetError::Io(message)),
    }
}

fn stamp(clock: &dyn Clock) -> String {
    let millis = i64::try_from(clock.unix_millis()).unwrap_or(0);
    let dt = DateTime::from_timestamp_millis(millis).unwrap_or(DateTime::UNIX_EPOCH);
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn index_error(err: crate::check::CheckFatal) -> SetError {
    match err {
        crate::check::CheckFatal::Usage(message) | crate::check::CheckFatal::Io(message) => {
            SetError::Io(message)
        }
    }
}

fn pack_error(err: PackError) -> SetError {
    SetError::Io(err.to_string())
}

fn vault_error(err: VaultError) -> SetError {
    match err {
        VaultError::NotFound => SetError::Vault(err.to_string()),
        VaultError::BadConfig(message) => SetError::Vault(message),
        VaultError::Unsupported { .. } => SetError::Vault(err.to_string()),
        VaultError::Io(message) => SetError::Io(message),
    }
}

fn lock_error(err: Error) -> SetError {
    match err {
        Error::LockHeld { pid, since } => SetError::LockHeld { pid, since },
        Error::Io(err) => SetError::Io(err.to_string()),
        Error::Empty(label) => SetError::Usage(format!("{label} is empty")),
    }
}
