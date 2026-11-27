# ADR-0005: Init scaffolds one directory and leaves existing files

## Status

Accepted. The vault `AGENTS.md` decision is superseded by ADR-0013.

## Date

2026-11-26

## Context

`mnemic init` is the first command that creates a vault. A vault is meant to be its own repository, but a target can already hold notes, a `.gitignore`, or an `AGENTS.md`. The check command requires `actor` in `mnemic.toml`. Init is also invoked without `--as`.

## Decision

`init` writes only into the directory it is given. It does not walk to a parent vault. A parent that already has `mnemic.toml` is `NESTED_VAULT`, and the target is not modified.

Files that already exist are not replaced. `.gitignore` is the exception: a missing `.mnemic/` line is appended. A vault-root `AGENTS.md` is skipped and reported in `data.skipped`. The agent template stays in the binary. Init does not copy it under `types/`.

`actor` is written only when `--as` is passed and only when `mnemic.toml` is created. A later `init` does not fill it in. `check` still requires `actor`.

## Consequences

Re-running `init` converges on the same scaffold. A vault created without `--as` is not a vault `check` will open until `actor` is added by hand. Template changes in a later CLI release do not reach vaults that already have `AGENTS.md`.
