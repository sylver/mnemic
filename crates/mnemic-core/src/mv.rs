//! Move or rename one note without dropping its id.
//!
//! Under one write lock, in order: add the old slug to `aliases` and stamp
//! `updated` when the slug changes, rename without replacing an existing file,
//! then rewrite incoming wikilinks one file at a time. A crash after any step
//! leaves a vault `get` can still resolve. A later `mv` to the same destination
//! finishes the rewrite.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::clock::Clock;
use crate::edit::FrontmatterEdit;
use crate::frontmatter::{self, FrontmatterError};
use crate::index::{wikilink_spans, ResolveError, Resolved, VaultIndex};
use crate::vault::{self, ConfineError, VaultError};
use crate::write::{self, WriteTxn};
use crate::Error;

#[cfg(debug_assertions)]
const CRASH_ENV: &str = "MNEMIC_MV_CRASH";

/// Inputs for [`move_note`]. `note` is a path, id, slug, or alias.
pub struct MoveRequest<'a> {
    pub vault: &'a Path,
    pub note: &'a str,
    pub dest: &'a str,
    pub rewrite: bool,
    pub clock: &'a dyn Clock,
    pub lock_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveResult {
    pub id: String,
    pub from: String,
    pub to: String,
    pub alias_added: bool,
    pub rewritten: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MoveSuccess {
    pub ok: bool,
    pub data: MoveData,
    pub error: Option<()>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MoveData {
    pub id: String,
    pub from: String,
    pub to: String,
    pub alias_added: bool,
    pub rewritten: Vec<String>,
}

impl MoveResult {
    pub fn success(&self) -> MoveSuccess {
        MoveSuccess {
            ok: true,
            data: MoveData {
                id: self.id.clone(),
                from: self.from.clone(),
                to: self.to.clone(),
                alias_added: self.alias_added,
                rewritten: self.rewritten.clone(),
            },
            error: None,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoveError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    NotFound(String),
    #[error("note matches more than one path: {}", candidates.join(", "))]
    Ambiguous { candidates: Vec<String> },
    #[error("{0}")]
    OutsideVault(String),
    #[error("{0}")]
    DestExists(String),
    #[error("{0}")]
    NoteArchived(String),
    #[error("{0}")]
    AliasNotUnique(String),
    #[error("{0}")]
    SlugCollision(String),
    #[error("{0}")]
    RawImmutable(String),
    #[error("lock held by pid {pid} since {}", since_secs(*since))]
    LockHeld {
        pid: u32,
        since: std::time::SystemTime,
    },
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Vault(String),
}

fn since_secs(since: std::time::SystemTime) -> u64 {
    since
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Resolve `request.note` under the vault write lock and move it to `dest`.
pub fn move_note(request: MoveRequest<'_>) -> Result<MoveResult, MoveError> {
    let root = vault::open_vault(request.vault, Some(request.vault))
        .map_err(vault_error)?
        .root;
    match write::with_write_lock(&root, request.lock_timeout, |txn| {
        Ok(move_locked(
            &root,
            request.note,
            request.dest,
            request.rewrite,
            request.clock,
            txn,
        ))
    }) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(lock_error(err)),
    }
}

fn move_locked(
    root: &Path,
    note: &str,
    dest: &str,
    rewrite: bool,
    clock: &dyn Clock,
    txn: &WriteTxn,
) -> Result<MoveResult, MoveError> {
    let index = VaultIndex::load(root).map_err(|err| match err {
        crate::check::CheckFatal::Usage(message) | crate::check::CheckFatal::Io(message) => {
            MoveError::Io(message)
        }
    })?;
    let shape = dest_shape(root, dest)?;
    let located = locate(&index, root, note, &shape)?;
    if located.archived {
        return Err(MoveError::NoteArchived(format!(
            "{} is archived",
            located.path
        )));
    }
    let to = match &shape {
        DestShape::Directory { dir } => format!("{dir}{}", located.file_name),
        DestShape::File { rel } => rel.clone(),
    };
    confine_dest(root, &to)?;
    if located.path == to {
        let from = link_from(root, note, &located.path);
        let rewritten = if rewrite {
            rewrite_incoming(root, &from, &located, txn)?
        } else {
            Vec::new()
        };
        return Ok(MoveResult {
            id: located.id,
            from,
            to,
            alias_added: false,
            rewritten,
        });
    }
    if dest_name_taken(root, &to)? {
        return Err(MoveError::DestExists(format!("{to} already exists")));
    }
    let slug_changed = fold_keep_case(&located.slug) != fold_keep_case(&located.dest_slug(&to));
    let new_slug = located.dest_slug(&to);
    if slug_changed && alias_taken(&index, &located.id, &located.slug) {
        return Err(MoveError::AliasNotUnique(format!(
            "alias {} is already used",
            located.slug
        )));
    }
    if slug_taken(&index, &located.id, &new_slug) {
        return Err(MoveError::SlugCollision(format!(
            "slug {new_slug} matches another note"
        )));
    }
    let source_full = confined_file(root, &located.path)?;
    let mut alias_added = false;
    if slug_changed {
        let source = read_note(root, &located.path)?;
        let with_alias = if alias_present(&located.aliases, &located.slug) {
            source
        } else {
            alias_added = true;
            FrontmatterEdit::append_sequence_item(&source, "aliases", &located.slug)
                .map_err(|err| MoveError::Io(format!("aliases: {err}")))?
        };
        let stamped = FrontmatterEdit::replace(&with_alias, "updated", &stamp(clock))
            .map_err(|err| MoveError::Io(format!("updated: {err}")))?;
        txn.atomic_write(&source_full, stamped.as_bytes())
            .map_err(|err| MoveError::Io(err.to_string()))?;
        maybe_crash("after-alias");
    }
    let dest_full = prepare_dest(root, &to)?;
    if dest_name_taken(root, &to)? {
        return Err(MoveError::DestExists(format!("{to} already exists")));
    }
    txn.rename_exclusive(&source_full, &dest_full)
        .map_err(|err| match err {
            Error::Io(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                MoveError::DestExists(format!("{to} already exists"))
            }
            Error::Io(error) => MoveError::Io(error.to_string()),
            Error::LockHeld { pid, since } => MoveError::LockHeld { pid, since },
            Error::Empty(label) => MoveError::Usage(format!("{label} is empty")),
        })?;
    maybe_crash("after-rename");
    let from = located.path.clone();
    let moved = Located {
        path: to.clone(),
        ..located
    };
    let rewritten = if rewrite {
        rewrite_incoming(root, &from, &moved, txn)?
    } else {
        Vec::new()
    };
    Ok(MoveResult {
        id: moved.id,
        from,
        to,
        alias_added,
        rewritten,
    })
}

fn link_from(root: &Path, query: &str, current: &str) -> String {
    let query = query.trim();
    if missing_path_arg(root, query) {
        query.trim_end_matches('/').to_string()
    } else {
        current.to_string()
    }
}

#[derive(Clone)]
struct Located {
    id: String,
    path: String,
    file_name: String,
    slug: String,
    aliases: Vec<String>,
    archived: bool,
}

impl Located {
    fn dest_slug(&self, to: &str) -> String {
        Path::new(to)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("")
            .to_string()
    }
}

enum DestShape {
    Directory { dir: String },
    File { rel: String },
}

fn locate(
    index: &VaultIndex,
    root: &Path,
    note: &str,
    shape: &DestShape,
) -> Result<Located, MoveError> {
    let query = note.trim();
    if query.is_empty() {
        return Err(MoveError::Usage("note is empty".to_string()));
    }
    match index.resolve_argument(query) {
        Ok(Resolved::Note { id, path }) => load_located(root, id, path),
        Ok(Resolved::File { path }) => Err(MoveError::NotFound(format!("no note matches {path}"))),
        Err(ResolveError::Outside) => Err(MoveError::OutsideVault(format!(
            "{query} is outside the vault"
        ))),
        Err(ResolveError::Ambiguous { candidates }) => Err(MoveError::Ambiguous { candidates }),
        Err(ResolveError::Missing) if missing_path_arg(root, query) => {
            recover_basename(index, root, query, shape)
        }
        Err(ResolveError::Missing) => Err(MoveError::NotFound(format!("no note matches {query}"))),
    }
}

fn missing_path_arg(root: &Path, query: &str) -> bool {
    let path_like = query.contains('/') || query.ends_with(".md") || Path::new(query).is_absolute();
    path_like && !root.join(query).exists()
}

fn recover_basename(
    index: &VaultIndex,
    root: &Path,
    query: &str,
    shape: &DestShape,
) -> Result<Located, MoveError> {
    let stem = Path::new(query)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    if stem.is_empty() {
        return Err(MoveError::NotFound(format!("no note matches {query}")));
    }
    let resolved = match index.resolve(stem) {
        Ok(Resolved::Note { id, path }) => (id, path),
        Ok(Resolved::File { .. }) | Err(ResolveError::Missing) => {
            return Err(MoveError::NotFound(format!("no note matches {query}")));
        }
        Err(ResolveError::Ambiguous { candidates }) => {
            return Err(MoveError::Ambiguous { candidates });
        }
        Err(ResolveError::Outside) => {
            return Err(MoveError::NotFound(format!("no note matches {query}")));
        }
    };
    let located = load_located(root, resolved.0, resolved.1)?;
    let to = match shape {
        DestShape::Directory { dir } => format!("{dir}{}", located.file_name),
        DestShape::File { rel } => rel.clone(),
    };
    if located.path != to {
        return Err(MoveError::NotFound(format!("no note matches {query}")));
    }
    Ok(located)
}

fn load_located(root: &Path, id: String, path: String) -> Result<Located, MoveError> {
    let bytes = regular_file_inside(root, &path)?;
    let text =
        String::from_utf8(bytes).map_err(|_| MoveError::Io(format!("{path} is not UTF-8")))?;
    let parsed = frontmatter::parse_note(&text).map_err(|err| match err {
        FrontmatterError::Missing => MoveError::Usage(format!("{path} has no frontmatter")),
        other => MoveError::Io(other.to_string()),
    })?;
    let slug = Path::new(&path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_string();
    let file_name = Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    let aliases = match parsed.values.get("aliases") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(text)) if !text.is_empty() => vec![text.clone()],
        _ => Vec::new(),
    };
    let archived = parsed
        .values
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status == "archived");
    Ok(Located {
        id,
        path,
        file_name,
        slug,
        aliases,
        archived,
    })
}

fn dest_shape(root: &Path, dest: &str) -> Result<DestShape, MoveError> {
    let raw = dest.trim();
    if raw.is_empty() {
        return Err(MoveError::Usage("destination is empty".to_string()));
    }
    let rel = vault_relative(root, raw)?;
    refuse_reserved(&rel)?;
    let as_dir = raw.ends_with('/') || root.join(&rel).is_dir();
    if as_dir {
        let dir = if rel.is_empty() {
            String::new()
        } else if rel.ends_with('/') {
            rel
        } else {
            format!("{rel}/")
        };
        return Ok(DestShape::Directory { dir });
    }
    if !rel.ends_with(".md") {
        return Err(MoveError::Usage(
            "destination must end in .md or be a directory".to_string(),
        ));
    }
    Ok(DestShape::File { rel })
}

fn vault_relative(root: &Path, dest: &str) -> Result<String, MoveError> {
    let path = Path::new(dest);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(MoveError::OutsideVault(format!(
            "{dest} is outside the vault"
        )));
    }
    if path.is_absolute() {
        return match vault::canonicalize_within(root, path) {
            Ok((canon_root, canon)) => Ok(strip_root(&canon_root, &canon)),
            Err(ConfineError::Outside(message)) => Err(MoveError::OutsideVault(message)),
            Err(ConfineError::Io(_)) => absolute_missing(root, path, dest),
        };
    }
    lexical_relative(dest, path)
}

fn absolute_missing(root: &Path, path: &Path, dest: &str) -> Result<String, MoveError> {
    let (canon_root, _) = vault::canonicalize_within(root, root).map_err(confine_map)?;
    let mut root_parts = canon_root.components();
    let mut dest_parts = path.components();
    loop {
        match (root_parts.next(), dest_parts.next()) {
            (Some(left), Some(right)) if left == right => {}
            (None, None) => return Ok(String::new()),
            (None, Some(component)) => {
                let mut rest = vec![component];
                rest.extend(dest_parts);
                return lexical_relative(dest, &PathBuf::from_iter(rest));
            }
            _ => {
                return Err(MoveError::OutsideVault(format!(
                    "{dest} is outside the vault"
                )));
            }
        }
    }
}

fn lexical_relative(dest: &str, path: &Path) -> Result<String, MoveError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let text = part
                    .to_str()
                    .ok_or_else(|| MoveError::Usage("destination is not utf-8".to_string()))?;
                parts.push(text.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(MoveError::OutsideVault(format!(
                    "{dest} is outside the vault"
                )));
            }
        }
    }
    Ok(parts.join("/"))
}

fn confine_map(err: ConfineError) -> MoveError {
    match err {
        ConfineError::Outside(message) => MoveError::OutsideVault(message),
        ConfineError::Io(message) => MoveError::Io(message),
    }
}

fn strip_root(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn refuse_reserved(rel: &str) -> Result<(), MoveError> {
    if rel == "sources/raw" || rel.starts_with("sources/raw/") {
        return Err(MoveError::RawImmutable(
            "sources/raw is not rewritten".to_string(),
        ));
    }
    if rel == ".mnemic" || rel.starts_with(".mnemic/") {
        return Err(MoveError::Usage(
            "destination must not be under .mnemic".to_string(),
        ));
    }
    Ok(())
}

fn confined_file(root: &Path, rel: &str) -> Result<PathBuf, MoveError> {
    let joined = root.join(rel);
    match vault::canonicalize_within(root, &joined) {
        Ok((_, full)) => Ok(full),
        Err(ConfineError::Outside(message)) => Err(MoveError::OutsideVault(message)),
        Err(ConfineError::Io(message)) => Err(MoveError::Io(message)),
    }
}

fn confine_dest(root: &Path, rel: &str) -> Result<(), MoveError> {
    let mut cursor = root.to_path_buf();
    let parent = Path::new(rel).parent().unwrap_or(Path::new(""));
    for component in parent.components() {
        let Component::Normal(part) = component else {
            return Err(MoveError::OutsideVault(format!(
                "{rel} is outside the vault"
            )));
        };
        cursor.push(part);
        match vault::canonicalize_within(root, &cursor) {
            Ok((_, canon)) => cursor = canon,
            Err(ConfineError::Io(_)) => return Ok(()),
            Err(ConfineError::Outside(message)) => return Err(MoveError::OutsideVault(message)),
        }
    }
    Ok(())
}

fn prepare_dest(root: &Path, rel: &str) -> Result<PathBuf, MoveError> {
    let (_, mut cursor) = vault::canonicalize_within(root, root).map_err(confine_map)?;
    let path = Path::new(rel);
    let parent = path.parent().unwrap_or(Path::new(""));
    for component in parent.components() {
        let Component::Normal(part) = component else {
            return Err(MoveError::OutsideVault(format!(
                "{rel} is outside the vault"
            )));
        };
        cursor.push(part);
        if cursor.exists() {
            let (_, canon) = match vault::canonicalize_within(root, &cursor) {
                Ok(confined) => confined,
                Err(ConfineError::Outside(message)) => {
                    return Err(MoveError::OutsideVault(message))
                }
                Err(ConfineError::Io(message)) => return Err(MoveError::Io(message)),
            };
            cursor = canon;
        } else {
            fs::create_dir(&cursor).map_err(|err| MoveError::Io(err.to_string()))?;
        }
    }
    let name = path.file_name().ok_or_else(|| {
        MoveError::Usage("destination must end in .md or be a directory".to_string())
    })?;
    Ok(cursor.join(name))
}

fn dest_name_taken(root: &Path, rel: &str) -> Result<bool, MoveError> {
    let path = root.join(rel);
    match fs::symlink_metadata(&path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(MoveError::Io(err.to_string())),
    }
}

fn alias_present(aliases: &[String], slug: &str) -> bool {
    let key = fold_key(slug);
    aliases.iter().any(|alias| fold_key(alias) == key)
}

fn alias_taken(index: &VaultIndex, id: &str, slug: &str) -> bool {
    let key = fold_key(slug);
    index.notes().any(|note| {
        note.id != id
            && (fold_key(file_slug(note.path)) == key
                || note.aliases.iter().any(|alias| fold_key(alias) == key))
    })
}

fn slug_taken(index: &VaultIndex, id: &str, slug: &str) -> bool {
    alias_taken(index, id, slug)
}

fn file_slug(path: &str) -> &str {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
}

fn fold_key(value: &str) -> String {
    value.nfc().collect::<String>().to_lowercase()
}

fn fold_keep_case(value: &str) -> String {
    value.nfc().collect()
}

fn rewrite_incoming(
    root: &Path,
    from: &str,
    note: &Located,
    txn: &WriteTxn,
) -> Result<Vec<String>, MoveError> {
    let old_slug = file_slug(from).to_string();
    let old_rel = from.strip_suffix(".md").unwrap_or(from).to_string();
    let new_slug = note.slug_from_path();
    let new_rel = note.path.strip_suffix(".md").unwrap_or(&note.path);
    let mut paths: Vec<String> = VaultIndex::load(root)
        .map_err(|err| match err {
            crate::check::CheckFatal::Usage(message) | crate::check::CheckFatal::Io(message) => {
                MoveError::Io(message)
            }
        })?
        .notes()
        .map(|item| item.path.to_string())
        .filter(|path| path != from)
        .collect();
    if !paths.iter().any(|path| path == &note.path) {
        paths.push(note.path.clone());
    }
    paths.sort();
    let mut rewritten = Vec::new();
    for path in paths {
        let bytes = regular_file_inside(root, &path)?;
        let text =
            String::from_utf8(bytes).map_err(|_| MoveError::Io(format!("{path} is not UTF-8")))?;
        let Some(next) = rewrite_note(&text, &old_slug, &old_rel, &new_slug, new_rel) else {
            continue;
        };
        if next != text {
            let full = confined_file(root, &path)?;
            txn.atomic_write(&full, next.as_bytes())
                .map_err(|err| MoveError::Io(err.to_string()))?;
            rewritten.push(path);
        }
    }
    Ok(rewritten)
}

impl Located {
    fn slug_from_path(&self) -> String {
        file_slug(&self.path).to_string()
    }
}

fn rewrite_note(
    source: &str,
    old_slug: &str,
    old_rel: &str,
    new_slug: &str,
    new_rel: &str,
) -> Option<String> {
    let parsed = frontmatter::parse_note(source).ok()?;
    let body = &source[parsed.block.end..];
    let next_body = rewrite_body(body, old_slug, old_rel, new_slug, new_rel);
    if next_body == body {
        return None;
    }
    let mut out = String::with_capacity(source.len() + next_body.len());
    out.push_str(&source[..parsed.block.end]);
    out.push_str(&next_body);
    Some(out)
}

fn rewrite_body(
    body: &str,
    old_slug: &str,
    old_rel: &str,
    new_slug: &str,
    new_rel: &str,
) -> String {
    let links = wikilink_spans(body);
    if links.is_empty() {
        return body.to_string();
    }
    let mut out = body.to_string();
    for link in links.into_iter().rev() {
        let Some(replacement) =
            replacement_target(&link.inner, old_slug, old_rel, new_slug, new_rel)
        else {
            continue;
        };
        let (target, _, _) = split_link(&link.inner);
        if replacement == target.trim() {
            continue;
        }
        let start = link.inner_start + leading_ws(&link.inner);
        let end = start + target.trim().len();
        if end > link.inner_end || &out[start..end] != target.trim() {
            continue;
        }
        out.replace_range(start..end, &replacement);
    }
    out
}

fn leading_ws(inner: &str) -> usize {
    inner.len() - inner.trim_start().len()
}

fn replacement_target(
    inner: &str,
    old_slug: &str,
    old_rel: &str,
    new_slug: &str,
    new_rel: &str,
) -> Option<String> {
    let (target, _, _) = split_link(inner);
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    let key = fold_key(target);
    let old_slug_key = fold_key(old_slug);
    let old_rel_key = fold_key(old_rel);
    let old_md = fold_key(&format!("{old_rel}.md"));
    if key != old_slug_key && key != old_rel_key && key != old_md {
        return None;
    }
    if target.contains('/') {
        Some(new_rel.to_string())
    } else {
        Some(new_slug.to_string())
    }
}

fn split_link(inner: &str) -> (&str, Option<&str>, Option<&str>) {
    let (before_label, label) = match inner.split_once('|') {
        Some((target, label)) => (target, Some(label)),
        None => (inner, None),
    };
    let (target, heading) = match before_label.split_once('#') {
        Some((target, heading)) => (target, Some(heading)),
        None => (before_label, None),
    };
    (target, heading, label)
}

fn stamp(clock: &dyn Clock) -> String {
    let millis = i64::try_from(clock.unix_millis()).unwrap_or(0);
    let dt = DateTime::<Utc>::from_timestamp_millis(millis).unwrap_or(DateTime::UNIX_EPOCH);
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn read_note(root: &Path, rel: &str) -> Result<String, MoveError> {
    let bytes = regular_file_inside(root, rel)?;
    String::from_utf8(bytes).map_err(|_| MoveError::Io(format!("{rel} is not UTF-8")))
}

fn regular_file_inside(root: &Path, rel: &str) -> Result<Vec<u8>, MoveError> {
    let full = confined_file(root, rel)?;
    if !crate::check::regular_file_inside(root, &full) {
        return Err(MoveError::Io(format!(
            "{rel} is not a regular file in the vault"
        )));
    }
    fs::read(&full).map_err(|err| MoveError::Io(err.to_string()))
}

fn maybe_crash(point: &str) {
    #[cfg(debug_assertions)]
    if std::env::var(CRASH_ENV).ok().as_deref() == Some(point) {
        std::process::exit(97);
    }
    #[cfg(not(debug_assertions))]
    let _ = point;
}

fn vault_error(err: VaultError) -> MoveError {
    match err {
        VaultError::NotFound => MoveError::Vault(err.to_string()),
        VaultError::BadConfig(message) | VaultError::Io(message) => MoveError::Vault(message),
        VaultError::Unsupported { .. } => MoveError::Vault(err.to_string()),
    }
}

fn lock_error(err: Error) -> MoveError {
    match err {
        Error::LockHeld { pid, since } => MoveError::LockHeld { pid, since },
        Error::Io(err) => MoveError::Io(err.to_string()),
        Error::Empty(label) => MoveError::Usage(format!("{label} is empty")),
    }
}
