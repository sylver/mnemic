---
name: mnemic
description: >-
  Read, search, create, update, move, and validate notes in a mnemic vault
  (a Git repo of Markdown notes with mnemic.toml at its root) using the mnemic
  CLI. Use whenever you store or recall lasting knowledge in a vault, or the
  repo you are in has mnemic.toml.
license: MIT
compatibility: Requires the mnemic CLI on PATH (macOS arm64 or Linux x86_64) and git.
metadata:
  mnemic-version: "0.1.1" # x-release-please-version
---

# mnemic

Use the `mnemic` CLI for a vault: a Git repo of Markdown notes with `mnemic.toml` at its root. Pass `--json` on every call. Read `ok`, then `error.code`. Codes, exits, and success `data` fields are in [references/contract.md](references/contract.md). Core types are in [references/types.md](references/types.md).

## Preflight

Run `mnemic --version`. The printed version matches `metadata.mnemic-version` in this skill.

If `mnemic` is missing, install the GitHub release binary and check its SHA-256 before putting it on `PATH`. `ver` is the version above without a leading `v`.

Linux x86_64:

```bash
ver=0.1.1
asset=mnemic-x86_64-unknown-linux-musl
curl -fsSL -O "https://github.com/sylver/mnemic/releases/download/v${ver}/${asset}"
curl -fsSL -O "https://github.com/sylver/mnemic/releases/download/v${ver}/${asset}.sha256"
sha256sum -c "${asset}.sha256"
install -m 755 "$asset" "$HOME/.local/bin/mnemic"
```

macOS arm64 uses `asset=mnemic-aarch64-apple-darwin` and `shasum -a 256 -c "${asset}.sha256"`.

A vault is the directory that holds `mnemic.toml`. Resolution order is `--vault <dir>` (exact, no walk), then the current directory and its parents, then the default vault from `mnemic use`. `check <path>` walks from that path and does not use the default. `init` does not walk to a parent and does not use the default. Pass `--vault <root>` on every call, or run `mnemic use --json` once and pass `data.vault`. Do not rely on the shared default: a bot fleet on one machine must not follow whichever vault the last `use` bound. Git must be available. The CLI does not commit or push.

## Envelope handling

Always pass `--json`. One JSON document is the whole result.

- `ok: true` means success. Read `data` and stop. Exit 0.
- `ok: false` and `error.code` `CHECK_FAILED` means findings. Read `data.errors[]` (and `data.warnings`). Exit 2. The command ran.
- Any other `error.code` is fatal. `data` is null. Do not look for findings.

`IO` exits 1. `LOCK_HELD` exits 75. Every other code exits 2. The code list is in [references/contract.md](references/contract.md).

## Retry rule

Exit 75 is `LOCK_HELD`. The vault write lock is held by another local process. Retry the same command with backoff, about three attempts inside 30 seconds (for example after 2s, then 8s, then 16s). If it still fails, report the holder from `error.details.pid` and `error.details.since` and quote `error.message`.

Never delete `.mnemic/write.lock`. Never retry exit 1 or exit 2. Those are `IO` and usage or findings, not a busy lock.

## Verb guide

Each entry is one subcommand. Flags below are the long flags that subcommand accepts. Success `data` keys match [references/contract.md](references/contract.md).

### check

Check core frontmatter in a vault or one note.

Synopsis: `mnemic check [PATH] --json --vault <path> --fix`

Example: `mnemic check --json --vault .`

Success `data`: `errors`, `warnings`, and `changes` when `--fix` ran.

Failures: `CHECK_FAILED` with `data.errors`. Fatal: `VAULT_NOT_FOUND`, `BAD_CONFIG`, `UNSUPPORTED_VAULT_VERSION`, `USAGE`, `OUTSIDE_VAULT`, `IO`, `LOCK_HELD`. `--fix` inserts only a missing `id` or a missing `type`.

### init

Scaffold a vault in the target directory.

Synopsis: `mnemic init --as <actor> --json --vault <path>`

Example: `mnemic init --as sylver --json`

Success `data`: `vault`, `created`, `skipped`.

Failures: `USAGE`, `BAD_CONFIG`, `UNSUPPORTED_VAULT_VERSION`, `NESTED_VAULT` (a parent already has `mnemic.toml`), `IO`, `LOCK_HELD`. Pass `--as` on every `init`. A new vault gets a static root `README.md` when that file is absent. An existing `README.md` is left in place and listed in `data.skipped`. `init` does not write a vault `AGENTS.md`.

### log

Append one timeline line.

Synopsis: `mnemic log <note> <text> --json --vault <path>`

Example: `mnemic log welcome "2026-11-27 | skill | summary" --json --vault .`

`<note>` is a path, id, slug, or alias. `<text>` is `YYYY-MM-DD | source | summary` or `YYYY-MM-DD | summary`.

Success `data`: `id`, `path`, `noop`, `line`, `heading_created`.

Failures: `NOT_FOUND`, `AMBIGUOUS` (`error.details.candidates`), `USAGE`, `TIMELINE_DISABLED`, `TIMELINE_HEADING_AMBIGUOUS`, `OUTSIDE_VAULT`, `IO`, `LOCK_HELD`.

### search

Search parsed notes. Tokens are literal and combined with AND.

Synopsis: `mnemic search [QUERY] --type <type> --tag <tag> --since <date> --limit <n> --json --vault <path>`

Example: `mnemic search Ada --type person --since 2020-01-01 --limit 5 --json --vault .`

Success `data`: `query`, `hits[]` with `id`, `path`, `title`, `type`, `snippet`, `section`, `score`. Zero hits is still `ok: true`.

Failures: `USAGE` (empty query, or `--limit` of zero or less), `VAULT_NOT_FOUND`, `IO`.

### get

Print one note by id, slug, alias, or vault-relative path.

Synopsis: `mnemic get <query> --json --vault <path>`

Example: `mnemic get ada --json --vault .`

Success `data`: `id`, `path`, `slug`, `title`, `type`, `status`, `frontmatter`, `body`, `outgoing`, `backlinks`.

Failures: `NOT_FOUND`, `AMBIGUOUS` with `error.details.candidates`, `OUTSIDE_VAULT`, `VAULT_NOT_FOUND`, `IO`.

### links

List outgoing links, backlinks, and broken links for one note.

Synopsis: `mnemic links <note> --json --vault <path>`

Example: `mnemic links ada --json --vault .`

Success `data`: `id`, `path`, `outgoing`, `backlinks`, `broken`. A non-empty `broken` list still exits 0. `check` is what fails on a broken link.

Failures: `NOT_FOUND`, `AMBIGUOUS` with `error.details.candidates`, `OUTSIDE_VAULT`, `VAULT_NOT_FOUND`, `IO`.

### new

Create a note from a type template.

Synopsis: `mnemic new <type> <title> --id <ulid> --as <actor> --date <date> --json --vault <path>`

Example: `mnemic new note "Probe note" --as sylver --json --vault .`

Pass `--as` on every `new`. `--date` sets journal `date`.

Success `data`: `id`, `path`, `noop`. `path` is vault-relative. `noop` is true only when that id is already the note at that path.

Failures: `UNKNOWN_TYPE`, `BAD_ID`, `ID_EXISTS` (that id is already another path), `SLUG_COLLISION` (the new slug equals another slug or alias), `MISSING_TEMPLATE`, `USAGE` (empty slug or missing author), `OUTSIDE_VAULT`, `IO`, `LOCK_HELD`.

### mv

Move or rename a note and rewrite incoming wikilinks. Use this command. Do not use `git mv`.

Synopsis: `mnemic mv <note> <dest> --no-rewrite --json --vault <path>`

Example: `mnemic mv notes/scratch.md notes/scratch-moved.md --json --vault .`

Success `data`: `id`, `from`, `to`, `alias_added`, `rewritten`. `--no-rewrite` leaves wikilinks unchanged, so `rewritten` is empty.

Failures: `USAGE`, `NOT_FOUND`, `AMBIGUOUS` with `error.details.candidates`, `OUTSIDE_VAULT`, `DEST_EXISTS`, `NOTE_ARCHIVED`, `ALIAS_NOT_UNIQUE`, `SLUG_COLLISION`, `RAW_IMMUTABLE` (destination under `sources/raw`), `IO`, `LOCK_HELD`.

### use

Show or set the default vault. This binding is for a person at a shell. Agents pass `--vault` or the `data.vault` from one `mnemic use --json` call.

Synopsis: `mnemic use [path] --clear --json`

Example: `mnemic use --json`

Success `data`: `vault`, `source` (`cwd`, `default`, or null), `default`, `config`, `changed`. The show form exits 0 when nothing resolves.

Failures: `VAULT_NOT_FOUND` (no vault at or above `<path>`), `BAD_CONFIG`, `UNSUPPORTED_VAULT_VERSION`, `USAGE` (`<path>` with `--clear`, a non-UTF-8 path, or no `HOME` and no `XDG_CONFIG_HOME` on set or clear), `IO`.

## Workflows

Capture: `search` first. If a note already holds the fact, `log` the event or edit that note's compiled truth. Otherwise `new`.

Record an event: `log <note> "YYYY-MM-DD | source | summary"`.

Rename or refile: `mv`. Never `git mv`.

Validate: `check` before every commit. `check --fix` only to insert a missing `id` or a missing `type`. It does not invent other fields.

Manual body edit: change compiled truth only. Never rewrite existing timeline lines. Never touch `sources/raw/`. Append timeline lines with `log`.

## Actor and fleet

Pass `--as <actor>` on every `new` and every `init`. There is no actor environment variable. Each bot keeps one stable actor id.

The write lock is local to one machine. Writers on other machines coordinate through Git, not through the lock file.

The CLI does not commit or push. After a successful write, commit with subject `mnemic(<actor>): <verb> <vault-relative path or summary>`.

## Vault rules

Search before creating, and update the existing entity when the search hits.

One file per entity. Put it in that type's folder. When the folder is unclear, use `inbox/`.

On a state change, rewrite the compiled truth and append a timeline line. Do not rewrite older timeline lines.

Never edit `sources/raw/`.

Prefer `mnemic` with `--json` over hand-written YAML frontmatter.

The CLI does not commit or push. Write human corrections into the vault immediately, then commit them yourself.

## Conflicts

On a rebase conflict in a note, keep both sides' timeline lines and sort them by date. Keep the newer compiled truth. Then run `check`.

## Types

One line per built-in type. Folders, fields, timeline, and states are generated in [references/types.md](references/types.md). A vault overlay at `<vault>/types/<type>.toml` replaces that type whole. `<vault>/types/<type>.md` replaces its body template.

- `decision` lives in `decisions/`, timeline on, states `proposed`, `accepted`, `superseded`.
- `idea` lives in `ideas/`, timeline off, states `open`, `parked`, `adopted`.
- `journal` lives in `journal/`, timeline on, states `open`, `closed`, required `date`.
- `note` lives in `inbox/` or `notes/`, timeline off, states `open`, `filed`.
- `org` lives in `orgs/`, timeline off, states `active`, `dissolved`.
- `person` lives in `people/`, timeline off, states `current`, `former`.
- `place` lives in `places/`, timeline off, states `current`, `former`.
- `project` lives in `projects/`, timeline off, states `active`, `paused`, `done`.
- `source` lives in `sources/`, timeline off, states `unread`, `read`, `archived`.
- `topic` lives in `topics/`, timeline off, states `active`, `archived`.
