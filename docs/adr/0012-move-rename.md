# ADR-0012: Move renames without replacing, then rewrites links

## Status

Accepted

## Date

2026-11-26

## Context

A rename changes the path humans and wikilinks use. The ULID stays the identity. A crash between those writes must not lose the note or make the old slug unresolvable. Replacing an existing file would destroy another note.

## Decision

`mnemic mv` holds one `with_write_lock` and does three steps in order. When the slug changes, `FrontmatterEdit::append_sequence_item` adds the old slug to `aliases` and a second edit stamps `updated`, and both land in one `atomic_write`. The rename is `renameat_with(RENAME_NOREPLACE)`, which is `renameat2` on Linux and `renamex_np(RENAME_EXCL)` on macOS. Incoming wikilinks are then rewritten one note at a time. A folder-only move skips the alias. `--no-rewrite` skips the third step. A re-run whose source path is gone resolves the basename through `VaultIndex` and, only when that note is already at the destination, finishes the rewrite.

## Consequences

Old links resolve through the alias until the rewrite finishes. A second `mv` of the same arguments converges. `mv` refuses to create a slug or alias collision and refuses to replace a destination name. The crash seam `MNEMIC_MV_CRASH` exists so tests can stop after the alias write or after the rename.
