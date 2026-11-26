use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::thread;

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::check::{collect_notes, rel_path, CheckFatal};
use crate::contract::{Finding, FindingCode};
use crate::frontmatter::{self, ParsedNote};
use crate::pack::Pack;

/// One vault-wide id, path, slug, and alias map.
///
/// [`VaultIndex::resolve`] is the resolver `check` uses. Later verbs call the
/// same method. Resolution order is vault-relative path (with or without
/// `.md`), then exact `id`, then filename slug, then alias. Slug and alias
/// matches are case-insensitive after NFC normalization. Two or more slug or
/// alias hits are [`ResolveError::Ambiguous`]. A link does not guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultIndex {
    root: PathBuf,
    notes: Vec<NoteRec>,
    by_id: BTreeMap<String, Vec<usize>>,
    by_path: BTreeMap<String, Vec<usize>>,
    by_slug: BTreeMap<String, Vec<usize>>,
    by_alias: BTreeMap<String, Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NoteRec {
    id: String,
    path: String,
    aliases: Vec<String>,
    status: String,
    type_name: String,
    title: String,
    tags: Vec<String>,
    updated: String,
    body: String,
    supersedes: Vec<String>,
    wikilinks: Vec<(bool, String)>,
    typed: Vec<(String, String)>,
    source_path: Option<String>,
    sha256: Option<String>,
}

/// One parsed note from [`VaultIndex`]. Search reads these fields.
/// It does not walk the vault again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexedNote<'a> {
    pub id: &'a str,
    pub path: &'a str,
    pub type_name: &'a str,
    pub title: &'a str,
    pub aliases: &'a [String],
    pub tags: &'a [String],
    pub updated: &'a str,
    pub body: &'a str,
}

/// A resolved wikilink or typed frontmatter link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Note { id: String, path: String },
    File { path: String },
}

/// Why [`VaultIndex::resolve`] did not return one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Missing,
    Ambiguous {
        candidates: Vec<String>,
    },
    /// A user path argument leaves the vault. Wikilink resolution does not
    /// return this. [`VaultIndex::resolve_argument`] does.
    Outside,
}

/// One wikilink, embed, or typed frontmatter edge.
///
/// `kind` is `wikilink`, `embed`, or `frontmatter:<field>`. `target` is the
/// note id when [`VaultIndex::resolve`] returns a note. `source` is the id of
/// the note that contains the link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkEdge {
    pub kind: String,
    pub raw: String,
    pub source: String,
    pub target: Option<String>,
}

/// Outgoing edges, backlinks, and unresolved edges for one note id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSets {
    pub outgoing: Vec<LinkEdge>,
    pub backlinks: Vec<LinkEdge>,
    pub broken: Vec<LinkEdge>,
}

/// Load every note under `root`. `root` is the vault directory.
pub fn index_vault(root: &Path) -> Result<VaultIndex, CheckFatal> {
    VaultIndex::load(root)
}

impl VaultIndex {
    pub fn load(root: &Path) -> Result<Self, CheckFatal> {
        let root = fs::canonicalize(root).map_err(|err| CheckFatal::Io(err.to_string()))?;
        let mut files = Vec::new();
        collect_notes(&root, &root, &mut files)?;
        files.sort();
        let notes = read_notes(&root, &files)?;
        Ok(Self::from_notes(root, notes))
    }

    /// Parsed notes in vault-relative path order.
    pub fn notes(&self) -> impl Iterator<Item = IndexedNote<'_>> {
        self.notes.iter().map(|note| IndexedNote {
            id: &note.id,
            path: &note.path,
            type_name: &note.type_name,
            title: &note.title,
            aliases: &note.aliases,
            tags: &note.tags,
            updated: &note.updated,
            body: &note.body,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn from_notes(root: PathBuf, notes: Vec<NoteRec>) -> Self {
        let mut by_id: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_path: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_slug: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_alias: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (index, note) in notes.iter().enumerate() {
            by_id.entry(note.id.clone()).or_default().push(index);
            by_path.entry(nfc(&note.path)).or_default().push(index);
            let slug = Path::new(&note.path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("");
            by_slug.entry(fold_key(slug)).or_default().push(index);
            let mut seen = BTreeSet::new();
            for alias in &note.aliases {
                let key = fold_key(alias);
                if seen.insert(key.clone()) {
                    by_alias.entry(key).or_default().push(index);
                }
            }
        }
        Self {
            root,
            notes,
            by_id,
            by_path,
            by_slug,
            by_alias,
        }
    }

    /// Resolve a user note argument from `get`, `links`, or `log`.
    ///
    /// `..`, an absolute path outside the vault, and a path whose canonical
    /// form leaves the vault are [`ResolveError::Outside`]. The check is
    /// [`crate::vault::canonicalize_within`]. It does not open the target.
    /// A bare id, slug, or alias continues into [`VaultIndex::resolve`].
    pub fn resolve_argument(&self, query: &str) -> Result<Resolved, ResolveError> {
        let query = confine_user_path(&self.root, query)?;
        self.resolve(&query)
    }

    /// Resolve the inside of a wikilink. A `#heading` is ignored. A `|label`
    /// is ignored. `![[...]]` callers pass the inside without the embed mark.
    pub fn resolve(&self, link: &str) -> Result<Resolved, ResolveError> {
        let target = link_target(link);
        if target.is_empty() {
            return Err(ResolveError::Missing);
        }
        if let Some(found) = self.resolve_path(&target) {
            return found;
        }
        if let Some(idxs) = self.by_id.get(&target) {
            return self.one_note(idxs);
        }
        let folded = fold_key(&target);
        if let Some(idxs) = self.by_slug.get(&folded) {
            return self.one_note(idxs);
        }
        if let Some(idxs) = self.by_alias.get(&folded) {
            return self.one_note(idxs);
        }
        Err(ResolveError::Missing)
    }

    fn resolve_path(&self, target: &str) -> Option<Result<Resolved, ResolveError>> {
        let rel = safe_rel(target)?;
        let key = nfc(&rel);
        if let Some(idxs) = self.by_path.get(&key) {
            return Some(self.one_note(idxs));
        }
        if self.file_exists(&rel) {
            return Some(Ok(Resolved::File { path: rel }));
        }
        if !rel.ends_with(".md") {
            let with_md = format!("{rel}.md");
            if let Some(idxs) = self.by_path.get(&nfc(&with_md)) {
                return Some(self.one_note(idxs));
            }
        } else if let Some(stripped) = rel.strip_suffix(".md") {
            if self.file_exists(stripped) {
                return Some(Ok(Resolved::File {
                    path: stripped.to_string(),
                }));
            }
        }
        None
    }

    fn file_exists(&self, rel: &str) -> bool {
        let path = self.root.join(rel);
        path.is_file() && !crate::check::is_note(&self.root, &path)
    }

    fn one_note(&self, idxs: &[usize]) -> Result<Resolved, ResolveError> {
        if idxs.len() == 1 {
            let note = &self.notes[idxs[0]];
            return Ok(Resolved::Note {
                id: note.id.clone(),
                path: note.path.clone(),
            });
        }
        let mut candidates: Vec<String> = idxs
            .iter()
            .map(|idx| self.notes[*idx].path.clone())
            .collect();
        candidates.sort();
        Err(ResolveError::Ambiguous { candidates })
    }

    pub(crate) fn vault_findings(
        &self,
        pack: &Pack,
        scope: &str,
        scope_is_file: bool,
    ) -> (Vec<Finding>, BTreeMap<String, Value>) {
        let mut findings = Vec::new();
        findings.extend(self.duplicate_ids(scope, scope_is_file));
        findings.extend(self.alias_clashes(scope, scope_is_file));
        findings.extend(self.link_findings(pack, scope, scope_is_file));
        findings.extend(self.supersession(scope, scope_is_file));
        findings.extend(self.hash_drift(scope, scope_is_file));
        let details = candidate_details(&findings);
        (findings, details)
    }

    fn duplicate_ids(&self, scope: &str, scope_is_file: bool) -> Vec<Finding> {
        let mut findings = Vec::new();
        for (id, idxs) in &self.by_id {
            if idxs.len() < 2 {
                continue;
            }
            let paths: Vec<&str> = idxs
                .iter()
                .map(|idx| self.notes[*idx].path.as_str())
                .collect();
            for idx in idxs {
                let path = &self.notes[*idx].path;
                if !in_scope(path, scope, scope_is_file) {
                    continue;
                }
                let others: Vec<&str> = paths
                    .iter()
                    .copied()
                    .filter(|other| *other != path)
                    .collect();
                findings.push(finding(
                    FindingCode::DuplicateId,
                    format!("id {id} is also in {}", others.join(", ")),
                    path.clone(),
                ));
            }
        }
        findings
    }

    fn alias_clashes(&self, scope: &str, scope_is_file: bool) -> Vec<Finding> {
        let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for note in &self.notes {
            for alias in &note.aliases {
                owners
                    .entry(fold_key(alias))
                    .or_default()
                    .insert(note.id.clone());
            }
        }
        let mut findings = Vec::new();
        for (key, ids) in owners {
            if ids.len() < 2 {
                continue;
            }
            for note in &self.notes {
                if !in_scope(&note.path, scope, scope_is_file) {
                    continue;
                }
                let Some(alias) = note.aliases.iter().find(|alias| fold_key(alias) == key) else {
                    continue;
                };
                let others: Vec<&str> = self
                    .notes
                    .iter()
                    .filter(|other| other.id != note.id && ids.contains(&other.id))
                    .filter(|other| other.aliases.iter().any(|item| fold_key(item) == key))
                    .map(|other| other.path.as_str())
                    .collect();
                findings.push(finding(
                    FindingCode::AliasNotUnique,
                    format!("alias {alias} is also on {}", others.join(", ")),
                    note.path.clone(),
                ));
            }
        }
        findings
    }

    fn link_findings(&self, pack: &Pack, scope: &str, scope_is_file: bool) -> Vec<Finding> {
        let mut findings = Vec::new();
        for note in &self.notes {
            if !in_scope(&note.path, scope, scope_is_file) {
                continue;
            }
            for (embed, link) in &note.wikilinks {
                let kind = if *embed { "embed" } else { "wikilink" };
                self.push_link(&mut findings, &note.path, kind, link);
            }
            for target in &note.supersedes {
                self.push_link(&mut findings, &note.path, "supersedes", target);
            }
            for (field, target) in &note.typed {
                if typed_field(pack, &note.type_name, field) {
                    self.push_link(&mut findings, &note.path, field, target);
                }
            }
        }
        findings
    }

    fn push_link(&self, findings: &mut Vec<Finding>, path: &str, kind: &str, link: &str) {
        match self.resolve(link) {
            Ok(_) => {}
            Err(ResolveError::Missing | ResolveError::Outside) => findings.push(finding(
                FindingCode::BrokenLink,
                format!("{kind} {link} does not resolve"),
                path.to_string(),
            )),
            Err(ResolveError::Ambiguous { candidates }) => {
                findings.push(ambiguous_finding(path, kind, link, candidates))
            }
        }
    }

    fn supersession(&self, scope: &str, scope_is_file: bool) -> Vec<Finding> {
        let unique = self.unique_ids();
        let mut incoming: BTreeMap<String, usize> = BTreeMap::new();
        let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut path_of: BTreeMap<String, String> = BTreeMap::new();
        for note in &self.notes {
            if !unique.contains(&note.id) {
                continue;
            }
            path_of.insert(note.id.clone(), note.path.clone());
            for target in &note.supersedes {
                if let Ok(Resolved::Note { id, .. }) = self.resolve(target) {
                    if unique.contains(&id) {
                        if id != note.id {
                            *incoming.entry(id.clone()).or_default() += 1;
                        }
                        edges.entry(note.id.clone()).or_default().push(id);
                    }
                }
            }
        }
        let mut findings = Vec::new();
        for note in &self.notes {
            if note.status != "superseded" || !unique.contains(&note.id) {
                continue;
            }
            if !in_scope(&note.path, scope, scope_is_file) {
                continue;
            }
            if incoming.get(&note.id).copied().unwrap_or(0) == 0 {
                findings.push(finding(
                    FindingCode::SupersededWithoutIncoming,
                    "status is superseded and no note supersedes this one",
                    note.path.clone(),
                ));
            }
        }
        let mut seen = BTreeSet::new();
        for cycle in find_cycles(&edges) {
            let rotated = rotate_cycle(&cycle, &path_of);
            if rotated.is_empty() || !seen.insert(rotated.clone()) {
                continue;
            }
            let Some(path) = rotated
                .iter()
                .find(|path| in_scope(path, scope, scope_is_file))
            else {
                continue;
            };
            let mut chain = rotated.clone();
            chain.push(rotated[0].clone());
            findings.push(finding(
                FindingCode::SupersedesCycle,
                format!("supersedes cycle {}", chain.join(" -> ")),
                path.clone(),
            ));
        }
        findings
    }

    fn hash_drift(&self, scope: &str, scope_is_file: bool) -> Vec<Finding> {
        let mut findings = Vec::new();
        for note in &self.notes {
            if note.type_name != "source" || !in_scope(&note.path, scope, scope_is_file) {
                continue;
            }
            let (Some(raw), Some(expected)) = (&note.source_path, &note.sha256) else {
                continue;
            };
            if expected.is_empty() {
                continue;
            }
            let actual = self.read_hash(raw);
            let drifted = !actual.is_some_and(|hash| hash.eq_ignore_ascii_case(expected));
            if drifted {
                findings.push(finding(
                    FindingCode::Sha256Drift,
                    format!("sha256 does not match {raw}"),
                    note.path.clone(),
                ));
            }
        }
        findings
    }

    fn read_hash(&self, raw: &str) -> Option<String> {
        let rel = safe_rel(raw)?;
        let joined = self.root.join(rel);
        let (_root, path) = crate::vault::canonicalize_within(&self.root, &joined).ok()?;
        if !fs::symlink_metadata(&path).ok()?.file_type().is_file() {
            return None;
        }
        let bytes = fs::read(path).ok()?;
        Some(hex_sha256(&bytes))
    }

    /// Edges for `id`, using [`VaultIndex::resolve`] for each target.
    ///
    /// Resolved edges from this id are `outgoing`. Resolved edges whose
    /// target is this id are `backlinks`. Edges from this id that are missing
    /// or ambiguous are `broken`. A resolved file has no note id, so `target`
    /// is `None` and the edge stays in `outgoing`.
    pub fn link_sets(&self, id: &str, pack: &Pack) -> LinkSets {
        let mut edges = Vec::new();
        for note in &self.notes {
            for (embed, raw) in &note.wikilinks {
                let kind = if *embed { "embed" } else { "wikilink" };
                edges.push(self.edge(note, kind, raw));
            }
            for raw in &note.supersedes {
                edges.push(self.edge(note, "frontmatter:supersedes", raw));
            }
            for (field, raw) in &note.typed {
                if field == "supersedes" {
                    continue;
                }
                if typed_field(pack, &note.type_name, field) {
                    edges.push(self.edge(note, &format!("frontmatter:{field}"), raw));
                }
            }
        }
        let mut outgoing = Vec::new();
        let mut backlinks = Vec::new();
        let mut broken = Vec::new();
        for (edge, resolved) in edges {
            if edge.source == id {
                if resolved {
                    outgoing.push(edge.clone());
                } else {
                    broken.push(edge.clone());
                }
            }
            if resolved && edge.target.as_deref() == Some(id) {
                backlinks.push(edge);
            }
        }
        sort_edges(&mut outgoing);
        sort_edges(&mut backlinks);
        sort_edges(&mut broken);
        LinkSets {
            outgoing,
            backlinks,
            broken,
        }
    }

    /// Note id stored at a vault-relative path, when the index has that path.
    pub fn id_at(&self, path: &str) -> Option<&str> {
        self.notes
            .iter()
            .find(|note| note.path == path)
            .map(|note| note.id.as_str())
    }

    fn edge(&self, note: &NoteRec, kind: &str, raw: &str) -> (LinkEdge, bool) {
        let (target, resolved) = match self.resolve(raw) {
            Ok(Resolved::Note { id, .. }) => (Some(id), true),
            Ok(Resolved::File { .. }) => (None, true),
            Err(ResolveError::Missing | ResolveError::Ambiguous { .. } | ResolveError::Outside) => {
                (None, false)
            }
        };
        (
            LinkEdge {
                kind: kind.to_string(),
                raw: raw.to_string(),
                source: note.id.clone(),
                target,
            },
            resolved,
        )
    }

    fn unique_ids(&self) -> BTreeSet<String> {
        self.by_id
            .iter()
            .filter(|(_, idxs)| idxs.len() == 1)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

fn read_notes(root: &Path, files: &[PathBuf]) -> Result<Vec<NoteRec>, CheckFatal> {
    let workers = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, 8);
    if files.len() < 64 || workers == 1 {
        return files.iter().try_fold(Vec::new(), |mut notes, file| {
            if let Some(note) = read_note(root, file)? {
                notes.push(note);
            }
            Ok(notes)
        });
    }
    let chunk = files.len().div_ceil(workers);
    thread::scope(|scope| {
        let mut joins = Vec::new();
        for group in files.chunks(chunk) {
            joins.push(scope.spawn(move || {
                let mut notes = Vec::with_capacity(group.len());
                for file in group {
                    if let Some(note) = read_note(root, file)? {
                        notes.push(note);
                    }
                }
                Ok(notes)
            }));
        }
        let mut notes = Vec::with_capacity(files.len());
        for join in joins {
            let group = join
                .join()
                .map_err(|_| CheckFatal::Io("index worker panicked".to_string()))??;
            notes.extend(group);
        }
        Ok(notes)
    })
}

fn read_note(root: &Path, file: &Path) -> Result<Option<NoteRec>, CheckFatal> {
    let text = fs::read(file).map_err(|err| CheckFatal::Io(err.to_string()))?;
    let Ok(source) = String::from_utf8(text) else {
        return Ok(None);
    };
    let Ok(parsed) = frontmatter::parse_note(&source) else {
        return Ok(None);
    };
    Ok(note_rec(root, file, &parsed))
}

fn note_rec(root: &Path, file: &Path, parsed: &ParsedNote) -> Option<NoteRec> {
    let id = parsed.values.get("id").and_then(Value::as_str)?;
    if id.is_empty() {
        return None;
    }
    let type_name = parsed
        .values
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let status = parsed
        .values
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let source_path = if type_name == "source" {
        parsed
            .values
            .get("path")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    } else {
        None
    };
    let sha256 = parsed
        .values
        .get("sha256")
        .and_then(Value::as_str)
        .map(str::to_string);
    let title = parsed
        .values
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let updated = parsed
        .values
        .get("updated")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(NoteRec {
        id: id.to_string(),
        path: rel_path(root, file),
        aliases: string_list(parsed.values.get("aliases")),
        status,
        type_name,
        title,
        tags: string_list(parsed.values.get("tags")),
        updated,
        body: parsed.body.clone(),
        supersedes: string_list(parsed.values.get("supersedes")),
        wikilinks: wikilink_targets(&parsed.body),
        typed: typed_pairs(parsed),
        source_path,
        sha256,
    })
}

fn typed_pairs(parsed: &ParsedNote) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for (key, value) in &parsed.values {
        if key == "supersedes" || key == "aliases" || key == "id" {
            continue;
        }
        for item in string_list(Some(value)) {
            pairs.push((key.clone(), item));
        }
    }
    pairs
}

fn typed_field(pack: &Pack, type_name: &str, field: &str) -> bool {
    field == "supersedes"
        || pack.get(type_name).is_some_and(|def| {
            def.fields
                .iter()
                .any(|item| item.name == field && item.target.is_some())
        })
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(text)) if !text.is_empty() => vec![text.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn candidate_details(findings: &[Finding]) -> BTreeMap<String, Value> {
    let mut rows = Vec::new();
    for item in findings {
        if item.code != FindingCode::Ambiguous {
            continue;
        }
        let (Some(link), Some(matches)) = (&item.link, &item.matches) else {
            continue;
        };
        rows.push(json!({
            "path": item.path,
            "link": link,
            "matches": matches,
        }));
    }
    let mut details = BTreeMap::new();
    if !rows.is_empty() {
        details.insert("candidates".to_string(), Value::Array(rows));
    }
    details
}

fn in_scope(path: &str, scope: &str, scope_is_file: bool) -> bool {
    if scope.is_empty() {
        return true;
    }
    if scope_is_file {
        path == scope
    } else {
        path == scope || path.starts_with(&format!("{scope}/"))
    }
}

fn link_target(link: &str) -> String {
    let trimmed = link.trim();
    let without_label = trimmed
        .split_once('|')
        .map(|(target, _)| target)
        .unwrap_or(trimmed);
    let without_heading = without_label
        .split_once('#')
        .map(|(target, _)| target)
        .unwrap_or(without_label);
    without_heading.trim().to_string()
}

fn confine_user_path(root: &Path, query: &str) -> Result<String, ResolveError> {
    let trimmed = query.trim();
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return match crate::vault::canonicalize_within(root, path) {
            Ok((canon_root, canon)) => Ok(rel_of(&canon_root, &canon)),
            Err(crate::vault::ConfineError::Outside(_)) => Err(ResolveError::Outside),
            Err(crate::vault::ConfineError::Io(_)) => {
                let canon_root = fs::canonicalize(root).map_err(|_| ResolveError::Outside)?;
                if path.starts_with(&canon_root) {
                    Ok(rel_of(&canon_root, path))
                } else {
                    Err(ResolveError::Outside)
                }
            }
        };
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(ResolveError::Outside);
    }
    let joined = root.join(trimmed);
    match crate::vault::canonicalize_within(root, &joined) {
        Err(crate::vault::ConfineError::Outside(_)) => Err(ResolveError::Outside),
        _ => Ok(trimmed.to_string()),
    }
}

fn rel_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn safe_rel(target: &str) -> Option<String> {
    let text = nfc(target.trim().trim_start_matches("./")).replace('\\', "/");
    if text.is_empty() || text.starts_with('/') {
        return None;
    }
    let path = Path::new(&text);
    if path.is_absolute() {
        return None;
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(text)
}

fn nfc(value: &str) -> String {
    value.nfc().collect()
}

fn fold_key(value: &str) -> String {
    nfc(value).to_lowercase()
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sort_edges(edges: &mut [LinkEdge]) {
    edges.sort_by(|left, right| {
        (&left.kind, &left.raw, &left.source, &left.target).cmp(&(
            &right.kind,
            &right.raw,
            &right.source,
            &right.target,
        ))
    });
}

fn finding(code: FindingCode, message: impl Into<String>, path: String) -> Finding {
    Finding {
        code,
        message: message.into(),
        path,
        link: None,
        matches: None,
    }
}

fn ambiguous_finding(path: &str, kind: &str, link: &str, matches: Vec<String>) -> Finding {
    Finding {
        code: FindingCode::Ambiguous,
        message: format!("{kind} {link} matches {}", matches.join(", ")),
        path: path.to_string(),
        link: Some(link.to_string()),
        matches: Some(matches),
    }
}

pub(crate) struct WikilinkSpan {
    pub embed: bool,
    pub inner: String,
    pub inner_start: usize,
    pub inner_end: usize,
}

pub(crate) fn wikilink_spans(body: &str) -> Vec<WikilinkSpan> {
    let mut out = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let stripped_len = line.trim_end_matches(['\n', '\r']).len();
        let stripped = &line[..stripped_len];
        if let Some((marker, len)) = fence {
            if closing_fence(stripped, marker, len) {
                fence = None;
            }
            offset += line.len();
            continue;
        }
        if let Some(opened) = opening_fence(stripped) {
            fence = Some(opened);
            offset += line.len();
            continue;
        }
        scan_line_spans(stripped, offset, &mut out);
        offset += line.len();
    }
    out
}

fn wikilink_targets(body: &str) -> Vec<(bool, String)> {
    wikilink_spans(body)
        .into_iter()
        .map(|link| (link.embed, link.inner))
        .collect()
}

fn scan_line_spans(line: &str, line_offset: usize, out: &mut Vec<WikilinkSpan>) {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '`' {
            let start = index;
            while index < chars.len() && chars[index] == '`' {
                index += 1;
            }
            let width = index - start;
            if let Some(end) = find_run(&chars, index, width) {
                index = end;
                continue;
            }
            index = start + 1;
            continue;
        }
        let embed = chars[index] == '!'
            && index + 2 < chars.len()
            && chars[index + 1] == '['
            && chars[index + 2] == '['
            && !bracket_escaped(&chars, index + 1);
        let plain = chars[index] == '['
            && index + 1 < chars.len()
            && chars[index + 1] == '['
            && !bracket_escaped(&chars, index);
        if embed || plain {
            let content = if embed { index + 3 } else { index + 2 };
            if let Some(end) = find_close(&chars, content) {
                let close = end + 2;
                out.push(WikilinkSpan {
                    embed,
                    inner: chars[content..end].iter().collect(),
                    inner_start: line_offset + byte_at(line, content),
                    inner_end: line_offset + byte_at(line, end),
                });
                index = close;
                continue;
            }
        }
        index += 1;
    }
}

fn byte_at(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn opening_fence(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start();
    let bytes = trimmed.as_bytes();
    if bytes.len() < 3 || (bytes[0] != b'`' && bytes[0] != b'~') {
        return None;
    }
    let marker = bytes[0];
    let len = bytes.iter().take_while(|byte| **byte == marker).count();
    if len < 3 {
        return None;
    }
    let info = trimmed[len..].trim();
    if marker == b'`' && info.contains('`') {
        return None;
    }
    Some((marker, len))
}

fn closing_fence(line: &str, marker: u8, len: usize) -> bool {
    let trimmed = line.trim();
    let bytes = trimmed.as_bytes();
    bytes.len() >= len && bytes.iter().all(|byte| *byte == marker)
}

fn bracket_escaped(chars: &[char], index: usize) -> bool {
    let mut count = 0;
    let mut cursor = index;
    while cursor > 0 && chars[cursor - 1] == '\\' {
        count += 1;
        cursor -= 1;
    }
    count % 2 == 1
}

fn find_run(chars: &[char], from: usize, width: usize) -> Option<usize> {
    let mut index = from;
    while index < chars.len() {
        if chars[index] == '`' {
            let start = index;
            while index < chars.len() && chars[index] == '`' {
                index += 1;
            }
            if index - start == width {
                return Some(index);
            }
            continue;
        }
        index += 1;
    }
    None
}

fn find_close(chars: &[char], from: usize) -> Option<usize> {
    let mut index = from;
    while index + 1 < chars.len() {
        if chars[index] == ']' && chars[index + 1] == ']' {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn find_cycles(edges: &BTreeMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut cycles = Vec::new();
    let mut color: BTreeMap<String, u8> = BTreeMap::new();
    let mut stack = Vec::new();
    let ids: Vec<String> = edges.keys().cloned().collect();
    for id in ids {
        if color.get(&id).copied().unwrap_or(0) == 0 {
            walk(&id, edges, &mut color, &mut stack, &mut cycles);
        }
    }
    cycles
}

fn walk(
    id: &str,
    edges: &BTreeMap<String, Vec<String>>,
    color: &mut BTreeMap<String, u8>,
    stack: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    color.insert(id.to_string(), 1);
    stack.push(id.to_string());
    if let Some(nexts) = edges.get(id) {
        for next in nexts {
            match color.get(next).copied().unwrap_or(0) {
                1 => {
                    if let Some(at) = stack.iter().position(|node| node == next) {
                        let mut cycle = stack[at..].to_vec();
                        cycle.push(next.clone());
                        cycles.push(cycle);
                    }
                }
                0 => walk(next, edges, color, stack, cycles),
                _ => {}
            }
        }
    }
    stack.pop();
    color.insert(id.to_string(), 2);
}

fn rotate_cycle(cycle: &[String], path_of: &BTreeMap<String, String>) -> Vec<String> {
    let mut nodes: Vec<&str> = cycle.iter().map(String::as_str).collect();
    if nodes.first() == nodes.last() && nodes.len() > 1 {
        nodes.pop();
    }
    let paths: Vec<String> = nodes
        .iter()
        .filter_map(|id| path_of.get(*id).cloned())
        .collect();
    if paths.len() != nodes.len() || paths.is_empty() {
        return Vec::new();
    }
    let min = paths
        .iter()
        .enumerate()
        .min_by_key(|(_, path)| *path)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let mut rotated = paths[min..].to_vec();
    rotated.extend(paths[..min].iter().cloned());
    rotated
}
