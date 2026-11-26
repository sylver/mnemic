# ADR-0010: new files one note under the write lock

## Status

Accepted

## Date

2026-11-26

## Context

Agents need one way to create a note so the id, the core fields, and the folder stay consistent. The vault already has one index, one resolver, and one write lock. A second scanner would drift from `check`.

## Decision

`mnemic new` builds the file from the type's body template and writes it with `O_EXCL` inside `with_write_lock`. Id, slug, and alias collisions are read from `VaultIndex` in that same lock. The same id at the same path returns success and does not rewrite the file. Any other existing id is `ID_EXISTS`. The filename slug is unique against every other note's slug and alias after NFC case-folding.

The slug is `deunicode`, then lowercase ASCII, with punctuation collapsed to dashes. Journal files use `journal/YYYY/YYYY-MM-DD.md`, and a second file that day appends that slug. The journal `date` is `--date` or the process local calendar day, not the timestamp clock. `created` and `updated` still come from the injected clock. There is no `visibility` field.

## Consequences

A retry with the same `--id` is safe. A crash during the `O_EXCL` write can leave a partial file that a later create will not replace. `new` does not file a `note` into `notes/`. Callers that need a filed note move it later.
