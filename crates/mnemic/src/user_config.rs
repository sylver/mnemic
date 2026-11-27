//! User-level default vault. Not a vault path. Plain `std::fs` only.

use std::ffi::OsString;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// `$XDG_CONFIG_HOME/mnemic/config.toml` when that variable is set, non-empty,
/// and absolute. Otherwise `$HOME/.config/mnemic/config.toml`. A relative or
/// empty `XDG_CONFIG_HOME` is ignored. The same rule on Linux and macOS.
pub fn config_path(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    if let Some(xdg) = xdg {
        let path = PathBuf::from(&xdg);
        if !xdg.is_empty() && path.is_absolute() {
            return Some(path.join("mnemic").join("config.toml"));
        }
    }
    let home = home.filter(|value| !value.is_empty())?;
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("mnemic")
            .join("config.toml"),
    )
}

/// `Ok(None)` is an empty file or a file with no `vault` key.
/// Only `vault` is legal, and it must be an absolute path.
pub fn parse(text: &str) -> Result<Option<PathBuf>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let value: toml::Value = toml::from_str(text).map_err(|err| err.to_string())?;
    let Some(table) = value.as_table() else {
        return Err("config is not a table".to_string());
    };
    for key in table.keys() {
        if key != "vault" {
            return Err(format!("unknown key `{key}`"));
        }
    }
    let Some(vault) = table.get("vault") else {
        return Ok(None);
    };
    let Some(vault) = vault.as_str() else {
        return Err("vault must be a string".to_string());
    };
    let path = PathBuf::from(vault);
    if !path.is_absolute() {
        return Err("vault must be an absolute path".to_string());
    }
    Ok(Some(path))
}

pub fn render(vault: Option<&Path>) -> String {
    match vault {
        Some(path) => {
            let text = path.to_string_lossy();
            let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
            format!("vault = \"{escaped}\"\n")
        }
        None => String::new(),
    }
}

/// Missing file is `Ok(None)`. A parse failure is prefixed with `path`.
pub fn read(path: &Path) -> Result<Option<PathBuf>, String> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text).map_err(|err| format!("{}: {err}", path.display())),
        Err(err) if err.kind() == ErrorKind::NotFound && fs::symlink_metadata(path).is_err() => {
            Ok(None)
        }
        Err(err) => Err(format!("{}: {err}", path.display())),
    }
}

/// Write `vault`, or an empty file when `vault` is `None`.
/// Returns whether bytes changed. `--clear` with no file creates nothing.
/// A symlink is left in place: the temp file is created in the target's parent
/// and renamed over the target, including when the link is dangling.
pub fn write(path: &Path, vault: Option<&Path>) -> Result<bool, String> {
    let body = render(vault);
    let linked = fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false);
    if vault.is_none() && !linked && fs::symlink_metadata(path).is_err() {
        return Ok(false);
    }
    if existing_text(path).is_some_and(|current| current == body) {
        return Ok(false);
    }
    let (dest, temp_dir) = if linked {
        let target = fs::read_link(path).map_err(|err| err.to_string())?;
        let resolved = if target.is_absolute() {
            target
        } else {
            path.parent().unwrap_or(Path::new(".")).join(target)
        };
        let parent = parent_of(&resolved);
        (resolved, parent)
    } else {
        (path.to_path_buf(), parent_of(path))
    };
    fs::create_dir_all(&temp_dir).map_err(|err| err.to_string())?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temp = temp_dir.join(format!(".config.{}.{nanos}.mnemic-tmp", std::process::id()));
    fs::write(&temp, body).map_err(|err| err.to_string())?;
    if let Err(err) = fs::rename(&temp, &dest) {
        let _ = fs::remove_file(&temp);
        return Err(err.to_string());
    }
    Ok(true)
}

fn existing_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn parent_of(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn xdg(value: &str) -> Option<OsString> {
        Some(OsString::from(value))
    }

    #[test]
    fn config_path_prefers_absolute_xdg_then_home() {
        let from_xdg = config_path(xdg("/tmp/xdg"), xdg("/home/alice")).unwrap();
        assert_eq!(from_xdg, PathBuf::from("/tmp/xdg/mnemic/config.toml"));

        let relative = config_path(xdg("relative/xdg"), xdg("/home/alice")).unwrap();
        assert_eq!(
            relative,
            PathBuf::from("/home/alice/.config/mnemic/config.toml")
        );

        let empty = config_path(xdg(""), xdg("/home/alice")).unwrap();
        assert_eq!(
            empty,
            PathBuf::from("/home/alice/.config/mnemic/config.toml")
        );

        let home_only = config_path(None, xdg("/home/alice")).unwrap();
        assert_eq!(
            home_only,
            PathBuf::from("/home/alice/.config/mnemic/config.toml")
        );

        assert!(config_path(None, None).is_none());
        assert!(config_path(xdg(""), None).is_none());
        assert!(config_path(xdg("relative"), xdg("")).is_none());
    }

    #[test]
    fn parse_accepts_only_an_absolute_vault() {
        assert_eq!(parse("").unwrap(), None);
        assert_eq!(parse("   \n").unwrap(), None);
        assert_eq!(
            parse("vault = \"/Users/alice/notes\"\n").unwrap(),
            Some(PathBuf::from("/Users/alice/notes"))
        );
        assert!(parse("vualt = \"/x\"\n").unwrap_err().contains("vualt"));
        assert!(parse("vault = \"notes\"\n")
            .unwrap_err()
            .contains("absolute"));
        assert!(parse("vault = [\n").is_err());
        assert!(parse("vault = 1\n").is_err());
    }

    #[test]
    fn render_round_trips_the_one_key() {
        let text = render(Some(Path::new("/Users/alice/notes")));
        assert_eq!(text, "vault = \"/Users/alice/notes\"\n");
        assert_eq!(
            parse(&text).unwrap(),
            Some(PathBuf::from("/Users/alice/notes"))
        );
        assert_eq!(render(None), "");
    }
}
