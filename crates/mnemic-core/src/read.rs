use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::index::{LinkEdge, VaultIndex};
use crate::pack;
use crate::vault::{canonicalize_within, ConfineError};

/// A note returned by `get`, with edges from [`VaultIndex::link_sets`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoteView {
    pub id: String,
    pub path: String,
    pub slug: String,
    pub title: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub status: String,
    pub frontmatter: Map<String, Value>,
    pub body: String,
    pub outgoing: Vec<LinkEdge>,
    pub backlinks: Vec<LinkEdge>,
    pub broken: Vec<LinkEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReadError {
    #[error("no note matches {0}")]
    NotFound(String),
    #[error("{query} matches {}", listed(.candidates))]
    Ambiguous {
        query: String,
        candidates: Vec<Candidate>,
    },
    #[error("{0} is outside the vault")]
    OutsideVault(String),
    #[error("{0}")]
    Io(String),
}

fn listed(candidates: &[Candidate]) -> String {
    candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolve `query` through [`VaultIndex`] and read that note.
///
/// User arguments enter through [`VaultIndex::resolve_argument`], which
/// returns [`crate::index::ResolveError::Outside`] before the file is opened.
/// The note file is confined again before its bytes are read.
pub fn read_note(vault_root: &Path, query: &str) -> Result<NoteView, ReadError> {
    let root = fs::canonicalize(vault_root).map_err(|err| ReadError::Io(err.to_string()))?;
    if query.trim().is_empty() {
        return Err(ReadError::NotFound(query.to_string()));
    }
    let index = VaultIndex::load(&root)?;
    let pack = pack::resolve_pack(&root).map_err(|err| ReadError::Io(err.to_string()))?;
    let resolved = index
        .resolve_argument(query)
        .map_err(|err| map_resolve(&index, query, err))?;
    let crate::index::Resolved::Note { id, path } = resolved else {
        return Err(ReadError::NotFound(query.to_string()));
    };
    let text = read_confined(&root, &path)?;
    let parsed =
        crate::frontmatter::parse_note(&text).map_err(|err| ReadError::Io(err.to_string()))?;
    let sets = index.link_sets(&id, &pack);
    let slug = Path::new(&path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_string();
    Ok(NoteView {
        id,
        path,
        slug,
        title: string_field(&parsed.values, "title"),
        type_name: string_field(&parsed.values, "type"),
        status: string_field(&parsed.values, "status"),
        frontmatter: parsed.values.into_iter().collect(),
        body: parsed.body,
        outgoing: sets.outgoing,
        backlinks: sets.backlinks,
        broken: sets.broken,
    })
}

fn string_field(values: &std::collections::BTreeMap<String, Value>, key: &str) -> String {
    values
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn read_confined(root: &Path, rel: &str) -> Result<String, ReadError> {
    let joined = root.join(rel);
    let (_root, path) = canonicalize_within(root, &joined).map_err(|err| match err {
        ConfineError::Outside(_) => ReadError::OutsideVault(rel.to_string()),
        ConfineError::Io(message) => ReadError::Io(message),
    })?;
    fs::read_to_string(&path).map_err(|err| ReadError::Io(err.to_string()))
}

fn map_resolve(index: &VaultIndex, query: &str, err: crate::index::ResolveError) -> ReadError {
    match err {
        crate::index::ResolveError::Missing => ReadError::NotFound(query.to_string()),
        crate::index::ResolveError::Outside => ReadError::OutsideVault(query.to_string()),
        crate::index::ResolveError::Ambiguous { candidates } => {
            let mut rows: Vec<Candidate> = candidates
                .into_iter()
                .map(|path| Candidate {
                    id: index.id_at(&path).unwrap_or("").to_string(),
                    path,
                })
                .collect();
            rows.sort_by(|left, right| left.path.cmp(&right.path));
            ReadError::Ambiguous {
                query: query.to_string(),
                candidates: rows,
            }
        }
    }
}

impl From<crate::check::CheckFatal> for ReadError {
    fn from(err: crate::check::CheckFatal) -> Self {
        match err {
            crate::check::CheckFatal::Io(message) | crate::check::CheckFatal::Usage(message) => {
                ReadError::Io(message)
            }
        }
    }
}
