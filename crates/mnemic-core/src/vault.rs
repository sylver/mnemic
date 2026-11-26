use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;
use toml::Value;

pub const SUPPORTED_VAULT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vault {
    pub root: PathBuf,
    pub schema_version: u32,
    pub actor: String,
}

/// Parsed `mnemic.toml`. `actor` is absent when the key was not written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultConfig {
    pub schema_version: u32,
    pub actor: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VaultError {
    #[error("no mnemic.toml found")]
    NotFound,
    #[error("{0}")]
    BadConfig(String),
    #[error("vault schema_version {found} is newer than this CLI ({supported})")]
    Unsupported { found: i64, supported: u32 },
    #[error("{0}")]
    Io(String),
}

pub fn open_vault(start: &Path, explicit: Option<&Path>) -> Result<Vault, VaultError> {
    let root = match explicit {
        Some(path) => {
            if !path.is_dir() {
                return Err(VaultError::Io(format!(
                    "{} is not a directory",
                    path.display()
                )));
            }
            if !path.join("mnemic.toml").is_file() {
                return Err(VaultError::NotFound);
            }
            path.to_path_buf()
        }
        None => discover(start)?,
    };
    load_vault(&root)
}

fn discover(start: &Path) -> Result<PathBuf, VaultError> {
    let mut cursor = if start.is_dir() {
        start.to_path_buf()
    } else if start.is_file() {
        start
            .parent()
            .ok_or_else(|| VaultError::Io(format!("{} has no parent", start.display())))?
            .to_path_buf()
    } else {
        return Err(VaultError::Io(format!(
            "{} does not exist",
            start.display()
        )));
    };
    loop {
        if cursor.join("mnemic.toml").is_file() {
            return Ok(cursor);
        }
        if !cursor.pop() {
            return Err(VaultError::NotFound);
        }
    }
}

pub fn load_vault(root: &Path) -> Result<Vault, VaultError> {
    let path = root.join("mnemic.toml");
    let text = fs::read_to_string(&path).map_err(|err| VaultError::Io(err.to_string()))?;
    let config = parse_vault_config(&text)?;
    let actor = config
        .actor
        .ok_or_else(|| VaultError::BadConfig("missing actor".to_string()))?;
    Ok(Vault {
        root: root.to_path_buf(),
        schema_version: config.schema_version,
        actor,
    })
}

pub fn parse_vault_config(text: &str) -> Result<VaultConfig, VaultError> {
    let value: Value =
        toml::from_str(text).map_err(|err| VaultError::BadConfig(err.to_string()))?;
    let Some(table) = value.as_table() else {
        return Err(VaultError::BadConfig(
            "mnemic.toml is not a table".to_string(),
        ));
    };
    let schema_version = match table.get("schema_version") {
        None => return Err(VaultError::BadConfig("missing schema_version".to_string())),
        Some(Value::Integer(found)) if *found > i64::from(SUPPORTED_VAULT_SCHEMA_VERSION) => {
            return Err(VaultError::Unsupported {
                found: *found,
                supported: SUPPORTED_VAULT_SCHEMA_VERSION,
            });
        }
        Some(Value::Integer(found)) if *found == i64::from(SUPPORTED_VAULT_SCHEMA_VERSION) => {
            SUPPORTED_VAULT_SCHEMA_VERSION
        }
        Some(_) => {
            return Err(VaultError::BadConfig(
                "schema_version must be an integer this CLI supports".to_string(),
            ));
        }
    };
    let actor = match table.get("actor") {
        Some(Value::String(actor)) if !actor.is_empty() => Some(actor.clone()),
        Some(_) => {
            return Err(VaultError::BadConfig(
                "actor must be a non-empty string".to_string(),
            ));
        }
        None => None,
    };
    Ok(VaultConfig {
        schema_version,
        actor,
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ConfineError {
    Io(String),
    Outside(String),
}

/// Canonical `path` when it is `vault_root` or a path inside that root.
pub(crate) fn canonicalize_within(
    vault_root: &Path,
    path: &Path,
) -> Result<(PathBuf, PathBuf), ConfineError> {
    let mut cache = ConfineCache::open(vault_root)?;
    let start = cache.canonicalize(path)?;
    Ok((cache.root, start))
}

/// One canonical vault root, reused across many note paths.
pub(crate) struct ConfineCache {
    root: PathBuf,
    dirs: HashMap<PathBuf, PathBuf>,
}

impl ConfineCache {
    pub(crate) fn open(vault_root: &Path) -> Result<Self, ConfineError> {
        let root = fs::canonicalize(vault_root).map_err(|err| ConfineError::Io(err.to_string()))?;
        Ok(Self {
            root,
            dirs: HashMap::new(),
        })
    }

    pub(crate) fn canonicalize(&mut self, path: &Path) -> Result<PathBuf, ConfineError> {
        if path.starts_with(&self.root) {
            return self.walk(path.strip_prefix(&self.root).unwrap_or(Path::new("")));
        }
        let start = fs::canonicalize(path).map_err(|err| ConfineError::Io(err.to_string()))?;
        if !inside(&self.root, &start) {
            return Err(ConfineError::Outside(format!(
                "{} is outside the vault",
                path.display()
            )));
        }
        Ok(start)
    }

    fn walk(&mut self, suffix: &Path) -> Result<PathBuf, ConfineError> {
        let parts: Vec<Component<'_>> = suffix.components().collect();
        let mut resolved = self.root.clone();
        for (index, component) in parts.iter().enumerate() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    if resolved == self.root {
                        return Err(ConfineError::Outside(format!(
                            "{} is outside the vault",
                            suffix.display()
                        )));
                    }
                    resolved.pop();
                }
                Component::Normal(name) => {
                    let next = resolved.join(name);
                    let last = index + 1 == parts.len();
                    if !last {
                        if let Some(cached) = self.dirs.get(&next) {
                            resolved = cached.clone();
                            continue;
                        }
                    }
                    let meta = fs::symlink_metadata(&next)
                        .map_err(|err| ConfineError::Io(err.to_string()))?;
                    if meta.file_type().is_symlink() {
                        let canon = fs::canonicalize(&next)
                            .map_err(|err| ConfineError::Io(err.to_string()))?;
                        if !inside(&self.root, &canon) {
                            return Err(ConfineError::Outside(format!(
                                "{} is outside the vault",
                                next.display()
                            )));
                        }
                        if !last {
                            self.dirs.insert(next, canon.clone());
                        }
                        resolved = canon;
                    } else {
                        if !last {
                            self.dirs.insert(next.clone(), next.clone());
                        }
                        resolved = next;
                    }
                }
                _ => {
                    return Err(ConfineError::Outside(format!(
                        "{} is outside the vault",
                        suffix.display()
                    )));
                }
            }
        }
        Ok(resolved)
    }
}

fn inside(root: &Path, path: &Path) -> bool {
    path == root || path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_malformed_missing_and_newer_versions() {
        let dir = tempfile_dir();
        fs::write(dir.join("mnemic.toml"), "schema_version = [\n").unwrap();
        assert!(matches!(load_vault(&dir), Err(VaultError::BadConfig(_))));

        fs::write(dir.join("mnemic.toml"), "schema_version = 1\n").unwrap();
        assert!(matches!(
            load_vault(&dir),
            Err(VaultError::BadConfig(message)) if message.contains("actor")
        ));

        fs::write(
            dir.join("mnemic.toml"),
            "schema_version = 2\nactor = \"sylver\"\n",
        )
        .unwrap();
        assert!(matches!(
            load_vault(&dir),
            Err(VaultError::Unsupported { found: 2, .. })
        ));
    }

    fn tempfile_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("mnemic-vault-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }
}
