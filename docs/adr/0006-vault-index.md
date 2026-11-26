# ADR-0006: One vault index and one resolver

## Status

Accepted

## Date

2026-11-26

## Context

Unique ids, aliases, supersession, and wikilinks all need the same map of the vault. Per-note `check` cannot see a duplicate id or a cycle. A later `links` verb would otherwise build a second resolver.

## Decision

`VaultIndex` in `mnemic-core` is the map. Notes are keyed by id, with path, slug, and alias indexes. `VaultIndex::resolve` tries a vault-relative path first, then an exact id, then a case-folded NFC slug, then a case-folded NFC alias. More than one slug or alias hit is `AMBIGUOUS` and lists the paths. `mnemic check` is the only broken-link reporter. It does not write backlinks, and it does not warn on inbox age.

## Consequences

`get`, `links`, `mv`, `new`, `log`, and search call `VaultIndex::resolve` instead of parsing links again. A new link rule changes this resolver and the check codes together. Duplicate basenames stay legal in the map. Creating one is still a later `SLUG_COLLISION`.
