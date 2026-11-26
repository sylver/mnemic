use unicode_normalization::UnicodeNormalization;

use crate::frontmatter::{self, FrontmatterError};

/// Pure frontmatter edits. Bytes in, bytes out. This type does not write files.
/// Callers pass the result to `WriteTxn::atomic_write` inside `with_write_lock`.
///
/// Insert and replace change one top-level key whose value is a single line.
/// A block scalar, an indented continuation, or a flow value that spans lines
/// is refused. A `#` inside quotes stays part of the value. A trailing comment
/// outside quotes is copied. `create_block` prepends a new fence.
/// Every other byte is copied.
pub struct FrontmatterEdit;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EditError {
    #[error(transparent)]
    Frontmatter(#[from] FrontmatterError),
    #[error("frontmatter already contains {0}")]
    KeyExists(String),
    #[error("frontmatter has no {0}")]
    KeyMissing(String),
    #[error("frontmatter block already exists")]
    BlockExists,
    #[error("{key} value must be a single line")]
    MultilineValue { key: String },
}

impl FrontmatterEdit {
    pub fn insert(source: &str, key: &str, yaml: &str) -> Result<String, EditError> {
        one_line(key, yaml)?;
        let note = frontmatter::parse_note(source)?;
        if top_level_key(source, &note.keys, key).is_some() {
            return Err(EditError::KeyExists(key.to_string()));
        }
        let nl = newline_of(source, &note);
        let Some(close) = source[..note.block.end].rfind("---") else {
            return Err(FrontmatterError::Unterminated.into());
        };
        let mut out = String::with_capacity(source.len() + key.len() + yaml.len() + 4);
        out.push_str(&source[..close]);
        out.push_str(key);
        out.push_str(": ");
        out.push_str(yaml);
        out.push_str(nl);
        out.push_str(&source[close..]);
        Ok(out)
    }

    pub fn replace(source: &str, key: &str, yaml: &str) -> Result<String, EditError> {
        one_line(key, yaml)?;
        let note = frontmatter::parse_note(source)?;
        let span = top_level_key(source, &note.keys, key)
            .ok_or_else(|| EditError::KeyMissing(key.to_string()))?;
        if value_spans_lines(source, span.span.end) {
            return Err(EditError::MultilineValue {
                key: key.to_string(),
            });
        }
        let (start, end) = value_region(source, span.span.end);
        let mut out = String::with_capacity(source.len() + yaml.len());
        out.push_str(&source[..start]);
        out.push_str(yaml);
        out.push_str(&source[end..]);
        Ok(out)
    }

    /// Prepend `---\n<keys>\n---\n` when the file has no frontmatter block.
    /// The original bytes follow that block unchanged.
    pub fn create_block(source: &str, fields: &[(&str, &str)]) -> Result<String, EditError> {
        for (key, yaml) in fields {
            one_line(key, yaml)?;
        }
        match frontmatter::parse_note(source) {
            Ok(_) => return Err(EditError::BlockExists),
            Err(FrontmatterError::Missing) => {}
            Err(err) => return Err(err.into()),
        }
        let mut out = String::from("---\n");
        for (key, yaml) in fields {
            out.push_str(key);
            out.push_str(": ");
            out.push_str(yaml);
            out.push('\n');
        }
        out.push_str("---\n");
        out.push_str(source);
        Ok(out)
    }

    /// Append one sequence item to a top-level key.
    ///
    /// A missing key becomes a one-line flow sequence. A one-line value is
    /// rewritten as a flow sequence. A block sequence gains one indented item.
    /// The rest of the file is copied. An item already present is left as-is.
    pub fn append_sequence_item(source: &str, key: &str, item: &str) -> Result<String, EditError> {
        let scalar = yaml_scalar(item);
        let note = frontmatter::parse_note(source)?;
        let Some(span) = top_level_key(source, &note.keys, key) else {
            return Self::insert(source, key, &format!("[{scalar}]"));
        };
        if sequence_contains(note.values.get(key), item) {
            return Ok(source.to_string());
        }
        if !value_spans_lines(source, span.span.end) {
            let (start, end) = value_region(source, span.span.end);
            let current = source[start..end].trim();
            let flow = if current.is_empty() {
                format!("[{scalar}]")
            } else {
                match flow_items(current) {
                    Some(items) => {
                        let mut items = items;
                        items.push(scalar);
                        format!("[{}]", items.join(", "))
                    }
                    None => format!("[{current}, {scalar}]"),
                }
            };
            return Self::replace(source, key, &flow);
        }
        let nl = newline_of(source, &note);
        let at = block_sequence_end(source, span.span.end);
        let indent = sequence_indent(source, span.span.end);
        let mut out = String::with_capacity(source.len() + scalar.len() + 8);
        out.push_str(&source[..at]);
        if at > 0 && source.as_bytes()[at - 1] != b'\n' {
            out.push_str(nl);
        }
        out.push_str(&indent);
        out.push_str("- ");
        out.push_str(&scalar);
        out.push_str(nl);
        out.push_str(&source[at..]);
        Ok(out)
    }
}

fn yaml_scalar(item: &str) -> String {
    let plain = !item.is_empty()
        && item
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if plain {
        item.to_string()
    } else {
        format!("\"{}\"", item.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn sequence_contains(value: Option<&serde_json::Value>, item: &str) -> bool {
    let wanted = fold_item(item);
    match value {
        Some(serde_json::Value::String(text)) => fold_item(text) == wanted,
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .any(|value| value.as_str().is_some_and(|text| fold_item(text) == wanted)),
        _ => false,
    }
}

fn fold_item(value: &str) -> String {
    value.nfc().collect::<String>().to_lowercase()
}

fn flow_items(value: &str) -> Option<Vec<String>> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    Some(
        inner
            .split(',')
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect(),
    )
}

fn block_sequence_end(source: &str, key_end: usize) -> usize {
    let (_, line_len) = line_after(source, key_end);
    let mut cursor = key_end + line_len;
    if source.as_bytes().get(cursor) == Some(&b'\r') {
        cursor += 1;
    }
    if source.as_bytes().get(cursor) == Some(&b'\n') {
        cursor += 1;
    }
    let mut end = cursor;
    let rest = &source[cursor..];
    let mut offset = 0;
    for raw in rest.split_inclusive('\n') {
        let bare = raw.trim_end_matches(['\n', '\r']);
        if bare == "---" || (!bare.is_empty() && !bare.starts_with([' ', '\t'])) {
            break;
        }
        offset += raw.len();
        if !bare.trim().is_empty() {
            end = cursor + offset;
        }
    }
    end
}

fn sequence_indent(source: &str, key_end: usize) -> String {
    let (_, line_len) = line_after(source, key_end);
    let mut cursor = key_end + line_len;
    if source.as_bytes().get(cursor) == Some(&b'\r') {
        cursor += 1;
    }
    if source.as_bytes().get(cursor) == Some(&b'\n') {
        cursor += 1;
    }
    for raw in source[cursor..].split_inclusive('\n') {
        let bare = raw.trim_end_matches(['\n', '\r']);
        if bare == "---" || (!bare.is_empty() && !bare.starts_with([' ', '\t'])) {
            break;
        }
        let trimmed = bare.trim_start();
        if trimmed.starts_with("- ") {
            let width = bare.len() - trimmed.len();
            return bare[..width].to_string();
        }
    }
    "  ".to_string()
}

fn one_line(key: &str, yaml: &str) -> Result<(), EditError> {
    if yaml.contains(['\n', '\r']) {
        Err(EditError::MultilineValue {
            key: key.to_string(),
        })
    } else {
        Ok(())
    }
}

fn newline_of(source: &str, note: &frontmatter::ParsedNote) -> &'static str {
    if source[..note.block.end].contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn top_level_key<'a>(
    source: &str,
    keys: &'a [frontmatter::KeySpan],
    key: &str,
) -> Option<&'a frontmatter::KeySpan> {
    keys.iter()
        .find(|span| span.name == key && at_column_zero(source, span.span.start))
}

fn at_column_zero(source: &str, start: usize) -> bool {
    start == 0 || source.as_bytes().get(start.wrapping_sub(1)) == Some(&b'\n')
}

fn value_region(source: &str, key_end: usize) -> (usize, usize) {
    let (line, line_len) = line_after(source, key_end);
    let start = key_end + value_offset(line);
    let mut end = key_end + line_len;
    if let Some(comment) = trailing_comment(&source[start..end]) {
        end = start + comment;
        while end > start && source.as_bytes()[end - 1] == b' ' {
            end -= 1;
        }
    }
    (start, end)
}

fn value_spans_lines(source: &str, key_end: usize) -> bool {
    let (line, line_len) = line_after(source, key_end);
    let value = {
        let offset = value_offset(line);
        let text = &line[offset..];
        match trailing_comment(text) {
            Some(comment) => text[..comment].trim(),
            None => text.trim(),
        }
    };
    if is_block_scalar(value) || flow_unclosed(value) {
        return true;
    }
    let mut rest = key_end + line_len;
    if source.as_bytes().get(rest) == Some(&b'\r') {
        rest += 1;
    }
    if source.as_bytes().get(rest) == Some(&b'\n') {
        rest += 1;
    }
    for raw in source[rest..].split_inclusive('\n') {
        let bare = raw.trim_end_matches(['\n', '\r']);
        if bare == "---" || (!bare.is_empty() && !bare.starts_with([' ', '\t'])) {
            return false;
        }
        let trimmed = bare.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return true;
    }
    false
}

fn line_after(source: &str, key_end: usize) -> (&str, usize) {
    let tail = &source[key_end..];
    let line_len = tail.find(['\n', '\r']).unwrap_or(tail.len());
    (&tail[..line_len], line_len)
}

fn value_offset(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index] == b' ' {
        index += 1;
    }
    if index < bytes.len() && bytes[index] == b':' {
        index += 1;
    }
    while index < bytes.len() && bytes[index] == b' ' {
        index += 1;
    }
    index
}

fn is_block_scalar(value: &str) -> bool {
    matches!(value.as_bytes().first(), Some(b'|' | b'>'))
}

/// Index of the space before a `#` that sits outside quotes. `None` when the
/// line has no trailing comment.
fn trailing_comment(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(kind) = quote {
            if kind == b'"' && byte == b'\\' {
                index += 1 + usize::from(index + 1 < bytes.len());
                continue;
            }
            if byte == kind {
                if kind == b'\'' && bytes.get(index + 1) == Some(&b'\'') {
                    index += 2;
                    continue;
                }
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'#' && index > 0 && bytes[index - 1] == b' ' {
            return Some(index - 1);
        }
        index += 1;
    }
    None
}

fn flow_unclosed(text: &str) -> bool {
    let bytes = text.as_bytes();
    if !matches!(bytes.first(), Some(b'[' | b'{')) {
        return false;
    }
    let mut depth = 0i32;
    let mut index = 0;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(kind) = quote {
            if kind == b'"' && byte == b'\\' {
                index += 1 + usize::from(index + 1 < bytes.len());
                continue;
            }
            if byte == kind {
                if kind == b'\'' && bytes.get(index + 1) == Some(&b'\'') {
                    index += 2;
                    continue;
                }
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' | b'\'' => quote = Some(byte),
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    depth != 0 || quote.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "\
---\n\
# keep\n\
id: \"01ARZ3NDEKTSV4RRFFQ69G5FAV\"\n\
title: \"Hi\"\n\
---\n\
\n\
Body stays\n";

    #[test]
    fn replace_changes_only_the_value_bytes() {
        let edited = FrontmatterEdit::replace(SOURCE, "title", "\"Yo\"").expect("replace");
        assert_eq!(
            edited,
            "\
---\n\
# keep\n\
id: \"01ARZ3NDEKTSV4RRFFQ69G5FAV\"\n\
title: \"Yo\"\n\
---\n\
\n\
Body stays\n"
        );
    }

    #[test]
    fn insert_keeps_every_other_byte() {
        let edited = FrontmatterEdit::insert(SOURCE, "kind", "note").expect("insert");
        assert_eq!(
            edited,
            "\
---\n\
# keep\n\
id: \"01ARZ3NDEKTSV4RRFFQ69G5FAV\"\n\
title: \"Hi\"\n\
kind: note\n\
---\n\
\n\
Body stays\n"
        );
    }

    #[test]
    fn create_block_leaves_the_body_bytes() {
        let body = "Hello\n\n# not a fence\n";
        let edited = FrontmatterEdit::create_block(
            body,
            &[("id", "01ARZ3NDEKTSV4RRFFQ69G5FAV"), ("type", "note")],
        )
        .expect("create");
        assert_eq!(
            edited,
            "\
---\n\
id: 01ARZ3NDEKTSV4RRFFQ69G5FAV\n\
type: note\n\
---\n\
Hello\n\
\n\
# not a fence\n"
        );
        assert!(edited.ends_with(body));
    }

    #[test]
    fn replace_keeps_a_hash_inside_a_quoted_scalar() {
        let source = "---\ntitle: \"hello # world\"\n---\n";
        let edited = FrontmatterEdit::replace(source, "title", "\"Yo\"").expect("replace");
        assert_eq!(edited, "---\ntitle: \"Yo\"\n---\n");
    }

    #[test]
    fn replace_keeps_a_trailing_comment_after_a_closed_quote() {
        let source = "---\ntitle: \"Hi\"  # keep\n---\n";
        let edited = FrontmatterEdit::replace(source, "title", "\"Yo\"").expect("replace");
        assert_eq!(edited, "---\ntitle: \"Yo\"  # keep\n---\n");
    }

    #[test]
    fn replace_refuses_a_multiline_value() {
        let cases = [
            "---\ntitle: |\n  hello\n---\n",
            "---\ntitle: >\n  hello\n---\n",
            "---\ntitle: hello\n  world\n---\n",
            "---\naliases:\n  - a\n---\n",
            "---\nmeta:\n  a: 1\n---\n",
            "---\ntags: [a,\n  b]\n---\n",
        ];
        for source in cases {
            let err = FrontmatterEdit::replace(source, source_key(source), "\"x\"").unwrap_err();
            assert_eq!(
                err,
                EditError::MultilineValue {
                    key: source_key(source).to_string()
                }
            );
        }
        let flow = "---\ntags: [a, b]\n---\n";
        let edited = FrontmatterEdit::replace(flow, "tags", "[c]").expect("flow");
        assert_eq!(edited, "---\ntags: [c]\n---\n");
    }

    #[test]
    fn insert_and_replace_match_only_top_level_keys() {
        let source = "---\nmeta:\n  title: \"nested\"\n---\n";
        let inserted = FrontmatterEdit::insert(source, "title", "\"top\"").expect("insert");
        assert_eq!(
            inserted,
            "---\nmeta:\n  title: \"nested\"\ntitle: \"top\"\n---\n"
        );
        let err = FrontmatterEdit::replace(source, "title", "\"Yo\"").unwrap_err();
        assert_eq!(err, EditError::KeyMissing("title".to_string()));

        let both = "---\ntitle: \"top\"\nmeta:\n  title: \"nested\"\n---\n";
        let edited = FrontmatterEdit::replace(both, "title", "\"Yo\"").expect("replace");
        assert_eq!(
            edited,
            "---\ntitle: \"Yo\"\nmeta:\n  title: \"nested\"\n---\n"
        );
    }

    #[test]
    fn append_sequence_item_extends_a_block_and_a_flow_list() {
        let block = "---\naliases:\n  - Ada\n---\n";
        let edited = FrontmatterEdit::append_sequence_item(block, "aliases", "old-slug").unwrap();
        assert_eq!(edited, "---\naliases:\n  - Ada\n  - old-slug\n---\n");
        let again = FrontmatterEdit::append_sequence_item(&edited, "aliases", "old-slug").unwrap();
        assert_eq!(again, edited);
        let flow = "---\naliases: [Ada]\n---\n";
        let edited = FrontmatterEdit::append_sequence_item(flow, "aliases", "old-slug").unwrap();
        assert_eq!(edited, "---\naliases: [Ada, old-slug]\n---\n");
        let missing = "---\ntitle: Ada\n---\n";
        let edited = FrontmatterEdit::append_sequence_item(missing, "aliases", "old-slug").unwrap();
        assert_eq!(edited, "---\ntitle: Ada\naliases: [old-slug]\n---\n");
    }

    fn source_key(source: &str) -> &str {
        source
            .lines()
            .nth(1)
            .and_then(|line| line.split(':').next())
            .unwrap_or("title")
    }
}
