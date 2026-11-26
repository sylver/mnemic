//! Append one timeline line under `## Timeline`.
//!
//! The line is inserted in date order, newest first. The same date, source,
//! and summary is a no-op. Resolve, read, and write share one write lock.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, NaiveDate};
use serde::Serialize;

use crate::clock::Clock;
use crate::edit::FrontmatterEdit;
use crate::frontmatter::{self, FrontmatterError};
use crate::index::{ResolveError, Resolved, VaultIndex};
use crate::pack::{self, PackError};
use crate::vault::{self, ConfineError, VaultError};
use crate::write::{self, WriteTxn};
use crate::Error;

/// Inputs for [`log_note`]. `note` is a path, id, slug, or alias.
pub struct LogRequest<'a> {
    pub vault: &'a Path,
    pub note: &'a str,
    pub text: &'a str,
    pub clock: &'a dyn Clock,
    pub lock_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogResult {
    pub id: String,
    pub path: String,
    pub noop: bool,
    pub line: String,
    pub heading_created: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogSuccess {
    pub ok: bool,
    pub data: LogData,
    pub error: Option<()>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogData {
    pub id: String,
    pub path: String,
    pub noop: bool,
    pub line: String,
    pub heading_created: bool,
}

impl LogResult {
    pub fn success(&self) -> LogSuccess {
        LogSuccess {
            ok: true,
            data: LogData {
                id: self.id.clone(),
                path: self.path.clone(),
                noop: self.noop,
                line: self.line.clone(),
                heading_created: self.heading_created,
            },
            error: None,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LogError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    NotFound(String),
    #[error("note matches more than one path: {}", candidates.join(", "))]
    Ambiguous { candidates: Vec<String> },
    #[error("{0}")]
    TimelineDisabled(String),
    #[error("{0}")]
    TimelineHeadingAmbiguous(String),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimelineLine {
    date: String,
    source: Option<String>,
    summary: String,
}

impl TimelineLine {
    fn render(&self) -> String {
        match &self.source {
            Some(source) => format!("- {} | {} | {}", self.date, source, self.summary),
            None => format!("- {} | {}", self.date, self.summary),
        }
    }

    fn day(&self) -> &str {
        &self.date[..10.min(self.date.len())]
    }
}

struct Applied {
    text: String,
    heading_created: bool,
    noop: bool,
}

fn since_secs(since: std::time::SystemTime) -> u64 {
    since
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Resolve `request.note` under the vault write lock and append one timeline line.
pub fn log_note(request: LogRequest<'_>) -> Result<LogResult, LogError> {
    let line = parse_timeline_text(request.text)?;
    let root = vault::open_vault(request.vault, Some(request.vault))
        .map_err(vault_error)?
        .root;
    match write::with_write_lock(&root, request.lock_timeout, |txn| {
        Ok(log_locked(&root, request.note, &line, request.clock, txn))
    }) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(lock_error(err)),
    }
}

fn log_locked(
    root: &Path,
    note: &str,
    line: &TimelineLine,
    clock: &dyn Clock,
    txn: &WriteTxn,
) -> Result<LogResult, LogError> {
    let index = VaultIndex::load(root).map_err(|err| match err {
        crate::check::CheckFatal::Usage(message) | crate::check::CheckFatal::Io(message) => {
            LogError::Io(message)
        }
    })?;
    let resolved = match index.resolve(note) {
        Ok(resolved) => resolved,
        Err(ResolveError::Missing) => {
            return Err(LogError::NotFound(format!("no note matches {note}")));
        }
        Err(ResolveError::Ambiguous { candidates }) => {
            return Err(LogError::Ambiguous { candidates });
        }
    };
    let Resolved::Note { id, path } = resolved else {
        return Err(LogError::NotFound(format!("no note matches {note}")));
    };
    if path.starts_with("sources/raw/") {
        return Err(LogError::Usage("sources/raw is not rewritten".to_string()));
    }
    let joined = root.join(&path);
    let (_vault, full) = match vault::canonicalize_within(root, &joined) {
        Ok(confined) => confined,
        Err(ConfineError::Outside(message)) => return Err(LogError::OutsideVault(message)),
        Err(ConfineError::Io(message)) => return Err(LogError::Io(message)),
    };
    let bytes = std::fs::read(&full).map_err(|err| LogError::Io(err.to_string()))?;
    let source =
        String::from_utf8(bytes).map_err(|_| LogError::Io(format!("{path} is not UTF-8")))?;
    let type_name = note_type(&source)?;
    let pack = pack::resolve_pack(root).map_err(pack_error)?;
    match pack.get(&type_name) {
        Some(def) if def.timeline => {}
        Some(_) => {
            return Err(LogError::TimelineDisabled(format!(
                "{type_name} has timeline = false"
            )));
        }
        None => {
            return Err(LogError::Usage(format!("unknown type {type_name}")));
        }
    }
    let rendered = line.render();
    let applied = apply_timeline(&source, &rendered, line)?;
    if applied.noop {
        return Ok(LogResult {
            id,
            path,
            noop: true,
            line: rendered,
            heading_created: false,
        });
    }
    let stamped = FrontmatterEdit::replace(&applied.text, "updated", &stamp(clock))
        .map_err(|err| LogError::Io(format!("updated: {err}")))?;
    txn.atomic_write(&full, stamped.as_bytes())
        .map_err(|err| LogError::Io(err.to_string()))?;
    Ok(LogResult {
        id,
        path,
        noop: false,
        line: rendered,
        heading_created: applied.heading_created,
    })
}

fn note_type(source: &str) -> Result<String, LogError> {
    let parsed = frontmatter::parse_note(source).map_err(|err| match err {
        FrontmatterError::Missing => LogError::Usage("note has no frontmatter".to_string()),
        other => LogError::Io(other.to_string()),
    })?;
    parsed
        .values
        .get("type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| LogError::Usage("note has no type".to_string()))
}

fn stamp(clock: &dyn Clock) -> String {
    let millis = i64::try_from(clock.unix_millis()).unwrap_or(0);
    let dt = DateTime::from_timestamp_millis(millis).unwrap_or(DateTime::UNIX_EPOCH);
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn vault_error(err: VaultError) -> LogError {
    match err {
        VaultError::NotFound => LogError::Vault(err.to_string()),
        VaultError::BadConfig(message) | VaultError::Io(message) => LogError::Vault(message),
        VaultError::Unsupported { .. } => LogError::Vault(err.to_string()),
    }
}

fn pack_error(err: PackError) -> LogError {
    LogError::Io(err.to_string())
}

fn lock_error(err: Error) -> LogError {
    match err {
        Error::LockHeld { pid, since } => LogError::LockHeld { pid, since },
        Error::Io(err) => LogError::Io(err.to_string()),
        Error::Empty(label) => LogError::Usage(format!("{label} is empty")),
    }
}

fn parse_timeline_text(text: &str) -> Result<TimelineLine, LogError> {
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim();
    let Some((date, after)) = rest.split_once('|') else {
        return Err(LogError::Usage(
            "date must be YYYY-MM-DD or an RFC 3339 datetime with an offset".to_string(),
        ));
    };
    let date = date.trim();
    validate_date(date)?;
    let after = after.trim();
    let (source, summary) = split_source_summary(after)?;
    if summary.is_empty() || summary.contains('|') {
        return Err(LogError::Usage("summary must not contain |".to_string()));
    }
    Ok(TimelineLine {
        date: date.to_string(),
        source,
        summary: summary.to_string(),
    })
}

fn split_source_summary(after: &str) -> Result<(Option<String>, String), LogError> {
    if after.is_empty() {
        return Err(LogError::Usage("timeline line needs a summary".to_string()));
    }
    if let Some(rest) = after.strip_prefix("[[") {
        let Some(end) = rest.find("]]") else {
            return Err(LogError::Usage("timeline line needs a summary".to_string()));
        };
        let source = format!("[[{}]]", &rest[..end]);
        let tail = rest[end + 2..].trim();
        if tail.is_empty() {
            return Ok((None, source));
        }
        let Some(summary) = tail.strip_prefix('|') else {
            return Err(LogError::Usage(
                "timeline line must be a date, an optional source, and a summary".to_string(),
            ));
        };
        return Ok((Some(source), summary.trim().to_string()));
    }
    match after.split_once('|') {
        None => Ok((None, after.to_string())),
        Some((source, summary)) => {
            let source = source.trim();
            if source.is_empty() {
                return Err(LogError::Usage(
                    "timeline line must be a date, an optional source, and a summary".to_string(),
                ));
            }
            Ok((Some(source.to_string()), summary.trim().to_string()))
        }
    }
}

fn validate_date(date: &str) -> Result<(), LogError> {
    let calendar = date.len() == 10 && NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok();
    let dated = date.contains('T') && DateTime::parse_from_rfc3339(date).is_ok();
    if calendar || dated {
        Ok(())
    } else {
        Err(LogError::Usage(
            "date must be YYYY-MM-DD or an RFC 3339 datetime with an offset".to_string(),
        ))
    }
}

fn apply_timeline(source: &str, rendered: &str, line: &TimelineLine) -> Result<Applied, LogError> {
    let (lines, nl, trailing) = split_lines(source);
    if lines.iter().any(|item| item.trim() == rendered) {
        return Ok(Applied {
            text: source.to_string(),
            heading_created: false,
            noop: true,
        });
    }
    let headings = timeline_headings(&lines);
    if headings.len() > 1 {
        return Err(LogError::TimelineHeadingAmbiguous(
            "more than one ## Timeline heading".to_string(),
        ));
    }
    let mut lines = lines;
    let heading_created = if headings.is_empty() {
        if !lines.is_empty() && !lines.last().is_some_and(|item| item.is_empty()) {
            lines.push(String::new());
        }
        lines.push("## Timeline".to_string());
        lines.push(String::new());
        lines.push(rendered.to_string());
        true
    } else {
        insert_line(&mut lines, headings[0], rendered, line)?;
        false
    };
    Ok(Applied {
        text: join_lines(&lines, nl, trailing || heading_created),
        heading_created,
        noop: false,
    })
}

fn insert_line(
    lines: &mut Vec<String>,
    heading: usize,
    rendered: &str,
    line: &TimelineLine,
) -> Result<(), LogError> {
    if heading + 1 >= lines.len() || !lines[heading + 1].trim().is_empty() {
        lines.insert(heading + 1, String::new());
    }
    let section_end = next_section(lines, heading + 1);
    let mut at = section_end;
    let mut idx = heading + 1;
    while idx < section_end {
        let trimmed = lines[idx].trim();
        if trimmed.is_empty() {
            idx += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            if let Ok(existing) = parse_timeline_text(rest) {
                if existing.day() > line.day() {
                    idx += 1;
                    continue;
                }
            }
        }
        at = idx;
        break;
    }
    lines.insert(at, rendered.to_string());
    let _ = section_end;
    Ok(())
}

fn next_section(lines: &[String], from: usize) -> usize {
    let mut fence: Option<char> = None;
    for (offset, line) in lines.iter().enumerate().skip(from) {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            if fence_close(trimmed, marker) {
                fence = None;
            }
            continue;
        }
        if let Some(marker) = fence_open(trimmed) {
            fence = Some(marker);
            continue;
        }
        if is_section_break(trimmed) {
            return offset;
        }
    }
    lines.len()
}

fn timeline_headings(lines: &[String]) -> Vec<usize> {
    let mut fence: Option<char> = None;
    let mut found = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            if fence_close(trimmed, marker) {
                fence = None;
            }
            continue;
        }
        if let Some(marker) = fence_open(trimmed) {
            fence = Some(marker);
            continue;
        }
        if trimmed == "## Timeline" {
            found.push(idx);
        }
    }
    found
}

fn is_section_break(trimmed: &str) -> bool {
    if trimmed.starts_with("###") {
        return false;
    }
    trimmed.starts_with("# ") || trimmed.starts_with("## ")
}

fn fence_open(trimmed: &str) -> Option<char> {
    if trimmed.starts_with("```") {
        Some('`')
    } else if trimmed.starts_with("~~~") {
        Some('~')
    } else {
        None
    }
}

fn fence_close(trimmed: &str, marker: char) -> bool {
    let count = trimmed.chars().take_while(|ch| *ch == marker).count();
    count >= 3 && trimmed[marker.len_utf8() * count..].trim().is_empty()
}

fn split_lines(source: &str) -> (Vec<String>, &'static str, bool) {
    let nl = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let normalized = source.replace("\r\n", "\n");
    let trailing = normalized.ends_with('\n');
    let mut lines: Vec<String> = normalized.split('\n').map(str::to_string).collect();
    if trailing && lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    (lines, nl, trailing)
}

fn join_lines(lines: &[String], nl: &str, trailing: bool) -> String {
    let mut text = lines.join(nl);
    if trailing {
        text.push_str(nl);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_pipe_in_the_summary_and_a_bad_date() {
        assert!(matches!(
            parse_timeline_text("2026-11-23 | hello | there | extra"),
            Err(LogError::Usage(message)) if message.contains('|')
        ));
        assert!(matches!(
            parse_timeline_text("2026-13-40 | hello"),
            Err(LogError::Usage(_))
        ));
        assert!(matches!(
            parse_timeline_text("not a date"),
            Err(LogError::Usage(_))
        ));
    }

    #[test]
    fn writes_the_list_marker_and_keeps_a_labeled_wikilink() {
        let line =
            parse_timeline_text("2026-11-23 | [[01ARZ3NDEKTSV4RRFFQ69G5FAV|Ada]] | met").unwrap();
        assert_eq!(
            line.render(),
            "- 2026-11-23 | [[01ARZ3NDEKTSV4RRFFQ69G5FAV|Ada]] | met"
        );
        let plain = parse_timeline_text("- 2026-11-23 | hello").unwrap();
        assert_eq!(plain.render(), "- 2026-11-23 | hello");
    }
}
