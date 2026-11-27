//! Create one note from a type template.
//!
//! Identity checks and the `O_EXCL` create run inside [`crate::write::with_write_lock`].
//! The same id at the same path is a no-op and does not rewrite the file.

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, NaiveDate};
use serde::Serialize;

use crate::clock::Clock;
use crate::edit::FrontmatterEdit;
use crate::fields::{self, FieldEdit};
use crate::index::{ResolveError, Resolved, VaultIndex};
use crate::pack::{self, PackError, TypeDef};
use crate::ulid::{is_ulid, UlidGenerator};
use crate::vault::{self, ConfineError, VaultError};
use crate::write::{self, WriteTxn};
use crate::Error;

const AUTHOR_USAGE: &str = "set an author with --as or actor in mnemic.toml";

/// Inputs for [`create_note`].
pub struct NewRequest<'a> {
    pub vault: &'a Path,
    pub type_name: &'a str,
    pub title: &'a str,
    pub id: Option<&'a str>,
    pub actor: Option<&'a str>,
    pub date: Option<&'a str>,
    /// Source frontmatter `path`. Other types reject the flag.
    pub path: Option<&'a str>,
    /// Source frontmatter `url`. Other types reject the flag.
    pub url: Option<&'a str>,
    /// `key=value` pairs written after the core keys.
    pub fields: &'a [String],
    pub clock: &'a dyn Clock,
    pub ids: &'a mut UlidGenerator,
    pub lock_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewResult {
    pub id: String,
    pub path: String,
    pub noop: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NewSuccess {
    pub ok: bool,
    pub data: NewData,
    pub error: Option<()>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewData {
    pub id: String,
    pub path: String,
    pub noop: bool,
}

impl NewResult {
    pub fn success(&self) -> NewSuccess {
        NewSuccess {
            ok: true,
            data: NewData {
                id: self.id.clone(),
                path: self.path.clone(),
                noop: self.noop,
            },
            error: None,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NewError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    UnknownType(String),
    #[error("{0}")]
    BadId(String),
    #[error("{0}")]
    IdExists(String),
    #[error("{0}")]
    SlugCollision(String),
    #[error("{0}")]
    MissingTemplate(String),
    #[error("{0}")]
    OutsideVault(String),
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

/// Title to a filename slug: `deunicode`, then lowercase ASCII, with
/// spaces and punctuation collapsed to single dashes.
pub fn slugify(title: &str) -> String {
    let ascii = deunicode::deunicode(title);
    let mut out = String::new();
    let mut dash = false;
    for ch in ascii.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

/// Create a note, or return the existing path when `id` is already there.
pub fn create_note(request: NewRequest<'_>) -> Result<NewResult, NewError> {
    let slug = slugify(request.title);
    if slug.is_empty() {
        return Err(NewError::Usage("title slugs to empty".to_string()));
    }
    if let Some(id) = request.id {
        if !is_ulid(id) {
            return Err(NewError::BadId("--id must be a Crockford ULID".to_string()));
        }
    }
    if let Some(date) = request.date {
        parse_iso_date(date)?;
    }
    let assignments = request
        .fields
        .iter()
        .map(|raw| fields::parse_assignment(raw, false))
        .collect::<Result<Vec<_>, _>>()
        .map_err(NewError::Usage)?;
    if request.path.is_some() && assignments.iter().any(|field| field.key == "path") {
        return Err(NewError::Usage(
            "--path and --field path= cannot be combined".to_string(),
        ));
    }
    if request.url.is_some() && assignments.iter().any(|field| field.key == "url") {
        return Err(NewError::Usage(
            "--url and --field url= cannot be combined".to_string(),
        ));
    }
    if request.date.is_some() && assignments.iter().any(|field| field.key == "date") {
        return Err(NewError::Usage(
            "--date and --field date= cannot be combined".to_string(),
        ));
    }
    let (root, config) =
        vault::open_config(request.vault, Some(request.vault)).map_err(vault_error)?;
    let actor = author(request.actor, config.actor.as_deref())?;
    let pack = pack::resolve_pack(&root).map_err(pack_error)?;
    let def = pack
        .get(request.type_name)
        .ok_or_else(|| NewError::UnknownType(format!("unknown type {}", request.type_name)))?;
    let prepared =
        fields::edits_for(request.type_name, def, &assignments).map_err(NewError::Usage)?;
    let mut extras = Vec::new();
    for edit in prepared {
        match edit {
            FieldEdit::Replace { key, yaml, .. } => extras.push((key, yaml)),
            FieldEdit::Add { key, .. } => {
                return Err(NewError::Usage(format!(
                    "--add applies to strings and tags, not {key}"
                )));
            }
        }
    }
    let (source_path, source_url) = source_locators(
        request.type_name,
        request.path,
        request.url,
        extras.iter().any(|(key, _)| key == "path"),
        extras.iter().any(|(key, _)| key == "url"),
    )?;
    let mut present: BTreeSet<&str> = BTreeSet::new();
    present.insert("title");
    if request.type_name == "journal" || extras.iter().any(|(key, _)| key == "date") {
        present.insert("date");
    }
    if source_path.is_some() || extras.iter().any(|(key, _)| key == "path") {
        present.insert("path");
    }
    if source_url.is_some() || extras.iter().any(|(key, _)| key == "url") {
        present.insert("url");
    }
    for (key, _) in &extras {
        present.insert(key);
    }
    if let Some(message) = fields::missing_required(def, &present) {
        return Err(NewError::Usage(message));
    }
    let NewRequest {
        type_name,
        title,
        id,
        date,
        clock,
        ids,
        lock_timeout,
        ..
    } = request;
    let mut draft = Draft {
        type_name,
        title,
        slug: &slug,
        id,
        actor: &actor,
        date,
        source_path: source_path.as_deref(),
        source_url: source_url.as_deref(),
        extras,
        clock,
        ids,
    };
    match write::with_write_lock(&root, lock_timeout, |txn| {
        Ok(create_locked(&root, &mut draft, txn))
    }) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(lock_error(err)),
    }
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// `--path` and `--url` are source locators. Other types reject them.
/// A source note needs at least one trimmed, non-empty value.
fn source_locators(
    type_name: &str,
    path: Option<&str>,
    url: Option<&str>,
    field_path: bool,
    field_url: bool,
) -> Result<(Option<String>, Option<String>), NewError> {
    if type_name != "source" {
        if path.is_some() || url.is_some() {
            return Err(NewError::Usage(
                "--path and --url apply only to source notes".to_string(),
            ));
        }
        return Ok((None, None));
    }
    let path = nonempty(path);
    let url = nonempty(url);
    if path.is_none() && url.is_none() && !field_path && !field_url {
        return Err(NewError::Usage(
            "source notes need a non-empty --path or --url".to_string(),
        ));
    }
    Ok((path, url))
}

fn author(flag: Option<&str>, configured: Option<&str>) -> Result<String, NewError> {
    match flag.map(str::trim).filter(|value| !value.is_empty()) {
        Some(actor) => Ok(actor.to_string()),
        None => match configured.map(str::trim).filter(|value| !value.is_empty()) {
            Some(actor) => Ok(actor.to_string()),
            None => Err(NewError::Usage(AUTHOR_USAGE.to_string())),
        },
    }
}

struct Draft<'a> {
    type_name: &'a str,
    title: &'a str,
    slug: &'a str,
    id: Option<&'a str>,
    actor: &'a str,
    date: Option<&'a str>,
    source_path: Option<&'a str>,
    source_url: Option<&'a str>,
    extras: Vec<(String, String)>,
    clock: &'a dyn Clock,
    ids: &'a mut UlidGenerator,
}

fn create_locked(
    root: &Path,
    draft: &mut Draft<'_>,
    txn: &WriteTxn,
) -> Result<NewResult, NewError> {
    let pack = pack::resolve_pack(root).map_err(pack_error)?;
    let def = pack
        .get(draft.type_name)
        .ok_or_else(|| NewError::UnknownType(format!("unknown type {}", draft.type_name)))?;
    if draft.date.is_some() && def.name != "journal" {
        return Err(NewError::Usage(
            "--date applies to journal notes".to_string(),
        ));
    }
    let body = match pack::template_body(root, draft.type_name).map_err(pack_error)? {
        Some(body) => body,
        None => {
            return Err(NewError::MissingTemplate(format!(
                "missing template for {}",
                draft.type_name
            )))
        }
    };
    let journal_date = if def.name == "journal" {
        Some(match draft.date {
            Some(date) => date.to_string(),
            None => draft
                .extras
                .iter()
                .find(|(key, _)| key == "date")
                .map(|(_, yaml)| yaml.clone())
                .unwrap_or_else(local_iso_date),
        })
    } else {
        None
    };
    if journal_date.is_some() {
        draft.extras.retain(|(key, _)| key != "date");
    }
    let index = VaultIndex::load(root).map_err(|err| match err {
        crate::check::CheckFatal::Usage(message) | crate::check::CheckFatal::Io(message) => {
            NewError::Io(message)
        }
    })?;
    let rels = destination_rels(def, draft.slug, journal_date.as_deref())?;
    let rel = choose_rel(root, &index, &rels, draft.id)?;
    if let Some(id) = draft.id {
        match index.resolve(id) {
            Ok(Resolved::Note { path, .. }) if path == rel => {
                return Ok(NewResult {
                    id: id.to_string(),
                    path: rel,
                    noop: true,
                });
            }
            Ok(Resolved::Note { path, .. }) => {
                return Err(NewError::IdExists(format!(
                    "id {id} already exists at {path}"
                )));
            }
            Err(ResolveError::Ambiguous { .. }) => {
                return Err(NewError::IdExists(format!("id {id} already exists")));
            }
            _ => {}
        }
    }
    let stem = file_stem(&rel);
    if slug_taken(&index, &stem) {
        return Err(NewError::SlugCollision(format!(
            "slug {stem} matches another note"
        )));
    }
    let full = confine_destination(root, &rel)?;
    let new_id = match draft.id {
        Some(id) => id.to_string(),
        None => draft.ids.next(&ClockRef(draft.clock)),
    };
    let bytes = render_note(&Rendered {
        id: &new_id,
        type_name: draft.type_name,
        schema_version: def.schema_version,
        title: draft.title,
        stamp: &stamp(draft.clock),
        actor: draft.actor,
        date: journal_date.as_deref(),
        source_path: draft.source_path,
        source_url: draft.source_url,
        extras: &draft.extras,
        body: &body,
    })?;
    match txn.create_exclusive(&full, bytes.as_bytes()) {
        Ok(()) => Ok(NewResult {
            id: new_id,
            path: rel,
            noop: false,
        }),
        Err(Error::Io(err)) if err.kind() == ErrorKind::AlreadyExists => Err(
            NewError::SlugCollision(format!("slug {stem} matches another note")),
        ),
        Err(err) => Err(NewError::Io(err.to_string())),
    }
}

fn destination_rels(
    def: &TypeDef,
    slug: &str,
    journal_date: Option<&str>,
) -> Result<Vec<String>, NewError> {
    let folder = def
        .folders
        .first()
        .filter(|folder| plain_relative(folder))
        .ok_or_else(|| NewError::OutsideVault(format!("type {} has no vault folder", def.name)))?;
    if def.name == "journal" {
        let date = journal_date.unwrap_or("");
        let year = date.get(..4).unwrap_or("");
        Ok(vec![
            format!("{folder}/{year}/{date}.md"),
            format!("{folder}/{year}/{date}-{slug}.md"),
        ])
    } else {
        Ok(vec![format!("{folder}/{slug}.md")])
    }
}

fn choose_rel(
    root: &Path,
    index: &VaultIndex,
    rels: &[String],
    id: Option<&str>,
) -> Result<String, NewError> {
    if let Some(id) = id {
        if let Some(existing) = id_path(index, id) {
            if rels.iter().any(|rel| rel == &existing) {
                return Ok(existing);
            }
        }
    }
    for rel in rels {
        if !path_taken(root, rel)? {
            return Ok(rel.clone());
        }
    }
    Err(NewError::SlugCollision(
        "slug matches another note".to_string(),
    ))
}

fn path_taken(root: &Path, rel: &str) -> Result<bool, NewError> {
    let path = root.join(rel);
    match fs::symlink_metadata(&path) {
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
        Err(err) => Err(NewError::Io(err.to_string())),
        Ok(meta) if meta.file_type().is_symlink() => {
            match vault::canonicalize_within(root, &path) {
                Ok((_, canon)) if canon.is_file() => Ok(true),
                Ok(_) => Err(NewError::OutsideVault(format!(
                    "{rel} is outside the vault"
                ))),
                Err(ConfineError::Outside(_)) => Err(NewError::OutsideVault(format!(
                    "{rel} is outside the vault"
                ))),
                Err(ConfineError::Io(message)) => Err(NewError::Io(message)),
            }
        }
        Ok(_) => Ok(true),
    }
}

fn confine_destination(root: &Path, rel: &str) -> Result<PathBuf, NewError> {
    if !plain_relative(rel) {
        return Err(NewError::OutsideVault(format!(
            "{rel} is outside the vault"
        )));
    }
    let mut acc = root.to_path_buf();
    let path = Path::new(rel);
    let mut parts = path.components();
    let name = parts.next_back();
    for component in parts {
        acc.push(component);
        if fs::symlink_metadata(&acc).is_err() {
            fs::create_dir(&acc).map_err(|err| NewError::Io(err.to_string()))?;
        }
        match vault::canonicalize_within(root, &acc) {
            Ok(_) => {}
            Err(ConfineError::Outside(_)) => {
                return Err(NewError::OutsideVault(format!(
                    "{rel} is outside the vault"
                )))
            }
            Err(ConfineError::Io(message)) => return Err(NewError::Io(message)),
        }
    }
    let Some(Component::Normal(name)) = name else {
        return Err(NewError::OutsideVault(format!(
            "{rel} is outside the vault"
        )));
    };
    Ok(acc.join(name))
}

fn plain_relative(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn id_path(index: &VaultIndex, id: &str) -> Option<String> {
    match index.resolve(id) {
        Ok(Resolved::Note { path, .. }) => Some(path),
        _ => None,
    }
}

fn slug_taken(index: &VaultIndex, slug: &str) -> bool {
    matches!(
        index.resolve(slug),
        Ok(Resolved::Note { .. }) | Err(ResolveError::Ambiguous { .. })
    )
}

fn file_stem(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_string()
}

struct Rendered<'a> {
    id: &'a str,
    type_name: &'a str,
    schema_version: u64,
    title: &'a str,
    stamp: &'a str,
    actor: &'a str,
    date: Option<&'a str>,
    source_path: Option<&'a str>,
    source_url: Option<&'a str>,
    extras: &'a [(String, String)],
    body: &'a str,
}

fn render_note(note: &Rendered<'_>) -> Result<String, NewError> {
    let mut text = format!(
        "---\nid: {id}\ntype: {type_name}\nschema_version: {schema_version}\ntitle: {}\ncreated: {stamp}\nupdated: {stamp}\ncreated_by: {}\nstatus: active\n",
        yaml_string(note.title),
        yaml_string(note.actor),
        id = note.id,
        type_name = note.type_name,
        schema_version = note.schema_version,
        stamp = note.stamp,
    );
    if let Some(date) = note.date {
        text.push_str("date: ");
        text.push_str(date);
        text.push('\n');
    }
    if let Some(path) = note.source_path {
        text.push_str("path: ");
        text.push_str(&yaml_string(path));
        text.push('\n');
    }
    if let Some(url) = note.source_url {
        text.push_str("url: ");
        text.push_str(&yaml_string(url));
        text.push('\n');
    }
    text.push_str("---\n");
    text.push_str(note.body);
    if !note.body.ends_with('\n') {
        text.push('\n');
    }
    for (key, yaml) in note.extras {
        let parsed =
            crate::frontmatter::parse_note(&text).map_err(|err| NewError::Io(err.to_string()))?;
        text = if parsed.values.contains_key(key) {
            FrontmatterEdit::replace(&text, key, yaml)
        } else {
            FrontmatterEdit::insert(&text, key, yaml)
        }
        .map_err(|err| NewError::Io(err.to_string()))?;
    }
    Ok(text)
}

fn yaml_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

struct ClockRef<'a>(&'a dyn Clock);

impl Clock for ClockRef<'_> {
    fn unix_millis(&self) -> u64 {
        self.0.unix_millis()
    }
}

fn stamp(clock: &dyn Clock) -> String {
    let millis = i64::try_from(clock.unix_millis()).unwrap_or(0);
    let dt = DateTime::from_timestamp_millis(millis).unwrap_or(DateTime::UNIX_EPOCH);
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn local_iso_date() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn parse_iso_date(value: &str) -> Result<NaiveDate, NewError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| NewError::Usage(format!("--date must be an ISO date, got {value}")))
}

fn pack_error(err: PackError) -> NewError {
    NewError::Io(err.to_string())
}

fn vault_error(err: VaultError) -> NewError {
    match err {
        VaultError::NotFound => NewError::Vault(err.to_string()),
        VaultError::BadConfig(message) => NewError::Vault(message),
        VaultError::Unsupported { .. } => NewError::Vault(err.to_string()),
        VaultError::Io(message) => NewError::Io(message),
    }
}

fn lock_error(err: Error) -> NewError {
    match err {
        Error::LockHeld { pid, since } => NewError::LockHeld { pid, since },
        Error::Io(err) => NewError::Io(err.to_string()),
        Error::Empty(label) => NewError::Usage(format!("{label} is empty")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_transliterates_and_collapses() {
        assert_eq!(slugify("Test Person"), "test-person");
        assert_eq!(slugify("Café: \"hi\" #1"), "cafe-hi-1");
        assert_eq!(slugify("straße"), "strasse");
        assert_eq!(slugify("  ---  "), "");
        assert_eq!(slugify("???"), "");
    }
}
