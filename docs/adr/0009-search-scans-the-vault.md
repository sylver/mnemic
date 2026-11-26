# ADR-0009: Search scans the parsed vault

## Status

Accepted

## Date

2026-11-26

## Context

Agents need to look up a note before they create one. A SQLite FTS5 index would add a store that can drift from the files and a command to refresh it. A personal vault is a few thousand Markdown notes.

## Decision

`mnemic search` reads the notes `VaultIndex` already parsed. The query is literal tokens, not `MATCH` syntax. Filters only drop hits. There is no `mnemic index`, no FTS5, and no `--visibility`. SQLite stays deferred until a 5,000-note search on the macOS CI runner has a median at or above one second.

## Consequences

A deleted file is absent on the next search. The scan pays parse cost on every query. The budget test records that median and is the trigger to reopen an index.
