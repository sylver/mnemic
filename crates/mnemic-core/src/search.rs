use std::path::Path;

use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::Serialize;

use crate::index::{index_vault, IndexedNote};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchRequest<'a> {
    pub query: &'a str,
    pub type_name: Option<&'a str>,
    pub tag: Option<&'a str>,
    pub since: Option<&'a str>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchEnvelope {
    pub ok: bool,
    pub data: Option<SearchData>,
    pub error: Option<crate::contract::ErrorObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchData {
    pub query: String,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SearchHit {
    pub id: String,
    pub path: String,
    pub title: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub snippet: String,
    pub section: SearchSection,
    pub score: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchSection {
    Truth,
    Timeline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchError {
    Usage(String),
    Io(String),
}

pub fn search_vault(root: &Path, request: SearchRequest<'_>) -> Result<SearchData, SearchError> {
    if request.limit == 0 {
        return Err(SearchError::Usage(
            "limit must be greater than zero".to_string(),
        ));
    }
    let tokens = tokens(request.query);
    if tokens.is_empty() {
        return Err(SearchError::Usage("query is empty".to_string()));
    }
    let since = match request.since {
        Some(text) => Some(parse_since(text)?),
        None => None,
    };
    let index = index_vault(root).map_err(|err| match err {
        crate::check::CheckFatal::Io(message) => SearchError::Io(message),
        crate::check::CheckFatal::Usage(message) => SearchError::Usage(message),
    })?;
    let mut ranked = Vec::new();
    for note in index.notes() {
        if !passes_filters(&note, request.type_name, request.tag, since) {
            continue;
        }
        let folded_title = fold(note.title);
        let title_hit = tokens.iter().any(|token| folded_title.contains(token));
        let folded_aliases = fold(&note.aliases.join("\n"));
        let folded_body = fold(note.body);
        let mut hay = String::with_capacity(
            folded_title.len() + folded_aliases.len() + folded_body.len() + 2,
        );
        hay.push_str(&folded_title);
        hay.push('\n');
        hay.push_str(&folded_aliases);
        hay.push('\n');
        hay.push_str(&folded_body);
        if !tokens.iter().all(|token| hay.contains(token)) {
            continue;
        }
        let hit_count = tokens
            .iter()
            .map(|token| hay.matches(token.as_str()).count())
            .sum::<usize>();
        let (snippet, section) = snippet(&tokens, note.title, note.aliases, note.body);
        ranked.push(Ranked {
            title_hit,
            hit_count,
            updated: note.updated.to_string(),
            id: note.id.to_string(),
            path: note.path.to_string(),
            title: note.title.to_string(),
            type_name: note.type_name.to_string(),
            snippet,
            section,
        });
    }
    ranked.sort_by(|left, right| {
        right
            .title_hit
            .cmp(&left.title_hit)
            .then(right.hit_count.cmp(&left.hit_count))
            .then(updated_key(&right.updated).cmp(&updated_key(&left.updated)))
            .then(left.id.cmp(&right.id))
    });
    ranked.truncate(request.limit);
    let hits = ranked
        .into_iter()
        .enumerate()
        .map(|(index, note)| SearchHit {
            id: note.id,
            path: note.path,
            title: note.title,
            type_name: note.type_name,
            snippet: note.snippet,
            section: note.section,
            score: u32::try_from(index + 1).unwrap_or(u32::MAX),
        })
        .collect();
    Ok(SearchData {
        query: request.query.to_string(),
        hits,
    })
}

struct Ranked {
    title_hit: bool,
    hit_count: usize,
    updated: String,
    id: String,
    path: String,
    title: String,
    type_name: String,
    snippet: String,
    section: SearchSection,
}

fn tokens(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(fold)
        .filter(|token| !token.is_empty())
        .collect()
}

fn fold(text: &str) -> String {
    text.chars().flat_map(|ch| ch.to_lowercase()).collect()
}

fn passes_filters(
    note: &IndexedNote<'_>,
    type_name: Option<&str>,
    tag: Option<&str>,
    since: Option<DateTime<FixedOffset>>,
) -> bool {
    if let Some(type_name) = type_name {
        if note.type_name != type_name {
            return false;
        }
    }
    if let Some(tag) = tag {
        let matched = note
            .tags
            .iter()
            .any(|item| item == tag || item.starts_with(&format!("{tag}/")));
        if !matched {
            return false;
        }
    }
    if let Some(since) = since {
        let Some(updated) = DateTime::parse_from_rfc3339(note.updated).ok() else {
            return false;
        };
        if updated < since {
            return false;
        }
    }
    true
}

fn parse_since(text: &str) -> Result<DateTime<FixedOffset>, SearchError> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Ok(parsed);
    }
    if text.len() == 10 {
        if let Ok(parsed) = DateTime::parse_from_rfc3339(&format!("{text}T00:00:00Z")) {
            return Ok(parsed);
        }
    }
    Err(SearchError::Usage(
        "since is not an ISO date or datetime".to_string(),
    ))
}

fn updated_key(text: &str) -> i64 {
    DateTime::parse_from_rfc3339(text)
        .map(|parsed| parsed.timestamp())
        .unwrap_or(0)
}

fn snippet(
    tokens: &[String],
    title: &str,
    aliases: &[String],
    body: &str,
) -> (String, SearchSection) {
    if field_hits(&fold(title), tokens) {
        return (truncate(title), SearchSection::Truth);
    }
    for alias in aliases {
        if field_hits(&fold(alias), tokens) {
            return (truncate(alias), SearchSection::Truth);
        }
    }
    let mut timeline = false;
    for line in body.lines() {
        if line.trim() == "## Timeline" {
            timeline = true;
        }
        if field_hits(&fold(line), tokens) {
            let section = if timeline {
                SearchSection::Timeline
            } else {
                SearchSection::Truth
            };
            return (truncate(line), section);
        }
    }
    (truncate(title), SearchSection::Truth)
}

fn field_hits(folded: &str, tokens: &[String]) -> bool {
    tokens.iter().any(|token| folded.contains(token))
}

fn truncate(text: &str) -> String {
    text.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};

    fn note(
        dir: &Path,
        rel: &str,
        id: &str,
        type_name: &str,
        title: &str,
        updated: &str,
        body: &str,
    ) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            format!(
                "---\nid: \"{id}\"\ntype: {type_name}\nschema_version: 1\ntitle: \"{title}\"\ncreated: {updated}\nupdated: {updated}\ncreated_by: sylver\nstatus: active\n---\n{body}\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn five_thousand_notes_median() {
        let dir = std::env::temp_dir().join(format!("mnemic-search-budget-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("mnemic.toml"),
            "schema_version = 1\nactor = \"sylver\"\n",
        )
        .unwrap();
        for index in 0..5_000 {
            let id = format!("{index:026}");
            let body = if index == 4999 {
                "unique-budget-needle"
            } else {
                "filler text about notes"
            };
            note(
                &dir,
                &format!("notes/n{index}.md"),
                &id,
                "note",
                &format!("Note {index}"),
                "2026-11-01T00:00:00Z",
                body,
            );
        }
        let request = SearchRequest {
            query: "unique-budget-needle",
            type_name: None,
            tag: None,
            since: None,
            limit: 20,
        };
        let warm = search_vault(&dir, request).expect("warm search");
        assert_eq!(warm.hits.len(), 1);
        let mut samples = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            let found = search_vault(&dir, request).expect("search");
            let elapsed = start.elapsed();
            assert_eq!(found.hits.len(), 1);
            assert_eq!(found.hits[0].path, "notes/n4999.md");
            samples.push(elapsed);
        }
        samples.sort();
        let median = samples[2];
        eprintln!(
            "search budget median_ms={} range_ms={}..{} runs=5",
            median.as_millis(),
            samples[0].as_millis(),
            samples[4].as_millis()
        );
        if cfg!(target_os = "macos") {
            assert!(
                median < Duration::from_secs(1),
                "median {}ms",
                median.as_millis()
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
