use crate::frontmatter::{self, FrontmatterError};

/// Pure frontmatter edits. Bytes in, bytes out. This type does not write files.
/// Callers pass the result to `WriteTxn::atomic_write` inside `with_write_lock`.
///
/// Insert and replace change one line. `create_block` prepends a new fence.
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
    #[error("value must be a single line")]
    MultilineValue,
}

impl FrontmatterEdit {
    pub fn insert(source: &str, key: &str, yaml: &str) -> Result<String, EditError> {
        one_line(yaml)?;
        let note = frontmatter::parse_note(source)?;
        if note.keys.iter().any(|span| span.name == key) {
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
        one_line(yaml)?;
        let note = frontmatter::parse_note(source)?;
        let span = note
            .keys
            .iter()
            .find(|span| span.name == key)
            .ok_or_else(|| EditError::KeyMissing(key.to_string()))?;
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
        for (_, yaml) in fields {
            one_line(yaml)?;
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
}

fn one_line(yaml: &str) -> Result<(), EditError> {
    if yaml.contains(['\n', '\r']) {
        Err(EditError::MultilineValue)
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

fn value_region(source: &str, key_end: usize) -> (usize, usize) {
    let tail = &source[key_end..];
    let line_len = tail.find(['\n', '\r']).unwrap_or(tail.len());
    let line = &tail[..line_len];
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
    let start = key_end + index;
    let mut end = key_end + line_len;
    if let Some(comment) = source[start..end].find(" #") {
        end = start + comment;
        while end > start && source.as_bytes()[end - 1] == b' ' {
            end -= 1;
        }
    }
    (start, end)
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
}
