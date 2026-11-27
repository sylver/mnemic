# ADR-0013: Vault root gets a static README, not AGENTS.md

## Status

Accepted

## Date

2026-11-27

## Context

ADR-0005 decided that `mnemic init` writes a vault-root `AGENTS.md` from a binary template and skips an existing file. That file mixed agent procedure into the vault scaffold. The agent guide is `mnemic skill`. A new vault still needs a short statement of what the directory is.

## Decision

`init` writes a short static `README.md` when no entry exists at that path. An existing entry is left unchanged and listed in `data.skipped`. `init` does not write `AGENTS.md`.

This supersedes ADR-0005's vault-`AGENTS.md` decision and the consequence that a later template does not reach vaults that already have `AGENTS.md`. The rest of ADR-0005 stands.

## Consequences

A new vault's root `README.md` says what the vault is and points agents at `mnemic skill`. An existing `README.md` is never refreshed. An existing vault `AGENTS.md` stays in place and is not reported. A later CLI template reaches only vaults that do not already have `README.md`.
