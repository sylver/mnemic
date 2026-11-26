use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::{is_temp_name, temp_name};
use crate::Error;

pub(crate) struct Staged {
    tmp: PathBuf,
}

pub(crate) fn stage_temp(path: &Path, bytes: &[u8]) -> Result<Staged, Error> {
    if is_symlink(path)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to replace symlink {}", path.display()),
        )
        .into());
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("path has no utf-8 file name: {}", path.display()),
            )
        })?;

    remove_leftover_temps(parent)?;

    let mut file = None;
    let mut tmp = None;
    for _ in 0..8 {
        let candidate = parent.join(temp_name(file_name, std::process::id(), random_u64()?));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(opened) => {
                file = Some(opened);
                tmp = Some(candidate);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let (mut file, tmp) = match (file, tmp) {
        (Some(file), Some(tmp)) => (file, tmp),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a unique temp file",
            )
            .into());
        }
    };

    file.write_all(bytes)?;
    if let Some(mode) = existing_mode(path)? {
        let mut permissions = file.metadata()?.permissions();
        permissions.set_mode(mode);
        file.set_permissions(permissions)?;
    }
    file.sync_all()?;
    Ok(Staged { tmp })
}

pub(crate) fn create_exclusive(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if is_symlink(path)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("refusing to replace symlink {}", path.display()),
        )
        .into());
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub(crate) fn commit_temp(path: &Path, staged: &Staged) -> Result<(), Error> {
    if is_symlink(path)? {
        let _ = fs::remove_file(&staged.tmp);
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to replace symlink {}", path.display()),
        )
        .into());
    }

    fs::rename(&staged.tmp, path)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn is_symlink(path: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_symlink()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn existing_mode(path: &Path) -> Result<Option<u32>, Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(metadata.permissions().mode() & 0o777)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn remove_leftover_temps(dir: &Path) -> Result<(), Error> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_temp_name(&name) {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

pub fn random_u64() -> Result<u64, Error> {
    let mut bytes = [0u8; 8];
    let mut urandom = File::open("/dev/urandom")?;
    std::io::Read::read_exact(&mut urandom, &mut bytes)?;
    Ok(u64::from_ne_bytes(bytes))
}
