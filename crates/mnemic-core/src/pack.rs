use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

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

    /// Same-name entries replace the declaration whole. Folder strings must stay unique.
    pub fn overlay(mut self, overlay: Pack) -> Result<Pack, PackError> {
        for (name, def) in overlay.types {
            self.types.insert(name, def);
        }
        self.ensure_unique_folders()?;
        Ok(self)
    }

    fn ensure_unique_folders(&self) -> Result<(), PackError> {
        let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
        for def in self.types.values() {
            for folder in &def.folders {
                if let Some(other) = seen.insert(folder.as_str(), def.name.as_str()) {
                    let (left, right) = if other <= def.name.as_str() {
                        (other, def.name.as_str())
                    } else {
                        (def.name.as_str(), other)
                    };
                    return Err(invalid(
                        "types",
                        format!("folder {folder} claimed by {left} and {right}"),
                    ));
                }
            }
        }
        Ok(())
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

pub fn core_template(name: &str) -> Option<&'static str> {
    Some(match name {
        "decision" => include_str!("../pack/templates/decision.md"),
        "idea" => include_str!("../pack/templates/idea.md"),
        "journal" => include_str!("../pack/templates/journal.md"),
        "note" => include_str!("../pack/templates/note.md"),
        "org" => include_str!("../pack/templates/org.md"),
        "person" => include_str!("../pack/templates/person.md"),
        "place" => include_str!("../pack/templates/place.md"),
        "project" => include_str!("../pack/templates/project.md"),
        "source" => include_str!("../pack/templates/source.md"),
        "topic" => include_str!("../pack/templates/topic.md"),
        _ => return None,
    })
}

pub fn resolve_pack(vault_root: &Path) -> Result<Pack, PackError> {
    let core = core_pack()?;
    let dir = vault_root.join("types");
    if !dir.is_dir() {
        return Ok(core);
    }
    core.overlay(load_overlay_dir(&dir)?)
}

pub fn load_overlay_dir(dir: &Path) -> Result<Pack, PackError> {
    let mut paths = Vec::new();
    let entries = fs::read_dir(dir).map_err(|err| invalid("types", err.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|err| invalid("types", err.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") && path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    let mut loaded = Vec::with_capacity(paths.len());
    for path in paths {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| invalid("types", "overlay file name is not a type name"))?
            .to_string();
        let text = fs::read_to_string(&path).map_err(|err| invalid(&name, err.to_string()))?;
        loaded.push((name, text));
    }
    let files: Vec<(&str, &str)> = loaded
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    load_pack(files)
}

/// Body template for `new`. `<vault>/types/<type>.md` replaces the embedded template.
/// `check` does not call this. `None` means the template is missing.
pub fn template_body(vault_root: &Path, name: &str) -> Result<Option<String>, PackError> {
    let path = vault_root.join("types").join(format!("{name}.md"));
    if path.is_file() {
        return fs::read_to_string(&path)
            .map(Some)
            .map_err(|err| invalid(name, err.to_string()));
    }
    Ok(core_template(name).map(str::to_string))
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
    let pack = Pack { types };
    pack.ensure_unique_folders()?;
    Ok(pack)
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
    fn core_templates_are_body_only_markdown() {
        let pack = core_pack().expect("core pack");
        for name in pack.names() {
            let body = core_template(name).unwrap_or_else(|| panic!("missing template {name}"));
            assert!(!body.contains("{{"), "{name}");
            assert!(!body.contains("---"), "{name}");
            assert!(!body.contains("visibility"), "{name}");
        }
    }

    #[test]
    fn same_name_overlay_replaces_the_declaration_whole() {
        let base = core_pack().expect("core");
        let overlay = load_pack([(
            "person",
            "schema_version = 1\nfolder = \"folks\"\ntimeline = false\n[state]\nvalues = [\"current\"]\n[[field]]\nname = \"badge\"\ntype = \"string\"\nrequired = true\n",
        )])
        .expect("overlay");
        let custom = load_pack([(
            "quest",
            "schema_version = 1\nfolder = \"quests\"\ntimeline = false\nfield = []\n[state]\nvalues = [\"open\"]\n",
        )])
        .expect("custom");
        let merged = base
            .overlay(overlay)
            .expect("replace")
            .overlay(custom)
            .expect("add");
        let person = merged.get("person").expect("person");
        assert_eq!(person.folders, ["folks"]);
        assert_eq!(person.schema_version, 1);
        assert_eq!(person.fields.len(), 1);
        assert_eq!(person.fields[0].name, "badge");
        assert!(person.fields[0].required);
        assert_eq!(merged.get("quest").expect("quest").folders, ["quests"]);
        assert_eq!(
            merged.get("note").expect("note").folders,
            ["inbox".to_string(), "notes".to_string()]
        );
    }

    #[test]
    fn lower_overlay_schema_version_is_the_loaded_declaration() {
        let base = load_pack([(
            "person",
            "schema_version = 2\nfolder = \"people\"\ntimeline = false\nfield = []\n[state]\nvalues = [\"current\"]\n",
        )])
        .expect("base");
        let overlay = load_pack([(
            "person",
            "schema_version = 1\nfolder = \"people\"\ntimeline = false\nfield = []\n[state]\nvalues = [\"current\"]\n",
        )])
        .expect("overlay");
        let merged = base.overlay(overlay).expect("lower version is valid");
        assert_eq!(merged.get("person").expect("person").schema_version, 1);
    }

    #[test]
    fn good_fixture_bodies_match_shipped_templates() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/good");
        let notes = [
            ("decisions/folder.md", "decision"),
            ("ideas/pack.md", "idea"),
            ("log/welcome.md", "journal"),
            ("notes/scratch.md", "note"),
            ("orgs/rivendell.md", "org"),
            ("people/ada.md", "person"),
            ("places/shire.md", "place"),
            ("projects/mnemic.md", "project"),
            ("sources/blob.md", "source"),
            ("topics/types.md", "topic"),
        ];
        for (rel, type_name) in notes {
            let text = fs::read_to_string(root.join(rel)).expect(rel);
            let note = crate::frontmatter::parse_note(&text).expect(rel);
            assert_eq!(
                note.body,
                core_template(type_name).expect(type_name),
                "{rel}"
            );
        }
    }

    #[test]
    fn vault_template_file_replaces_the_embedded_body() {
        let dir = std::env::temp_dir().join(format!("mnemic-template-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("types")).expect("dir");
        fs::write(dir.join("types/note.md"), "Custom body.\n").expect("write");
        let body = template_body(&dir, "note")
            .expect("template")
            .expect("present");
        assert_eq!(body, "Custom body.\n");
        assert!(template_body(&dir, "quest").expect("missing").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_clash_is_invalid_pack() {
        let base = core_pack().expect("core");
        let overlay = load_pack([(
            "person",
            "schema_version = 1\nfolder = \"notes\"\ntimeline = false\nfield = []\n[state]\nvalues = [\"current\"]\n",
        )])
        .expect("overlay");
        let err = base.overlay(overlay).expect_err("clash");
        assert!(matches!(err, PackError::Invalid { .. }));
        assert!(err.to_string().contains("notes"));
    }

    #[test]
    fn folder_must_be_string_or_list() {
        let text = "schema_version = 1\nfolder = 1\ntimeline = false\n[state]\nvalues = [\"a\"]\nfield = []\n";
        let err = load_pack([("note", text)]).expect_err("folder");
        assert!(matches!(err, PackError::Invalid { .. }));
    }
}
