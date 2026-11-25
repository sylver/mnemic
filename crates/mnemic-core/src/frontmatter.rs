use std::collections::BTreeMap;

use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, ScanError, YamlData};
use serde_json::{Map, Number, Value};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySpan {
    pub name: String,
    pub span: ByteSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedNote {
    pub block: ByteSpan,
    pub keys: Vec<KeySpan>,
    pub values: BTreeMap<String, Value>,
    pub body: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrontmatterError {
    #[error("frontmatter is missing")]
    Missing,
    #[error("frontmatter has an unterminated --- fence")]
    Unterminated,
    #[error("frontmatter is not a mapping")]
    NotAMapping,
    #[error("frontmatter has a duplicate YAML key")]
    DuplicateKey,
    #[error("frontmatter is not valid YAML: {0}")]
    Yaml(String),
}

pub fn parse_note(source: &str) -> Result<ParsedNote, FrontmatterError> {
    let (inner, block) = split_block(source)?;
    let docs = MarkedYaml::load_from_str(inner).map_err(map_yaml_error)?;
    let doc = match docs.as_slice() {
        [] => return Err(FrontmatterError::NotAMapping),
        [doc] => doc,
        _ => return Err(FrontmatterError::Yaml("multiple documents".to_string())),
    };
    let YamlData::Mapping(mapping) = &doc.data else {
        return Err(FrontmatterError::NotAMapping);
    };

    let inner_start = block.start + opening_len(source);
    let mut keys = Vec::new();
    collect_keys(doc, inner, inner_start, &mut keys);

    let mut values = BTreeMap::new();
    for (key, value) in mapping {
        let name = key_name(key)?;
        values.insert(name, to_json(value)?);
    }

    let body = source[block.end..].to_string();
    Ok(ParsedNote {
        block,
        keys,
        values,
        body,
    })
}

fn map_yaml_error(err: ScanError) -> FrontmatterError {
    let info = err.info();
    if info.contains("duplicated key") {
        FrontmatterError::DuplicateKey
    } else {
        FrontmatterError::Yaml(info.to_string())
    }
}

fn opening_len(source: &str) -> usize {
    if source.starts_with("---\r\n") {
        5
    } else {
        4
    }
}

fn split_block(source: &str) -> Result<(&str, ByteSpan), FrontmatterError> {
    let Some(rest) = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))
    else {
        return Err(FrontmatterError::Missing);
    };
    let line_end = if source.starts_with("---\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let offset = source.len() - rest.len();
    let bytes = rest.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let next = memchr_nl(bytes, cursor);
        let end = next.unwrap_or(bytes.len());
        let line = &rest[cursor..end];
        if line == "---" {
            let close_end = if next.is_some() {
                end + line_end.len()
            } else {
                end
            };
            let block_end = offset + close_end;
            return Ok((
                &rest[..cursor],
                ByteSpan {
                    start: 0,
                    end: block_end,
                },
            ));
        }
        match next {
            Some(at) => cursor = at + line_end.len(),
            None => break,
        }
    }
    let _ = offset;
    Err(FrontmatterError::Unterminated)
}

fn memchr_nl(bytes: &[u8], from: usize) -> Option<usize> {
    bytes[from..]
        .iter()
        .position(|byte| *byte == b'\n' || *byte == b'\r')
        .map(|pos| from + pos)
}

fn collect_keys(node: &MarkedYaml<'_>, inner: &str, inner_start: usize, keys: &mut Vec<KeySpan>) {
    let YamlData::Mapping(mapping) = &node.data else {
        return;
    };
    for (key, value) in mapping {
        if let Ok(name) = key_name(key) {
            keys.push(KeySpan {
                name,
                span: shift_span(
                    inner,
                    inner_start,
                    key.span.start.index(),
                    key.span.end.index(),
                ),
            });
        }
        collect_keys(value, inner, inner_start, keys);
    }
}

fn shift_span(inner: &str, inner_start: usize, char_start: usize, char_end: usize) -> ByteSpan {
    ByteSpan {
        start: inner_start + char_index_to_byte(inner, char_start),
        end: inner_start + char_index_to_byte(inner, char_end),
    }
}

fn char_index_to_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map(|(byte, _)| byte)
        .unwrap_or(text.len())
}

fn key_name(node: &MarkedYaml<'_>) -> Result<String, FrontmatterError> {
    match &node.data {
        YamlData::Value(Scalar::String(text)) => Ok(text.to_string()),
        _ => Err(FrontmatterError::Yaml(
            "mapping key is not a string".to_string(),
        )),
    }
}

fn to_json(node: &MarkedYaml<'_>) -> Result<Value, FrontmatterError> {
    match &node.data {
        YamlData::Value(scalar) => Ok(scalar_json(scalar)),
        YamlData::Sequence(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(to_json(item)?);
            }
            Ok(Value::Array(out))
        }
        YamlData::Mapping(mapping) => {
            let mut out = Map::new();
            for (key, value) in mapping {
                out.insert(key_name(key)?, to_json(value)?);
            }
            Ok(Value::Object(out))
        }
        YamlData::Tagged(_, inner) => to_json(inner),
        YamlData::Alias(_) | YamlData::BadValue | YamlData::Representation(_, _, _) => {
            Err(FrontmatterError::Yaml("unsupported YAML node".to_string()))
        }
    }
}

fn scalar_json(scalar: &Scalar<'_>) -> Value {
    match scalar {
        Scalar::Null => Value::Null,
        Scalar::Boolean(value) => Value::Bool(*value),
        Scalar::Integer(value) => Value::Number((*value).into()),
        Scalar::FloatingPoint(value) => Number::from_f64((*value).into())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Scalar::String(text) => Value::String(text.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_block_and_key_spans() {
        let source = "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FAV\ntitle: Hi\n---\n\nBody\n";
        let note = parse_note(source).expect("parse");
        assert_eq!(
            &source[..note.block.end],
            "---\nid: 01ARZ3NDEKTSV4RRFFQ69G5FAV\ntitle: Hi\n---\n"
        );
        let id = note.keys.iter().find(|key| key.name == "id").expect("id");
        assert!(source[id.span.start..id.span.end].starts_with("id"));
        assert_eq!(note.body, "\nBody\n");
    }

    #[test]
    fn rejects_each_frontmatter_class() {
        assert_eq!(parse_note("# hi\n").unwrap_err(), FrontmatterError::Missing);
        assert_eq!(
            parse_note("---\n- a\n---\n").unwrap_err(),
            FrontmatterError::NotAMapping
        );
        assert_eq!(
            parse_note("---\nid: a\nid: b\n---\n").unwrap_err(),
            FrontmatterError::DuplicateKey
        );
        assert_eq!(
            parse_note("---\nid: a\n").unwrap_err(),
            FrontmatterError::Unterminated
        );
    }
}
