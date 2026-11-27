# ADR-0015: Default vault lives in user config, below the directory walk

## Status

Accepted

## Date

2026-11-27

## Context

Without a binding, a user outside their vault must pass `--vault` on every call. The binding has to lose to the directory walk. A user standing in vault B who runs `mnemic new` expects the note in B. `git`, `jj`, and `cargo` all resolve from the current directory first. The default only answers which vault to use when the current directory is in no vault.

`check <path>` names what to check, so it must not fall through to an unrelated default. `init` creates a vault where it is told to. A global side effect there would rebind a developer's real default whenever a test ran `mnemic init` in a temp directory.

## Decision

Every vault verb except `init` resolves in this order: `--vault <path>` (exact root, no walk), the current directory and its parents, the default vault in the user config, then `VAULT_NOT_FOUND`. `check <path>` stops after the walk from that path. The CLI reads the user config only when the walk misses.

The file is `$XDG_CONFIG_HOME/mnemic/config.toml` when that variable is set, non-empty, and absolute, otherwise `$HOME/.config/mnemic/config.toml`, on Linux and macOS. It holds one key, `vault`, an absolute path. `mnemic use <path>` writes the canonical root. `mnemic use` with no argument writes nothing and reports the resolution. `mnemic use --clear` removes the binding and creates no file when none exists. A symlink `config.toml` stays a symlink: the replacement is written in the target's directory and renamed over the target.

Env reads and this file's IO stay in the `mnemic` binary. `mnemic-core` does not learn about user config.

`VAULT_NOT_FOUND` covers both "no vault and no default" and "the bound vault's `mnemic.toml` is gone". `error.details` carries `searched_from`, plus `default` and `config` when a default was tried.

Rejected: a `mnemic vault` noun (it invites subcommands), a `mnemic config` get/set/unset surface (one key does not need a key namespace), an `MNEMIC_VAULT` env var (a fourth tier, inherited by every child, and the same trade-off declined for `MNEMIC_ACTOR`), an `init --use` flag (a scaffold command must not change global state), a separate `DEFAULT_VAULT_NOT_FOUND` code (same exit, same fix, details already distinguish the cases), and the `dirs` crate (on macOS it would pick `~/Library/Application Support`).

## Consequences

`mnemic use ~/notes` once is enough for later commands outside a vault. Inside another vault, that vault still wins. Agents pass `--vault` or a `data.vault` they just read. They do not share one machine-wide default. A broken user config never blocks work inside a vault directory, because that file is read only at tier 3. Two concurrent `use` calls leave one of the two values. A crash leaves the old file or the new file.
