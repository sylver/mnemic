# ADR-0001: Advisory write lock and durable rename

## Status

Accepted

## Date

2026-11-25

## Context

Many local processes write one vault, and v1 has no daemon. A PID file that is stolen when the recorded process is dead still has a takeover race and a PID-reuse lockout. A crash during a write must not leave a torn destination.

## Decision

Writers call `with_write_lock` and receive a `WriteTxn`. That call is the only way to obtain a `WriteTxn`. It takes `flock(LOCK_EX)` on `<vault>/.mnemic/write.lock`, waits with backoff up to 5 seconds, and returns `Error::LockHeld { pid, since }` when the wait expires. The kernel releases the lock on exit or crash. The lock file is never deleted. Bytes are published by an exclusive temp file in the same directory, `fsync`, rename, and `fsync` of the parent directory. The same thread may re-enter `with_write_lock` for that vault without taking a second `flock`.

This applies to local filesystems on macOS and Linux. Network and synced mounts are out of scope.

## Consequences

Later write verbs share one lock and one rename path. A helper that locks again inside a vault-wide operation cannot time out on this process. Concurrent writers outside mnemic still bypass the lock. Windows, NFS, SMB, and synced folders are unsupported for concurrent writers.
