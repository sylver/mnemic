use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;
use toml::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    types: BTreeMap<String, TypeDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef {
    pub name: String,
    pub schema_version: u64,
    pub folders: Vec<String>,
    pub timeline: bool,
    pub states: Vec<String>,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub name: String,
    pub kind: FieldKind,
    pub required: bool,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    String,
    Strings,
    Date,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PackError {
    #[error("pack {name}: {message}")]
    Invalid { name: String, message: String },
}

impl Pack {
    pub fn get(&self, name: &str) -> Option<&TypeDef> {
        self.types.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.types.keys().map(String::as_str)
    }
}

pub fn core_pack() -> Result<Pack, PackError> {
    load_pack([
        ("decision", include_str!("../pack/core/decision.toml")),
        ("idea", include_str!("../pack/core/idea.toml")),
        ("journal", include_str!("../pack/core/journal.toml")),
        ("note", include_str!("../pack/core/note.toml")),
        ("org", include_str!("../pack/core/org.toml")),
        ("person", include_str!("../pack/core/person.toml")),
        ("place", include_str!("../pack/core/place.toml")),
        ("project", include_str!("../pack/core/project.toml")),
        ("source", include_str!("../pack/core/source.toml")),
        ("topic", include_str!("../pack/core/topic.toml")),
    ])
}

pub fn load_pack<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<Pack, PackError> {
    let mut types = BTreeMap::new();
    for (name, text) in files {
        let def = parse_type(name, text)?;
        if types.insert(def.name.clone(), def).is_some() {
            return Err(invalid(name, "duplicate type"));
        }
    }
    Ok(Pack { types })
}

fn parse_type(name: &str, text: &str) -> Result<TypeDef, PackError> {
    let value: Value = toml::from_str(text).map_err(|err| invalid(name, err.to_string()))?;
    let Some(table) = value.as_table() else {
        return Err(invalid(name, "type file is not a table"));
    };
    require_keys(
        name,
        table,
        &["schema_version", "folder", "timeline", "state", "field"],
    )?;
    let schema_version = required_int(name, table, "schema_version")?;
    if schema_version < 1 {
        return Err(invalid(name, "schema_version must be >= 1"));
    }
    let schema_version =
        u64::try_from(schema_version).map_err(|_| invalid(name, "schema_version must be >= 1"))?;
    let folders = folders(name, table.get("folder"))?;
    let timeline = table
        .get("timeline")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid(name, "timeline must be a boolean"))?;
    let states = states(name, table.get("state"))?;
    let fields = fields(name, table.get("field"))?;
    Ok(TypeDef {
        name: name.to_string(),
        schema_version,
        folders,
        timeline,
        states,
        fields,
    })
}

fn folders(name: &str, value: Option<&Value>) -> Result<Vec<String>, PackError> {
    match value {
        Some(Value::String(folder)) if !folder.is_empty() => Ok(vec![folder.clone()]),
        Some(Value::Array(items)) if !items.is_empty() => {
            let mut folders = Vec::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(folder) if !folder.is_empty() => folders.push(folder.to_string()),
                    _ => return Err(invalid(name, "folder entries must be non-empty strings")),
                }
            }
            Ok(folders)
        }
        _ => Err(invalid(
            name,
            "folder must be a string or a list of strings",
        )),
    }
}

fn states(name: &str, value: Option<&Value>) -> Result<Vec<String>, PackError> {
    let Some(table) = value.and_then(Value::as_table) else {
        return Err(invalid(name, "state must be a table"));
    };
    require_keys(name, table, &["values"])?;
    let Some(items) = table.get("values").and_then(Value::as_array) else {
        return Err(invalid(name, "state.values must be a list"));
    };
    if items.is_empty() {
        return Err(invalid(name, "state.values must not be empty"));
    }
    let mut seen = BTreeSet::new();
    let mut states = Vec::with_capacity(items.len());
    for item in items {
        match item.as_str() {
            Some(state) if !state.is_empty() && seen.insert(state.to_string()) => {
                states.push(state.to_string());
            }
            Some(_) => return Err(invalid(name, "state values must be unique and non-empty")),
            None => return Err(invalid(name, "state values must be strings")),
        }
    }
    Ok(states)
}

fn fields(name: &str, value: Option<&Value>) -> Result<Vec<FieldDef>, PackError> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Err(invalid(name, "field must be an array"));
    };
    let mut fields = Vec::with_capacity(items.len());
    let mut seen = BTreeSet::new();
    for item in items {
        let Some(table) = item.as_table() else {
            return Err(invalid(name, "field entry must be a table"));
        };
        require_keys(name, table, &["name", "type", "required", "target"])?;
        if table.contains_key("target") {
            require_keys(name, table, &["name", "type", "required", "target"])?;
        } else {
            require_keys(name, table, &["name", "type", "required"])?;
        }
        let field_name = table
            .get("name")
            .and_then(Value::as_str)
            .filter(|field| !field.is_empty())
            .ok_or_else(|| invalid(name, "field name must be a non-empty string"))?;
        if !seen.insert(field_name.to_string()) {
            return Err(invalid(name, format!("duplicate field {field_name}")));
        }
        let kind = match table.get("type").and_then(Value::as_str) {
            Some("string") => FieldKind::String,
            Some("strings") => FieldKind::Strings,
            Some("date") => FieldKind::Date,
            _ => return Err(invalid(name, "field type must be string, strings, or date")),
        };
        let required = table
            .get("required")
            .and_then(Value::as_bool)
            .ok_or_else(|| invalid(name, "field required must be a boolean"))?;
        let target = match table.get("target") {
            None => None,
            Some(Value::String(target)) if !target.is_empty() => Some(target.clone()),
            _ => return Err(invalid(name, "field target must be a non-empty string")),
        };
        fields.push(FieldDef {
            name: field_name.to_string(),
            kind,
            required,
            target,
        });
    }
    Ok(fields)
}

fn require_keys(
    name: &str,
    table: &toml::map::Map<String, Value>,
    allowed: &[&str],
) -> Result<(), PackError> {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(invalid(name, format!("unknown key {key}")));
        }
    }
    Ok(())
}

fn required_int(
    name: &str,
    table: &toml::map::Map<String, Value>,
    key: &str,
) -> Result<i64, PackError> {
    table
        .get(key)
        .and_then(Value::as_integer)
        .ok_or_else(|| invalid(name, format!("{key} must be an integer")))
}

fn invalid(name: &str, message: impl Into<String>) -> PackError {
    PackError::Invalid {
        name: name.to_string(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_pack_loads_ten_types() {
        let pack = core_pack().expect("core pack");
        let names: Vec<_> = pack.names().collect();
        assert_eq!(
            names,
            [
                "decision", "idea", "journal", "note", "org", "person", "place", "project",
                "source", "topic"
            ]
        );
        assert_eq!(pack.get("note").expect("note").folders, ["inbox", "notes"]);
        let source = pack.get("source").expect("source");
        assert!(source.fields.iter().any(|field| {
            field.name == "sha256" && field.kind == FieldKind::String && field.target.is_none()
        }));
        assert!(pack.get("journal").expect("journal").timeline);
        assert_eq!(
            pack.get("person").expect("person").folders,
            ["people".to_string()]
        );
    }

    #[test]
    fn malformed_pack_toml_is_invalid() {
        let err = load_pack([("person", "folder = [")]).expect_err("toml");
        assert!(matches!(err, PackError::Invalid { .. }));
    }

    #[test]
    fn folder_must_be_string_or_list() {
        let text = "schema_version = 1\nfolder = 1\ntimeline = false\n[state]\nvalues = [\"a\"]\nfield = []\n";
        let err = load_pack([("note", text)]).expect_err("folder");
        assert!(matches!(err, PackError::Invalid { .. }));
    }
}
