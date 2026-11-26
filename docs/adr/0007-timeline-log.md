# ADR-0007: Timeline lines are appended under one lock

## Status

Accepted

## Date

2026-11-26

## Context

`mnemic log` is the first verb that edits a note that already exists. The timeline is append-only, and two processes can log the same note at once. A second writer, or a rewrite of older lines, would drop history.

## Decision

`log` resolves the note with `VaultIndex::resolve` inside `with_write_lock`, then reads and writes that file before the lock is released. The stored line is a Markdown list item in the format grammar. Insertion is newest-first by calendar date. The same date, source, and summary is a no-op and does not stamp `updated`. `updated` is a `FrontmatterEdit` replace from the injected clock. A missing `## Timeline` is created at the end of the note when the type allows a timeline. A path that canonicalizes outside the vault is refused before the read.

## Consequences

Concurrent logs on one note serialize on the vault lock. A process that waits out the lock exits `LOCK_HELD` and writes nothing. Later verbs that edit a note use this same lock scope. `log` does not sort existing lines and does not rewrite them.
