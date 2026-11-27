use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::fs::{flock, FlockOperation};

use crate::Error;

/// How long a writer waits for .mnemic/write.lock before returning LockHeld. Every CLI verb uses it.
pub const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(5);

const INITIAL_BACKOFF: Duration = Duration::from_millis(20);
const MAX_BACKOFF: Duration = Duration::from_millis(200);

thread_local! {
    static HELD: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct HeldMark {
    path: PathBuf,
}

impl HeldMark {
    pub(crate) fn new(path: PathBuf) -> Self {
        HELD.with(|held| held.borrow_mut().push(path.clone()));
        Self { path }
    }
}

impl Drop for HeldMark {
    fn drop(&mut self) {
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            if let Some(index) = held.iter().rposition(|path| path == &self.path) {
                held.remove(index);
            }
        });
    }
}

pub(crate) fn already_held(lock_path: &Path) -> bool {
    HELD.with(|held| held.borrow().iter().any(|path| path == lock_path))
}

pub(crate) struct LockGuard {
    file: File,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = flock(&self.file, FlockOperation::Unlock);
    }
}

pub(crate) fn lock_path(vault: &Path) -> Result<PathBuf, Error> {
    let vault = fs::canonicalize(vault)?;
    if !vault.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("vault directory does not exist: {}", vault.display()),
        )
        .into());
    }
    let dir = vault.join(".mnemic");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("write.lock"))
}

pub(crate) fn acquire(lock_path: &Path, timeout: Duration) -> Result<LockGuard, Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;

    let deadline = Instant::now() + timeout;
    let mut backoff = INITIAL_BACKOFF;
    loop {
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {
                write_diagnostics(&mut file)?;
                return Ok(LockGuard { file });
            }
            Err(errno) if is_contended(errno) => {
                let now = Instant::now();
                if now >= deadline {
                    let (pid, since) = read_holder(lock_path);
                    return Err(Error::LockHeld { pid, since });
                }
                let remaining = deadline.saturating_duration_since(now);
                thread::sleep(backoff.min(remaining));
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
            Err(errno) => return Err(io::Error::from(errno).into()),
        }
    }
}

fn is_contended(errno: rustix::io::Errno) -> bool {
    let error = io::Error::from(errno);
    error.kind() == io::ErrorKind::WouldBlock || error.kind() == io::ErrorKind::Interrupted
}

fn write_diagnostics(file: &mut File) -> Result<(), Error> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    writeln!(file, "{}", std::process::id())?;
    writeln!(file, "{secs}")?;
    file.sync_all()?;
    Ok(())
}

fn read_holder(lock_path: &Path) -> (u32, SystemTime) {
    let text = fs::read_to_string(lock_path).unwrap_or_default();
    let mut lines = text.lines();
    let pid = lines
        .next()
        .and_then(|line| line.trim().parse().ok())
        .unwrap_or(0);
    let since = lines
        .next()
        .and_then(|line| line.trim().parse::<u64>().ok())
        .map(|secs| UNIX_EPOCH + Duration::from_secs(secs))
        .unwrap_or(UNIX_EPOCH);
    (pid, since)
}
