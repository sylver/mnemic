# ADR-0008: get and links share the vault index

## Status

Accepted

## Date

2026-11-26

## Context

Agents need one read path for a note and its links. A second scanner would drift from `check`. Backlinks are derived. The verbs must not store them in Markdown, and a path that escapes the vault must not be read.

## Decision

`mnemic get` and `mnemic links` resolve a user argument through `VaultIndex::resolve_argument` and classify edges with `VaultIndex::link_sets`. `log` uses that same argument check. An edge kind is `wikilink`, `embed`, or `frontmatter:<field>`. `links` lists unresolved edges and exits 0. `check` is still the command that fails, and it keeps calling `VaultIndex::resolve`. A query containing `..`, an absolute path outside the vault, or a path whose canonical form leaves the vault is `OUTSIDE_VAULT`. The check is `canonicalize_within` and does not open the target. The note bytes are read only after that check accepts the path. The vault walk stays `collect_notes`.

## Consequences

Link rules stay in the index. `get` and `links` grow new fatal codes `NOT_FOUND`, `AMBIGUOUS`, and `OUTSIDE_VAULT` on the shared envelope. They do not accept a visibility flag.
