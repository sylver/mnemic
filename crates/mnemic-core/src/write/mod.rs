//! Crash-safe, mutually exclusive writes for one vault on one machine.
//!
//! [`with_write_lock`] is the only way to obtain a [`WriteTxn`]. Atomic writes
//! take `&WriteTxn`, so a nested call cannot take a second `flock` on the same
//! vault and time itself out.

mod atomic;
mod lock;

use std::path::Path;
use std::time::Duration;

use atomic::stage_temp;
pub use lock::DEFAULT_LOCK_TIMEOUT;

use crate::Error;

/// Token that proves the caller holds the vault write lock.
///
/// Constructed only by [`with_write_lock`].
pub struct WriteTxn {
    _not_send_marker: std::marker::PhantomData<*const ()>,
}

impl WriteTxn {
    /// Replace `path` with `bytes` via a same-directory temp file and rename.
    ///
    /// A crash before the rename leaves the previous bytes in place.
    pub fn atomic_write(&self, path: &Path, bytes: &[u8]) -> Result<(), Error> {
        let staged = stage_temp(path, bytes)?;
        atomic::commit_temp(path, &staged)
    }

    /// Create `path` with `O_EXCL` and write `bytes`. An existing path is left unchanged.
    pub fn create_exclusive(&self, path: &Path, bytes: &[u8]) -> Result<(), Error> {
        atomic::create_exclusive(path, bytes)
    }

    /// Write and `fsync` the temp file, then exit the process before rename.
    ///
    /// Named crash seam for tests. Does not return.
    pub fn crash_before_rename(&self, path: &Path, bytes: &[u8]) -> Result<(), Error> {
        let _staged = stage_temp(path, bytes)?;
        std::process::exit(97);
    }
}

/// Run `body` while holding the exclusive write lock for `vault`.
///
/// The target path is resolved inside `body`. The same thread can call this
/// again for the same vault without waiting on its own lock.
pub fn with_write_lock<T>(
    vault: &Path,
    timeout: Duration,
    body: impl FnOnce(&WriteTxn) -> Result<T, Error>,
) -> Result<T, Error> {
    let lock_path = lock::lock_path(vault)?;
    if lock::already_held(&lock_path) {
        return body(&WriteTxn {
            _not_send_marker: std::marker::PhantomData,
        });
    }

    let _guard = lock::acquire(&lock_path, timeout)?;
    let _mark = lock::HeldMark::new(lock_path);
    body(&WriteTxn {
        _not_send_marker: std::marker::PhantomData,
    })
}

/// Temp path shape: `.<name>.mnemic-tmp-<pid>-<rand>`.
pub(crate) fn temp_name(file_name: &str, pid: u32, rand: u64) -> String {
    format!(".{file_name}.mnemic-tmp-{pid}-{rand:016x}")
}

pub(crate) fn is_temp_name(name: &str) -> bool {
    name.starts_with('.') && name.contains(".mnemic-tmp-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_lock_temp_name_is_dotfile_with_marker() {
        let name = temp_name("note.md", 42, 0xabc);
        assert!(name.starts_with('.'), "{name}");
        assert!(name.contains(".mnemic-tmp-"), "{name}");
        assert!(is_temp_name(&name));
        assert!(!is_temp_name("note.md"));
        assert!(!is_temp_name(".mnemic"));
    }
}
