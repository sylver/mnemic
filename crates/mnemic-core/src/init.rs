use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use thiserror::Error;

use crate::write::{with_write_lock, DEFAULT_LOCK_TIMEOUT};
use crate::{core_pack, parse_vault_config, Error as CoreError, VaultError};

const GITIGNORE_LINE: &[u8] = b".mnemic/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitRequest<'a> {
    pub target: &'a Path,
    pub actor: Option<&'a str>,
    pub agents_template: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitReport {
    pub vault: PathBuf,
    pub created: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct InitSuccess {
    pub ok: bool,
    pub data: InitData,
    pub error: Option<crate::ErrorObject>,
}

#[derive(Debug, Serialize)]
pub struct InitData {
    pub vault: String,
    pub created: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Error)]
pub enum InitError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Io(String),
    #[error("lock held by pid {pid} since {}", since_secs(.since))]
    LockHeld { pid: u32, since: SystemTime },
    #[error("target is inside a vault at {}", .0.display())]
    NestedVault(PathBuf),
    #[error("vault schema_version {found} is newer than this CLI ({supported})")]
    Unsupported { found: i64, supported: u32 },
    #[error("{0}")]
    BadConfig(String),
}

fn since_secs(since: &SystemTime) -> u64 {
    since
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

enum Step {
    Done(InitReport),
    Failed(InitError),
}

pub fn init_vault(request: InitRequest<'_>) -> Result<InitReport, InitError> {
    if let Some(actor) = request.actor {
        quote_toml(actor)?;
    }
    let target = prepare_target(request.target)?;
    if let Some(parent) = enclosing_vault(&target)? {
        return Err(InitError::NestedVault(parent));
    }
    classify_config(&target.join("mnemic.toml"))?;

    let template = request.agents_template.to_string();
    let toml_body = toml_bytes(request.actor)?;
    let folders = type_folders()?;
    let step = with_write_lock(&target, DEFAULT_LOCK_TIMEOUT, |txn| {
        if let Err(err) = classify_config(&target.join("mnemic.toml")) {
            return Ok(Step::Failed(err));
        }
        let mut created = Vec::new();
        let mut skipped = Vec::new();
        let toml_path = target.join("mnemic.toml");
        if toml_path.is_file() {
            skipped.push("mnemic.toml".to_string());
        } else {
            txn.atomic_write(&toml_path, &toml_body)?;
            created.push("mnemic.toml".to_string());
        }
        write_if_absent(
            txn,
            &target.join("AGENTS.md"),
            template.as_bytes(),
            "AGENTS.md",
            &mut created,
            &mut skipped,
        )?;
        ensure_gitignore(txn, &target, &mut created, &mut skipped)?;
        for folder in &folders {
            let dir = target.join(folder);
            fs::create_dir_all(&dir)?;
            let keep = dir.join(".gitkeep");
            let rel = format!("{folder}/.gitkeep");
            if keep.exists() {
                skipped.push(rel);
                continue;
            }
            if directory_has_entries(&dir)? {
                continue;
            }
            txn.atomic_write(&keep, b"")?;
            created.push(rel);
        }
        created.sort();
        skipped.sort();
        Ok(Step::Done(InitReport {
            vault: target.clone(),
            created,
            skipped,
        }))
    })
    .map_err(write_to_init)?;
    match step {
        Step::Done(report) => Ok(report),
        Step::Failed(err) => Err(err),
    }
}

fn prepare_target(target: &Path) -> Result<PathBuf, InitError> {
    if target.exists() {
        if !target.is_dir() {
            return Err(InitError::Usage(format!(
                "{} is not a directory",
                target.display()
            )));
        }
        return fs::canonicalize(target).map_err(io_err);
    }
    if let Some(parent) = enclosing_vault_for_missing(target)? {
        return Err(InitError::NestedVault(parent));
    }
    fs::create_dir(target).map_err(io_err)?;
    fs::canonicalize(target).map_err(io_err)
}

fn enclosing_vault(target: &Path) -> Result<Option<PathBuf>, InitError> {
    let mut cursor = target.parent().map(Path::to_path_buf);
    while let Some(dir) = cursor {
        if dir.join("mnemic.toml").is_file() {
            return Ok(Some(dir));
        }
        cursor = dir.parent().map(Path::to_path_buf);
    }
    Ok(None)
}

fn enclosing_vault_for_missing(target: &Path) -> Result<Option<PathBuf>, InitError> {
    let mut cursor = target.to_path_buf();
    while !cursor.exists() {
        if !cursor.pop() {
            return Ok(None);
        }
    }
    let existing = fs::canonicalize(&cursor).map_err(io_err)?;
    if existing.join("mnemic.toml").is_file() {
        return Ok(Some(existing));
    }
    enclosing_vault(&existing)
}

fn classify_config(path: &Path) -> Result<(), InitError> {
    if !path.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(path).map_err(io_err)?;
    match parse_vault_config(&text) {
        Ok(_) => Ok(()),
        Err(VaultError::BadConfig(message)) => Err(InitError::BadConfig(message)),
        Err(VaultError::Unsupported { found, supported }) => {
            Err(InitError::Unsupported { found, supported })
        }
        Err(VaultError::NotFound) => Ok(()),
        Err(VaultError::Io(message)) => Err(InitError::Io(message)),
    }
}

fn toml_bytes(actor: Option<&str>) -> Result<Vec<u8>, InitError> {
    let mut text = String::from("schema_version = 1\n");
    if let Some(actor) = actor {
        text.push_str("actor = ");
        text.push_str(&quote_toml(actor)?);
        text.push('\n');
    }
    Ok(text.into_bytes())
}

fn quote_toml(value: &str) -> Result<String, InitError> {
    if value.is_empty() || value.chars().any(|c| matches!(c, '\n' | '\r' | '\0')) {
        return Err(InitError::Usage(
            "actor must be a single-line non-empty string".to_string(),
        ));
    }
    let mut out = String::from("\"");
    for ch in value.chars() {
        if ch == '\\' || ch == '"' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    Ok(out)
}

fn write_if_absent(
    txn: &crate::write::WriteTxn,
    path: &Path,
    bytes: &[u8],
    rel: &str,
    created: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> Result<(), CoreError> {
    if path.exists() {
        skipped.push(rel.to_string());
        return Ok(());
    }
    txn.atomic_write(path, bytes)?;
    created.push(rel.to_string());
    Ok(())
}

fn ensure_gitignore(
    txn: &crate::write::WriteTxn,
    vault: &Path,
    created: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> Result<(), CoreError> {
    let path = vault.join(".gitignore");
    if !path.exists() {
        txn.atomic_write(&path, b".mnemic/\n")?;
        created.push(".gitignore".to_string());
        return Ok(());
    }
    let bytes = fs::read(&path)?;
    if gitignore_has_line(&bytes) {
        skipped.push(".gitignore".to_string());
        return Ok(());
    }
    let mut next = bytes;
    if !next.is_empty() && !next.ends_with(b"\n") {
        next.push(b'\n');
    }
    next.extend_from_slice(b".mnemic/\n");
    txn.atomic_write(&path, &next)?;
    created.push(".gitignore".to_string());
    Ok(())
}

fn gitignore_has_line(bytes: &[u8]) -> bool {
    bytes.split(|byte| *byte == b'\n').any(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        trim_ascii(line) == GITIGNORE_LINE
    })
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

fn directory_has_entries(path: &Path) -> Result<bool, CoreError> {
    Ok(fs::read_dir(path)?.next().is_some())
}

fn type_folders() -> Result<Vec<String>, InitError> {
    let pack = core_pack().map_err(|err| InitError::Io(err.to_string()))?;
    let mut folders = Vec::new();
    for name in pack.names() {
        let Some(def) = pack.get(name) else {
            continue;
        };
        for folder in &def.folders {
            if !folders.contains(folder) {
                folders.push(folder.clone());
            }
        }
    }
    folders.sort();
    Ok(folders)
}

fn io_err(err: std::io::Error) -> InitError {
    InitError::Io(err.to_string())
}

fn write_to_init(err: CoreError) -> InitError {
    match err {
        CoreError::LockHeld { pid, since } => InitError::LockHeld { pid, since },
        CoreError::Io(err) => InitError::Io(err.to_string()),
        CoreError::Empty(label) => InitError::Io(format!("{label} is empty")),
    }
}
