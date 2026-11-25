# ADR-0001: Check envelope and note file set

## Status

Accepted

## Date

2026-11-25

## Context

Later verbs need one JSON shape and one note parser. A hand-written schema would drift from the Rust types. Invalid notes have to fail before packs, writes, or search exist.

## Decision

`mnemic check --json` prints a `CheckEnvelope`. A clean tree has empty `errors` and `warnings` and a null `error`. Per-note failures keep those findings in `data` and set `error.code` to `CHECK_FAILED` (exit 2). Vault and usage failures set `data` to null. `LOCK_HELD` is exit 75 in that map.

The note file set ignores `README.md`, `AGENTS.md`, every dot-prefixed path, and `sources/raw/`. Core frontmatter does not include a visibility field. `mnemic.toml` v1 has `schema_version` and `actor`.

`schemars` generates `docs/contract/check.schema.json` and `docs/contract/check.md`. A test fails when those files differ from the types.

## Consequences

A new check code or envelope field updates the Rust type, the generated files, and the snapshots in one change. Type-specific rules and frontmatter edits stay out of this parser.
