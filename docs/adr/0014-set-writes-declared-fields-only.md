# ADR-0014: set writes declared fields only

## Status

Accepted

## Date

2026-11-27

## Context

Types can declare fields, and `check` already requires the ones marked `required`. No command wrote those fields, so a required custom field made every `new` of that type fail `check`, and agents edited the YAML by hand. The vault already has one pack, one byte-preserving editor, and one write lock. Link targets are checked by `check` alone (ADR-0006). `aliases` and `supersedes` are identity and link structure, not payload.

## Decision

`mnemic set` and `new --field` write a field the note's type declares, plus `state`, `tags`, and `title`. They refuse `id`, `type`, `schema_version`, `created`, `created_by`, `updated`, `status`, `aliases`, and `supersedes`, and they refuse any key the type does not declare. They do not resolve a link target at write time. They do not unset a key.

## Consequences

A type with a required field can pass `check` on the `new` that creates it. An unknown key cannot land in frontmatter through these commands. `aliases` still change through `mv`. A broken link still shows up only in `check`. A later command would have to add an explicit unset.
