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

A vault is the directory that holds `mnemic.toml`, found at the current directory or a parent, unless you pass `--vault <dir>`. `init` does not walk to a parent. Git must be available. The CLI does not commit or push.

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

Synopsis: `mnemic new <type> <title> --id <ulid> --as <actor> --date <date> --path <path> --url <url> --field <key=value> --json --vault <path>`

Example: `mnemic new note "Probe note" --as sylver --json --vault .`

Pass `--as` on every `new`. `--date` sets journal `date`. A `source` note needs a non-empty `--path` or `--url` (or both). Other types reject those flags.

Success `data`: `id`, `path`, `noop`. `path` is vault-relative. `noop` is true only when that id is already the note at that path.

Failures: `UNKNOWN_TYPE`, `BAD_ID`, `ID_EXISTS` (that id is already another path), `SLUG_COLLISION` (the new slug equals another slug or alias), `MISSING_TEMPLATE`, `USAGE` (empty slug, missing author, locator flags on a non-source, a source with no locator, a bad `--field`, or a missing required field), `OUTSIDE_VAULT`, `IO`, `LOCK_HELD`.

`--field key=value` sets a declared field, `state`, `tags`, or `title` when the note is created. Repeat it to build a list. Do not combine `--path` with `--field path=`, `--url` with `--field url=`, or `--date` with `--field date=`.

### set

Set declared frontmatter fields on one note. Do not hand-edit YAML.

Synopsis: `mnemic set <note> --field <key=value> --add <key=value> --json --vault <path>`

Example: `mnemic set ada --field email=ada@example.com --field state=current --json --vault .`

Pass at least one `--field` or `--add`. `--field` sets a declared field, `state`, `tags`, or `title`. `--add` appends one `strings` or `tags` item. A repeated `--field` builds a list. Do not pass `--field` and `--add` for the same key.

Success `data`: `id`, `path`, `noop`, `changed`. `changed` lists keys whose bytes changed, in argument order, without `updated`. `noop` is true when nothing changed. The file is not written and `updated` is not stamped.

Failures: `USAGE` (bad key, kind, value, or state; a multi-line existing value; no frontmatter; `sources/raw/`), `NOT_FOUND`, `AMBIGUOUS` with `error.details.candidates`, `OUTSIDE_VAULT`, `UNKNOWN_TYPE`, `VAULT_NOT_FOUND`, `IO`, `LOCK_HELD`.

### mv

Move or rename a note and rewrite incoming wikilinks. Use this command. Do not use `git mv`.

Synopsis: `mnemic mv <note> <dest> --no-rewrite --json --vault <path>`

Example: `mnemic mv notes/scratch.md notes/scratch-moved.md --json --vault .`

Success `data`: `id`, `from`, `to`, `alias_added`, `rewritten`. `--no-rewrite` leaves wikilinks unchanged, so `rewritten` is empty.

Failures: `USAGE`, `NOT_FOUND`, `AMBIGUOUS` with `error.details.candidates`, `OUTSIDE_VAULT`, `DEST_EXISTS`, `NOTE_ARCHIVED`, `ALIAS_NOT_UNIQUE`, `SLUG_COLLISION`, `RAW_IMMUTABLE` (destination under `sources/raw`), `IO`, `LOCK_HELD`.

### skill

Print the embedded agent skill, or one reference file. This command does not open a vault.

Synopsis: `mnemic skill [--reference <contract|types>] [--json]`

Example: `mnemic skill --json`

No `--reference` prints `SKILL.md`. `--reference contract` prints `references/contract.md`. `--reference types` prints `references/types.md`.

Success `data`: `name`, `version`, `content`. `version` matches `mnemic --version`.

Failures: `USAGE` when `--reference` is not `contract` or `types`.

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

Custom types are vault files at `types/<name>.toml`, with an optional body at `types/<name>.md`. After `mnemic check` accepts the pack, `mnemic new <name>` creates a note of that type. Steps, field checks, and shipping types from a skill are in [docs/format.md](../../../../docs/format.md).

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
