use std::fs;
use std::path::{Path, PathBuf};

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
    let (root, config) = open_config(start, explicit)?;
    let actor = config
        .actor
        .ok_or_else(|| VaultError::BadConfig("missing actor".to_string()))?;
    Ok(Vault {
        root,
        schema_version: config.schema_version,
        actor,
    })
}

/// Vault root and parsed `mnemic.toml`. `actor` may be absent.
pub fn open_config(
    start: &Path,
    explicit: Option<&Path>,
) -> Result<(PathBuf, VaultConfig), VaultError> {
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

    let text = read_config_text(&root)?;
    let config = parse_vault_config(&text)?;
    Ok((root, config))
}

fn read_config_text(root: &Path) -> Result<String, VaultError> {
    fs::read_to_string(root.join("mnemic.toml")).map_err(|err| VaultError::Io(err.to_string()))
}

fn discover(start: &Path) -> Result<PathBuf, VaultError> {
    let cwd = std::env::current_dir().map_err(|err| VaultError::Io(err.to_string()))?;
    discover_from(start, &cwd)
}

/// Walk from `start` to the nearest `mnemic.toml`. A relative `start` is joined
/// onto `cwd` first so `pop` climbs real parents instead of stopping at `.`.
fn discover_from(start: &Path, cwd: &Path) -> Result<PathBuf, VaultError> {
    let joined = cwd.join(start);
    let mut cursor = if joined.is_dir() {
        joined
    } else if joined.is_file() {
        joined
            .parent()
            .ok_or_else(|| VaultError::Io(format!("{} has no parent", joined.display())))?
            .to_path_buf()
    } else {
        return Err(VaultError::Io(format!(
            "{} does not exist",
            joined.display()
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
    let text = read_config_text(root)?;
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
    let root = fs::canonicalize(vault_root).map_err(|err| ConfineError::Io(err.to_string()))?;
    let start = fs::canonicalize(path).map_err(|err| ConfineError::Io(err.to_string()))?;
    if start != root && !start.starts_with(&root) {
        return Err(ConfineError::Outside(format!(
            "{} is outside the vault",
            path.display()
        )));
    }
    Ok((root, start))
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

    #[test]
    fn discover_from_relative_start_walks_to_the_vault() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_path_buf();
        fs::write(vault.join("mnemic.toml"), "schema_version = 1\n").unwrap();
        let nested = vault.join("people").join("sub");
        fs::create_dir_all(&nested).unwrap();

        let from_dot = discover_from(Path::new("."), &nested).unwrap();
        assert_eq!(from_dot, vault);

        let from_sub = discover_from(Path::new("sub"), &vault.join("people")).unwrap();
        assert_eq!(from_sub, vault);
    }

    fn tempfile_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("mnemic-vault-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }
}
