# ADR-0004: Body templates and whole-replace vault overlays

## Status

Accepted

## Date

2026-11-26

## Context

`new` needs a body for each type, and a vault needs a way to add a type or replace a built-in one without a second CLI. A field-by-field merge would make `folder` and `schema_version` ambiguous, and two types in one folder would make a later `--fix` pick a folder at random.

## Decision

Each core type ships a body-only Markdown template in the binary. There is no placeholder syntax. `init` does not copy templates into the vault.

`<vault>/types/<type>.toml` is the only overlay. The file stem is the type name. A new name adds a type. The same name replaces that type's declaration whole, including `folder` and `schema_version`. The overlay version is the one `SCHEMA_VERSION_AHEAD` compares, even when it is lower than the built-in it replaced. Two types that share a folder string, and malformed overlay TOML, are `INVALID_PACK`. `<vault>/types/<type>.md` replaces the embedded body. `check` ignores files under `types/` and does not require a template.

## Consequences

A vault can change a built-in folder. `check` still does not compare a note's directory to `folder`. `MISSING_TEMPLATE` belongs to `new`. There is no `[packs]` key and no second overlay path.
