use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mnemic_core::write::{with_write_lock, DEFAULT_LOCK_TIMEOUT};
use mnemic_core::{exit_code, EnvelopeCode, Error};

#[test]
fn write_lock_worker() {
    let Ok(mode) = std::env::var("MNEMIC_WRITE_CHILD") else {
        return;
    };
    let code = match mode.as_str() {
        "increment" => child_increment(),
        "hold" => child_hold(),
        "crash" => child_crash(),
        _ => 1,
    };
    std::process::exit(code);
}

#[test]
fn write_lock_crash_before_rename_keeps_previous_bytes() {
    let vault = tempfile::tempdir().unwrap();
    let file = vault.path().join("note.md");
    fs::write(&file, b"previous").unwrap();

    let mut child = spawn("crash", vault.path(), &file, 5_000);
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(97));
    assert_eq!(fs::read(&file).unwrap(), b"previous");

    let temps: Vec<_> = leftover_temps(vault.path());
    assert_eq!(temps.len(), 1, "one leftover temp, got {temps:?}");
    let temp_name = temps[0].file_name().unwrap().to_string_lossy();
    assert!(
        temp_name.starts_with('.') && temp_name.contains(".mnemic-tmp-"),
        "{temp_name}"
    );

    with_write_lock(vault.path(), DEFAULT_LOCK_TIMEOUT, |txn| {
        txn.atomic_write(&file, b"next")
    })
    .unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"next");
    assert!(
        leftover_temps(vault.path()).is_empty(),
        "next writer removes leftovers"
    );
    assert!(vault.path().join(".mnemic/write.lock").is_file());
}

#[test]
fn write_lock_live_holder_returns_lock_held() {
    let vault = tempfile::tempdir().unwrap();
    let file = vault.path().join("note.md");
    fs::write(&file, b"original").unwrap();
    let ready = vault.path().join("ready");

    let mut child = spawn("hold", vault.path(), &file, 30_000);
    wait_for_file(&ready);

    let started = Instant::now();
    let error = with_write_lock(vault.path(), DEFAULT_LOCK_TIMEOUT, |txn| {
        txn.atomic_write(&file, b"torn")
    })
    .unwrap_err();
    let elapsed = started.elapsed();
    child.kill().unwrap();
    let _ = child.wait();

    match error {
        Error::LockHeld { pid, since } => {
            assert_eq!(pid, child.id());
            let age = SystemTime::now().duration_since(since).unwrap();
            assert!(age < Duration::from_secs(60), "since was {since:?}");
            assert!(since > UNIX_EPOCH);
        }
        other => panic!("expected LockHeld, got {other:?}"),
    }
    assert!(elapsed >= DEFAULT_LOCK_TIMEOUT);
    assert!(elapsed < Duration::from_secs(8), "wait ran for {elapsed:?}");
    assert_eq!(fs::read(&file).unwrap(), b"original");
}

#[test]
fn write_lock_kill_holder_releases_immediately() {
    let vault = tempfile::tempdir().unwrap();
    let file = vault.path().join("note.md");
    fs::write(&file, b"before-kill").unwrap();
    let ready = vault.path().join("ready");

    let mut child = spawn("hold", vault.path(), &file, 30_000);
    wait_for_file(&ready);
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(!status.success());

    let started = Instant::now();
    with_write_lock(vault.path(), DEFAULT_LOCK_TIMEOUT, |txn| {
        txn.atomic_write(&file, b"after-kill")
    })
    .unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(fs::read(&file).unwrap(), b"after-kill");
}

#[test]
fn write_lock_multiprocess_increments_match_successes() {
    let vault = tempfile::tempdir().unwrap();
    let file = vault.path().join("counter");
    fs::write(&file, b"0").unwrap();

    let mut children = Vec::new();
    for _ in 0..8 {
        children.push(spawn("increment", vault.path(), &file, 15_000));
    }

    let mut successes = 0u64;
    let mut lock_held = 0u64;
    for child in children {
        let output = child.wait_with_output().unwrap();
        match output.status.code() {
            Some(0) => successes += 1,
            Some(75) => lock_held += 1,
            other => {
                panic!(
                    "unexpected exit {other:?}\nstdout {}\nstderr {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }

    let text = fs::read_to_string(&file).unwrap();
    let count: u64 = text
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("torn counter {text:?}"));
    assert_eq!(successes + lock_held, 8);
    assert_eq!(count, successes);
    assert_eq!(lock_held, 0, "15s is enough for eight increments");
}

#[test]
fn write_lock_refuses_symlink_and_preserves_mode() {
    let vault = tempfile::tempdir().unwrap();
    let notes = vault.path().join("notes");
    fs::create_dir(&notes).unwrap();
    let file = notes.join("a.txt");
    fs::write(&file, b"old").unwrap();
    let mut permissions = fs::metadata(&file).unwrap().permissions();
    permissions.set_mode(0o640);
    fs::set_permissions(&file, permissions).unwrap();

    let link = notes.join("link.txt");
    symlink("a.txt", &link).unwrap();

    let error = with_write_lock(vault.path(), Duration::from_millis(200), |txn| {
        txn.atomic_write(&link, b"through-link")
    })
    .unwrap_err();
    assert!(matches!(error, Error::Io(_)), "{error:?}");
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(fs::read(&file).unwrap(), b"old");

    with_write_lock(vault.path(), DEFAULT_LOCK_TIMEOUT, |txn| {
        txn.atomic_write(&file, b"new")
    })
    .unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"new");
    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640);
}

#[test]
fn write_lock_nested_lock_does_not_self_wait() {
    let vault = tempfile::tempdir().unwrap();
    let file = vault.path().join("note.md");
    let started = Instant::now();
    with_write_lock(vault.path(), Duration::from_millis(200), |txn| {
        let _ = txn;
        with_write_lock(vault.path(), Duration::from_millis(200), |inner| {
            inner.atomic_write(&file, b"nested")
        })
    })
    .unwrap();
    assert!(started.elapsed() < Duration::from_millis(200));
    assert_eq!(fs::read(&file).unwrap(), b"nested");
    assert!(vault.path().join(".mnemic").is_dir());
}

fn spawn(mode: &str, vault: &Path, file: &Path, timeout_ms: u64) -> std::process::Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["write_lock_worker", "--exact", "--test-threads=1"])
        .env("MNEMIC_WRITE_CHILD", mode)
        .env("MNEMIC_VAULT", vault)
        .env("MNEMIC_FILE", file)
        .env("MNEMIC_TIMEOUT_MS", timeout_ms.to_string())
        .env_remove("MNEMIC_READY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn child_paths() -> (PathBuf, PathBuf, Duration) {
    let vault = PathBuf::from(std::env::var("MNEMIC_VAULT").unwrap());
    let file = PathBuf::from(std::env::var("MNEMIC_FILE").unwrap());
    let timeout_ms: u64 = std::env::var("MNEMIC_TIMEOUT_MS").unwrap().parse().unwrap();
    (vault, file, Duration::from_millis(timeout_ms))
}

fn child_increment() -> i32 {
    let (vault, file, timeout) = child_paths();
    match with_write_lock(&vault, timeout, |txn| {
        let current = fs::read_to_string(&file).unwrap_or_else(|_| "0".into());
        let value: u64 = current.trim().parse().unwrap_or(0);
        txn.atomic_write(&file, (value + 1).to_string().as_bytes())
    }) {
        Ok(()) => 0,
        Err(Error::LockHeld { .. }) => exit_code(EnvelopeCode::LockHeld),
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn child_hold() -> i32 {
    let (vault, _file, timeout) = child_paths();
    let ready = vault.join("ready");
    match with_write_lock(&vault, timeout, |_txn| {
        fs::write(&ready, b"ready").map_err(Error::from)?;
        std::thread::sleep(Duration::from_secs(60));
        Ok(())
    }) {
        Ok(()) => 0,
        Err(Error::LockHeld { .. }) => exit_code(EnvelopeCode::LockHeld),
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn child_crash() -> i32 {
    let (vault, file, timeout) = child_paths();
    match with_write_lock(&vault, timeout, |txn| {
        txn.crash_before_rename(&file, b"replacement")
    }) {
        Ok(()) => 0,
        Err(Error::LockHeld { .. }) => exit_code(EnvelopeCode::LockHeld),
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

fn leftover_temps(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.') && name.contains(".mnemic-tmp-"))
        })
        .collect()
}

fn wait_for_file(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        if start.elapsed() > Duration::from_secs(10) {
            panic!("timed out waiting for {}", path.display());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
