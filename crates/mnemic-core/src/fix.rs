//! `check --fix` fills a missing `id` and a uniquely derived `type`.
//!
//! The time half of each id comes from the caller clock. The random half comes
//! from [`FIX_ID_SEED`], so one clock and one file order always yield the same ids.

use std::path::Path;
use std::time::Duration;

use crate::check::{self, locate_notes, LocateError};
use crate::clock::Clock;
use crate::contract::{self, CheckEnvelope, FixChange, FixField};
use crate::edit::FrontmatterEdit;
use crate::frontmatter::{self, FrontmatterError};
use crate::pack::{self, Pack};
use crate::ulid::UlidGenerator;
use crate::vault::Vault;
use crate::write::{self, WriteTxn};
use crate::Error;

/// Seed for ids minted by `check --fix`. Tests and the CLI share it.
pub const FIX_ID_SEED: u64 = 7;

pub struct FixRequest<'a, C: Clock> {
    pub vault: &'a Vault,
    pub path: &'a Path,
    pub clock: &'a C,
    pub lock_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixOutcome {
    pub changes: Vec<FixChange>,
    pub report: check::CheckReport,
}

#[derive(Debug, thiserror::Error)]
pub enum FixError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Io(String),
    #[error("{}", lock_message(.pid, .since))]
    LockHeld {
        pid: u32,
        since: std::time::SystemTime,
    },
    #[error("{0}")]
    Outside(String),
}

fn lock_message(pid: &u32, since: &std::time::SystemTime) -> String {
    let secs = since
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("lock held by pid {pid} since {secs}")
}

pub fn check_fix<C: Clock>(request: FixRequest<'_, C>) -> Result<FixOutcome, FixError> {
    let vault = Vault {
        root: request.vault.root.clone(),
        schema_version: request.vault.schema_version,
        actor: request.vault.actor.clone(),
    };
    let path = request.path.to_path_buf();
    let clock = request.clock;
    match write::with_write_lock(&vault.root, request.lock_timeout, |txn| {
        Ok(fix_locked(&vault, &path, clock, txn))
    }) {
        Ok(Ok(outcome)) => Ok(outcome),
        Ok(Err(err)) => Err(err),
        Err(Error::LockHeld { pid, since }) => Err(FixError::LockHeld { pid, since }),
        Err(Error::Io(err)) => Err(FixError::Io(err.to_string())),
        Err(Error::Empty(name)) => Err(FixError::Usage(format!("{name} is empty"))),
    }
}

fn fix_locked<C: Clock>(
    vault: &Vault,
    path: &Path,
    clock: &C,
    txn: &WriteTxn,
) -> Result<FixOutcome, FixError> {
    let (root, _start, files) = match locate_notes(&vault.root, path) {
        Ok(located) => located,
        Err(LocateError::Outside(message)) => return Err(FixError::Outside(message)),
        Err(LocateError::Io(message)) => return Err(FixError::Io(message)),
    };
    let pack = pack::resolve_pack(&root).ok();
    let mut changes = Vec::new();
    let mut ids = UlidGenerator::from_seed(FIX_ID_SEED);
    if let Some(pack) = &pack {
        for file in &files {
            if !check::regular_file_inside(&root, file) {
                continue;
            }
            let (_, canon) = match crate::vault::canonicalize_within(&root, file) {
                Ok(confined) => confined,
                Err(crate::vault::ConfineError::Outside(message)) => {
                    return Err(FixError::Outside(message));
                }
                Err(crate::vault::ConfineError::Io(message)) => {
                    return Err(FixError::Io(message));
                }
            };
            if !check::regular_file_inside(&root, &canon) {
                return Err(FixError::Outside(format!(
                    "{} is outside the vault",
                    canon.display()
                )));
            }
            let rel = check::rel_path(&root, file);
            let bytes = std::fs::read(&canon).map_err(|err| FixError::Io(err.to_string()))?;
            let Ok(source) = String::from_utf8(bytes) else {
                continue;
            };
            let Some((edited, file_changes)) = plan_fix(&rel, &source, pack, &mut ids, clock)?
            else {
                continue;
            };
            txn.atomic_write(&canon, edited.as_bytes())
                .map_err(|err| FixError::Io(err.to_string()))?;
            changes.extend(file_changes);
        }
    }
    let report = check::check_path(
        &Vault {
            root,
            schema_version: vault.schema_version,
            actor: vault.actor.clone(),
        },
        path,
    )
    .map_err(|err| match err {
        check::CheckFatal::Usage(message) => FixError::Usage(message),
        check::CheckFatal::Io(message) => FixError::Io(message),
    })?;
    Ok(FixOutcome { changes, report })
}

/// `None` when the file needs no insert. Malformed frontmatter and a present
/// `id` or `type` are left as they are.
fn plan_fix<C: Clock>(
    rel: &str,
    source: &str,
    pack: &Pack,
    ids: &mut UlidGenerator,
    clock: &C,
) -> Result<Option<(String, Vec<FixChange>)>, FixError> {
    let derived = top_folder(rel).and_then(|folder| type_for_folder(pack, folder));
    match frontmatter::parse_note(source) {
        Err(FrontmatterError::Missing) => {
            let id = ids.next(clock);
            let mut pairs = vec![("id", id.clone())];
            let mut changes = vec![change(rel, FixField::Id, id)];
            if let Some(type_name) = derived {
                changes.push(change(rel, FixField::Type, type_name.clone()));
                pairs.push(("type", type_name));
            }
            let fields: Vec<(&str, &str)> = pairs
                .iter()
                .map(|(key, value)| (*key, value.as_str()))
                .collect();
            let text = FrontmatterEdit::create_block(source, &fields)
                .map_err(|err| FixError::Io(err.to_string()))?;
            Ok(Some((text, changes)))
        }
        Err(_) => Ok(None),
        Ok(note) => {
            let mut text = source.to_string();
            let mut changes = Vec::new();
            if !note.values.contains_key("id") {
                let id = ids.next(clock);
                text = FrontmatterEdit::insert(&text, "id", &id)
                    .map_err(|err| FixError::Io(err.to_string()))?;
                changes.push(change(rel, FixField::Id, id));
            }
            if !note.values.contains_key("type") {
                if let Some(type_name) = derived {
                    text = FrontmatterEdit::insert(&text, "type", &type_name)
                        .map_err(|err| FixError::Io(err.to_string()))?;
                    changes.push(change(rel, FixField::Type, type_name));
                }
            }
            if changes.is_empty() {
                Ok(None)
            } else {
                Ok(Some((text, changes)))
            }
        }
    }
}

fn top_folder(rel: &str) -> Option<&str> {
    let (folder, rest) = rel.split_once('/')?;
    if folder.is_empty() || rest.is_empty() {
        None
    } else {
        Some(folder)
    }
}

fn type_for_folder(pack: &Pack, folder: &str) -> Option<String> {
    let mut found = None;
    for name in pack.names() {
        let def = pack.get(name)?;
        if def.folders.iter().any(|item| item == folder) {
            if found.is_some() {
                return None;
            }
            found = Some(name.to_string());
        }
    }
    found
}

fn change(path: &str, field: FixField, new: String) -> FixChange {
    FixChange {
        path: path.to_string(),
        field,
        old: None,
        new,
    }
}

pub fn fix_envelope(outcome: &FixOutcome) -> CheckEnvelope {
    let mut envelope = if outcome.report.findings.is_empty() {
        contract::success_envelope()
    } else {
        contract::findings_envelope(
            outcome.report.findings.clone(),
            Vec::new(),
            outcome.report.details.clone(),
        )
    };
    if let Some(data) = envelope.data.as_mut() {
        data.changes = Some(outcome.changes.clone());
    }
    envelope
}
