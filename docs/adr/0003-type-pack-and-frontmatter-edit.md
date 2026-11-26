# ADR-0003: Embedded type pack and span-preserving edits

## Status

Accepted

## Date

2026-11-25

## Context

Type rules need one pack, and later write verbs need to change a frontmatter key without rewriting the rest of the file. A second editor per verb would drop comments, key order, and quoting. A vault overlay would fork the pack before the core types are stable.

## Decision

The ten core types ship as TOML compiled into `mnemic-core`. v1 has no `[packs]` table. Vault overlays are [ADR-0004](0004-templates-and-vault-overlays.md). `folder` is a string or a list. `note` owns `inbox` and `notes`. `source.sha256` is part of the type so a later drift check can read it.

`FrontmatterEdit` is a pure function of the parser's key spans. Insert, replace, and `create_block` return new bytes. Writers pass those bytes to `WriteTxn::atomic_write`. They do not build their own YAML rewrite.

## Consequences

A pack change is a CLI release. `check` reports `INVALID_PACK` when a type file fails to parse. Notes whose `schema_version` is newer than the loaded type fail closed. Templates and vault overlays are [ADR-0004](0004-templates-and-vault-overlays.md).
