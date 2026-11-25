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
    let value: Value =
        toml::from_str(&text).map_err(|err| VaultError::BadConfig(err.to_string()))?;
    let Some(table) = value.as_table() else {
        return Err(VaultError::BadConfig(
            "mnemic.toml is not a table".to_string(),
        ));
    };
    let version = match table.get("schema_version") {
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
        Some(Value::String(actor)) if !actor.is_empty() => actor.clone(),
        Some(_) => {
            return Err(VaultError::BadConfig(
                "actor must be a non-empty string".to_string(),
            ));
        }
        None => return Err(VaultError::BadConfig("missing actor".to_string())),
    };
    Ok(Vault {
        root: root.to_path_buf(),
        schema_version: version,
        actor,
    })
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
