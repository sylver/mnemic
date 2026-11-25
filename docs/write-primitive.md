# Write primitive

CLI writes go through `mnemic_core::write::with_write_lock`. Resolve the target and read it inside the closure. Do not invent a write path.

`with_write_lock` is the only constructor of `WriteTxn`. `WriteTxn::atomic_write` takes `&WriteTxn`. There is no public write that skips the lock. A nested `with_write_lock` on the same vault, from the same thread, reuses the held lock. It does not wait on this process and then return `Error::LockHeld`.

## Lock

The lock file is `<vault>/.mnemic/write.lock`. The primitive creates `.mnemic/` when it is missing. It never deletes the lock file.

The lock is `flock(LOCK_EX)` via `rustix`. The kernel releases it when the holder exits or crashes, including `kill -9`. The file contents are a pid and a unix timestamp, written only so `Error::LockHeld { pid, since }` can name the holder. They are not the lock.

The wait polls with backoff until `--lock-timeout`. The library default is `DEFAULT_LOCK_TIMEOUT` (5 seconds). When the wait expires the primitive returns `Error::LockHeld`. Later verbs map that variant to the shared envelope code `LOCK_HELD`, exit 75. Other failures are `Error::Io`.

One lock covers the whole operation. Multi-file work (`mv`, vault-wide `--fix`) calls `with_write_lock` once and resolves every path inside that closure.

## Atomic write

`atomic_write` creates `.<name>.mnemic-tmp-<pid>-<rand>` in the destination directory with `O_EXCL`, `fsync`s the file, renames it over the destination, then `fsync`s the parent directory. The original file mode is copied onto the replacement. A symlinked destination is refused. The previous destination bytes stay intact if the process dies before rename.

Temp names start with `.` and contain `.mnemic-tmp-`. A note-file scan that skips dotfiles ignores them. The next writer deletes leftovers. Until then they are unused.

`WriteTxn::crash_before_rename` is the crash seam. It `fsync`s the temp file and exits before rename.

## Scope

macOS and Linux, on local filesystems. NFS, SMB, and synced folders (iCloud, Dropbox) are not supported for concurrent writers. `flock` on those mounts does not provide the mutual exclusion this primitive promises.

Writers that are not mnemic (Obsidian, `git pull`) do not take this lock. There is no daemon, no write receipt, and no cross-machine lock.
