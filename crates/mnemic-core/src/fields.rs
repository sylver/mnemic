//! Shared `--field` and `--add` parser for `set` and `new`.

use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate};
use unicode_normalization::UnicodeNormalization;

use crate::edit::yaml_scalar;
use crate::pack::{FieldKind, TypeDef};

const REFUSED: &[&str] = &[
    "id",
    "type",
    "schema_version",
    "created",
    "created_by",
    "updated",
    "status",
    "aliases",
    "supersedes",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Assignment {
    pub key: String,
    pub value: String,
    pub add: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FieldEdit {
    /// Replace the key with this YAML value. `sequence` is a flow list.
    Replace {
        key: String,
        yaml: String,
        sequence: bool,
    },
    /// Append one list item. The item is the raw scalar, not quoted YAML.
    Add { key: String, item: String },
}

pub(crate) fn parse_assignment(raw: &str, add: bool) -> Result<Assignment, String> {
    let flag = if add { "--add" } else { "--field" };
    let Some((key, value)) = raw.split_once('=') else {
        return Err(format!("{flag} must be key=value"));
    };
    if key.is_empty() {
        return Err(format!("{flag} key is empty"));
    }
    Ok(Assignment {
        key: key.to_string(),
        value: value.to_string(),
        add,
    })
}

pub(crate) fn edits_for(
    type_name: &str,
    def: &TypeDef,
    inputs: &[Assignment],
) -> Result<Vec<FieldEdit>, String> {
    let mut lists: Vec<Acc> = Vec::new();
    let mut mode: Vec<(String, bool)> = Vec::new();
    let mut edits: Vec<FieldEdit> = Vec::new();
    for input in inputs {
        let kind = classify(type_name, def, &input.key)?;
        if input.add && !matches!(kind, Slot::List) {
            return Err(format!(
                "--add applies to strings and tags, not {}",
                input.key
            ));
        }
        if let Some((_, add)) = mode.iter().find(|(key, _)| key == &input.key) {
            if *add != input.add {
                return Err(format!("--field and --add both set {}", input.key));
            }
            if !input.add && !matches!(kind, Slot::List) {
                return Err(format!("{} was set more than once", input.key));
            }
        } else {
            mode.push((input.key.clone(), input.add));
        }
        if input.add {
            let item = scalar_text(&input.key, &input.value)?;
            edits.push(FieldEdit::Add {
                key: input.key.clone(),
                item,
            });
            continue;
        }
        match kind {
            Slot::List => {
                if let Some(existing) = lists.iter_mut().find(|edit| edit.key == input.key) {
                    existing.values.push(input.value.clone());
                } else {
                    lists.push(Acc {
                        key: input.key.clone(),
                        values: vec![input.value.clone()],
                        index: edits.len(),
                    });
                    edits.push(FieldEdit::Replace {
                        key: input.key.clone(),
                        yaml: String::new(),
                        sequence: true,
                    });
                }
            }
            Slot::Scalar => {
                let text = scalar_text(&input.key, &input.value)?;
                edits.push(FieldEdit::Replace {
                    key: input.key.clone(),
                    yaml: yaml_scalar(&text),
                    sequence: false,
                });
            }
            Slot::Date => {
                let text = date_text(&input.key, &input.value)?;
                edits.push(FieldEdit::Replace {
                    key: input.key.clone(),
                    yaml: text,
                    sequence: false,
                });
            }
            Slot::State => {
                let text = scalar_text(&input.key, &input.value)?;
                state_allowed(def, &text)?;
                edits.push(FieldEdit::Replace {
                    key: input.key.clone(),
                    yaml: yaml_scalar(&text),
                    sequence: false,
                });
            }
        }
    }
    for list in lists {
        let yaml = flow_list(&list.key, &list.values)?;
        let FieldEdit::Replace { yaml: slot, .. } = &mut edits[list.index] else {
            continue;
        };
        *slot = yaml;
    }
    Ok(edits)
}

/// Keys this call will write, plus the core keys `new` always emits.
pub(crate) fn missing_required(def: &TypeDef, present: &BTreeSet<&str>) -> Option<String> {
    let missing: Vec<&str> = def
        .fields
        .iter()
        .filter(|field| field.required && !present.contains(field.name.as_str()))
        .map(|field| field.name.as_str())
        .collect();
    if missing.is_empty() {
        None
    } else {
        Some(format!(
            "missing required field {} on {}",
            missing.join(", "),
            def.name
        ))
    }
}

struct Acc {
    key: String,
    values: Vec<String>,
    index: usize,
}

#[derive(Clone, Copy)]
enum Slot {
    Scalar,
    List,
    Date,
    State,
}

pub(crate) fn require_key(type_name: &str, def: &TypeDef, key: &str) -> Result<(), String> {
    classify(type_name, def, key).map(|_| ())
}

fn classify(type_name: &str, def: &TypeDef, key: &str) -> Result<Slot, String> {
    if REFUSED.contains(&key) {
        return Err(format!("{key} is not a field of {type_name}"));
    }
    if key == "title" {
        return Ok(Slot::Scalar);
    }
    if key == "tags" {
        return Ok(Slot::List);
    }
    if key == "state" {
        return Ok(Slot::State);
    }
    match def.fields.iter().find(|field| field.name == key) {
        Some(field) => Ok(match field.kind {
            FieldKind::String => Slot::Scalar,
            FieldKind::Strings => Slot::List,
            FieldKind::Date => Slot::Date,
        }),
        None => Err(format!("{key} is not a field of {type_name}")),
    }
}

fn scalar_text(key: &str, value: &str) -> Result<String, String> {
    if value.contains(['\n', '\r']) {
        return Err(format!("{key} must be a single line"));
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{key} is empty"));
    }
    Ok(trimmed.to_string())
}

fn date_text(key: &str, value: &str) -> Result<String, String> {
    let text = scalar_text(key, value)?;
    let calendar = text.len() == 10 && NaiveDate::parse_from_str(&text, "%Y-%m-%d").is_ok();
    let dated = text.contains('T') && DateTime::parse_from_rfc3339(&text).is_ok();
    if calendar || dated {
        Ok(text)
    } else {
        Err(format!(
            "{key} must be YYYY-MM-DD or an RFC 3339 datetime with an offset"
        ))
    }
}

fn flow_list(key: &str, values: &[String]) -> Result<String, String> {
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    for value in values {
        let text = scalar_text(key, value)?;
        if seen.insert(fold_item(&text)) {
            items.push(yaml_scalar(&text));
        }
    }
    Ok(format!("[{}]", items.join(", ")))
}

fn fold_item(value: &str) -> String {
    value.nfc().collect::<String>().to_lowercase()
}

pub(crate) fn state_allowed(def: &TypeDef, value: &str) -> Result<(), String> {
    if def.states.iter().any(|state| state == value) {
        Ok(())
    } else {
        Err(format!("state must be one of {}", def.states.join(", ")))
    }
}
