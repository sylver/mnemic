# ADR-0011: check --fix inserts only derived id and type

## Status

Accepted

## Date

2026-11-26

## Context

Adoption needs a verb that can add a missing `id` and a missing `type` without rewriting the rest of the note. A YAML round-trip would change comments, key order, and quoting. Guessing `title`, dates, or `status` would invent content the file does not have.

## Decision

`mnemic check --fix` runs under one vault write lock. It inserts a missing `id` from the seeded ULID generator and the caller clock, and a missing `type` when the top-level folder matches exactly one type `folder`. A file with no frontmatter block gets `FrontmatterEdit::create_block` with only those keys. Every other byte is copied. The bytes go through `WriteTxn::atomic_write`. A present `id` or `type` is not replaced. `updated` is not stamped. Root `AGENTS.md` and `README.md` are outside the note-file set, so they are not written.

## Consequences

After `--fix`, a note can still fail `check` on the other required fields. A second run is a no-op when the inserted keys remain. A crash before rename leaves the previous file. An I/O error midway leaves earlier renames in place, and a later run converges. The id stream is deterministic for a clock and a seed, so two runs in the same millisecond can mint the same first id.
