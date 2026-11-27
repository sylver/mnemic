# Note format

This is the v1 note format. `mnemic check` reads it. The JSON envelope is in [the check contract](contract/check.md).

## Vault root

A vault is a directory with `mnemic.toml`. Vault verbs other than `init` resolve the root in this order:

1. `--vault <path>` is that directory and does not walk.
2. The current directory and its parents. The first directory that holds `mnemic.toml` wins.
3. The default vault recorded in the user config.
4. Otherwise the command is `VAULT_NOT_FOUND`.

`check <path>` walks from `<path>` and does not consult the default. `init` does not walk and does not consult the default.

The user config file is `$XDG_CONFIG_HOME/mnemic/config.toml` when `XDG_CONFIG_HOME` is set, non-empty, and absolute. Otherwise it is `$HOME/.config/mnemic/config.toml`. A relative or empty `XDG_CONFIG_HOME` is ignored. The file has one key, `vault`, and the value is an absolute path. `mnemic use <path>` writes the canonical root. `mnemic use --clear` removes the binding.

`mnemic.toml` has two keys:

```toml
schema_version = 1
actor = "sylver"
```

`schema_version` is an integer. This CLI accepts `1`. A higher value is refused. `actor` is a non-empty name or bot id. `mnemic check` requires `actor`. `mnemic init` writes `actor` only when `--as` is passed, and only when it creates `mnemic.toml`.

## Init

`mnemic init` scaffolds the target directory. `--vault` is that directory and does not walk to a parent. Without `--vault`, the target is the current directory, also with no walk.

The command writes `mnemic.toml`, `README.md` from the binary template, `.gitignore` containing `.mnemic/`, and one `.gitkeep` in each empty type folder from the core pack. It does not copy templates into the vault, and it does not run `git init`. It does not write `AGENTS.md`.

Existing files stay as they are. A second run adds only what is missing. An existing `.gitignore` gains a `.mnemic/` line when that line is absent. The rest of the file is left as it was. If `README.md` already exists at the vault root, `init` does not edit it. `--json` lists that path in `data.skipped` and the command exits 0.

`init` refuses a target whose parent already has `mnemic.toml` (`NESTED_VAULT`). An existing `mnemic.toml` that is malformed (`BAD_CONFIG`) or newer than this CLI (`UNSUPPORTED_VAULT_VERSION`) is left in place.

## Note files

A note is a UTF-8 file whose name ends in `.md`, with these exceptions:

- The file name is `README.md` or `AGENTS.md`.
- Any path component, including the file name, starts with `.`. That covers `.mnemic/` and atomic-write temps such as `.welcome.mnemic-tmp-1`.
- The path is under `sources/raw/`. Those files are immutable raw sources. Tools read them and do not rewrite them.

The note-file set is that walk. Each entry is `symlink_metadata` before `canonicalize_within`, and that check runs before `read_dir` or a file read. A directory symlink is not followed. A file symlink is in the set only when its canonical path is a regular file inside the vault. A FIFO, socket, device, or path `canonicalize_within` rejects is not a note. `check` does not report it, and the path is not opened.

Notes live under a folder named by the type pack. `check` does not compare the folder name to `type`.

## `check --fix`

`mnemic check --fix` inserts only what it can derive. A missing `id` becomes a new Crockford ULID. The time component comes from the process clock, not the file mtime. A missing `type` is set from the note's top-level folder when that folder equals exactly one type's `folder`. `folder` is a string or a list of strings. `people/friends/x.md` uses `people`. `inbox/` and `notes/` both derive `note`, because the `note` type lists both. A folder that maps to no type, or to more than one, leaves `type` missing.

A note with no frontmatter block gets a new `---` block that contains only those derived keys. The body bytes stay identical. An `id` that is present but not a ULID is reported and left in place. `title`, `created`, `updated`, `created_by`, `status`, `schema_version`, aliases, and the body are not invented. `updated` is not stamped.

The command considers the note-file set only. Root `AGENTS.md` and `README.md` are not fixed. One lock covers the vault. A second run writes nothing when the first run's inserts are still in place. An I/O error after a rename leaves the earlier files fixed. The next run finishes the rest.

## Frontmatter

A note starts with a YAML mapping between `---` fences. The mapping is the only document. Duplicate keys are rejected. The fence must close.

Required keys:

| Key | Rule |
| --- | --- |
| `id` | 26-character ULID, Crockford base32 |
| `type` | non-empty string |
| `schema_version` | integer greater than or equal to 1 |
| `title` | non-empty string |
| `created` | RFC 3339 datetime with an offset, including `Z` |
| `updated` | RFC 3339 datetime with an offset, including `Z` |
| `created_by` | non-empty actor name or bot id |
| `status` | `active`, `archived`, or `superseded` |

Unknown keys are kept and are not validated. `status` is not a per-type state field.

## Body

Text after the closing fence is the body. A `## Timeline` section is the place for later events. `check` reads the body only for wikilinks.

`mnemic log` appends one timeline item when the note's type has `timeline = true`. The line stored under the heading is a Markdown list item. The grammar is one of:

- `- YYYY-MM-DD | source | summary`
- `- YYYY-MM-DDTHH:MM:SS+01:00 | source | summary`
- `- YYYY-MM-DD | summary`

The date is an ISO calendar date or an RFC 3339 datetime with an offset. `source` may be a wikilink, a path, a URL, or a short actor name, and may be omitted when the line has a single `|`. `summary` is the rest of the line and must not contain `|`. The command accepts the same text with or without the leading `- `, and it always writes the marker.

Lines are inserted in date order, newest first, after the heading and one blank line. The same calendar date keeps insertion order, newest on top. An existing line is not rewritten. The same date, source, and summary is a no-op. A type with `timeline = false` is `TIMELINE_DISABLED` and does not gain a heading. More than one `## Timeline` heading outside a code fence is `TIMELINE_HEADING_AMBIGUOUS`. A heading inside a code fence is ignored. When `timeline = true` and the heading is missing, `log` creates `## Timeline` at the end of the note. `updated` changes only when a line is written, from the clock, through `FrontmatterEdit`. A path that canonicalizes outside the vault is `OUTSIDE_VAULT`.

## Identity

A note has one `id`. Two files with the same `id` are `DUPLICATE_ID`. `aliases` is a list of strings. The same alias on two ids, compared case-insensitively after NFC normalization, is `ALIAS_NOT_UNIQUE`. An alias is not an email address or a phone number (`ALIAS_CONTACT`).

`supersedes` is a string or a list of strings. Each entry is a link target. `status: superseded` with no other note whose `supersedes` resolves to this note is `SUPERSEDED_WITHOUT_INCOMING`. A cycle in those edges is `SUPERSEDES_CYCLE`. Backlinks are computed from these edges. They are not written into Markdown files.

## Resolver

`VaultIndex::resolve` in `mnemic-core` is the one wikilink resolver. `check` uses it.

`get`, `links`, and `log` pass the user note argument through `VaultIndex::resolve_argument` first. That check refuses `..`, an absolute path outside the vault, and a path whose canonical form leaves the vault, including a symlink to an outside file, with `OUTSIDE_VAULT` and exit 2. It uses `canonicalize_within` and does not open the target. A bare id, slug, or alias skips that refusal and uses the order below.

Resolution order:

1. Vault-relative path, with or without `.md`.
2. Exact `id`. A link to a ULID is valid.
3. Exact filename slug, the basename without `.md`.
4. Exact alias.

Slug and alias matching is case-insensitive after NFC normalization. The resolver does not guess. Two filename slugs such as `people/sam.md` and `orgs/sam.md` may both exist. A bare `[[sam]]` that hits more than one is `AMBIGUOUS`. `error.details.candidates` lists `path`, `link`, and `matches`. `[[people/sam]]` is a path first, then falls through the rest of the order.

A wikilink is `[[target]]`, `[[target|label]]`, `[[target#heading]]`, or an embed `![[target]]`. `[[target#heading]]` does not require the heading to exist. `![[image.png]]` and any link to a non-`.md` file are valid when that file exists, and `BROKEN_LINK` when it does not. Wikilinks inside fenced code or inline code are not links. A `[` preceded by an odd number of backslashes does not open a wikilink, so `\[\[x]]` is not a link.

Typed frontmatter fields with a pack `target`, and `supersedes`, are edges through the same resolver. A target that does not resolve is `BROKEN_LINK`.

## Raw bytes

A `source` note may set `path` to a vault-relative raw file and `sha256` to the hex digest of that file. When both are set, `check` hashes the file. A mismatch, a missing file, or a path that is not a regular file inside the vault is `SHA256_DRIFT`. `check` does not read a file outside the vault. A symlink is hashed only when its canonical path is a regular file inside the vault. `sha256` is not required for a source note to pass.

## Type pack

The core pack is compiled into the CLI. Each type is one TOML file with this shape:

```toml
schema_version = 1
folder = "people"
timeline = false

[state]
values = ["current", "former"]

[[field]]
name = "org"
type = "string"
required = false
target = "org"
```

`folder` is a string or a list of strings. `note` uses `["inbox", "notes"]`. `schema_version` is the highest note `schema_version` this type accepts. A higher note value is `SCHEMA_VERSION_AHEAD`. `timeline` says the body may carry a `## Timeline` section. `state.values` is that type's `state` enum. `state` is not `status`.

`field.type` is `string`, `strings`, or `date`. `required` marks a field `check` rejects with `MISSING_FIELD` when it is absent. `target` names another type a link field points at.

A `source` note must have `path` or `url` (`MISSING_FIELD` when both are missing). `source` also has `sha256`, the hex digest of the raw bytes. Drift checks compare that field. It is not required for `check` to pass. A `journal` note must have `date`. An email or phone number in `aliases` is `ALIAS_CONTACT`. A `type` outside the pack is `UNKNOWN_TYPE`. A pack file that is not valid TOML, or that breaks this schema, is `INVALID_PACK`.

The ten types are `person`, `org`, `place`, `project`, `idea`, `decision`, `topic`, `source`, `journal`, and `note`.

## Templates

Each core type has one body template compiled into the CLI, next to its TOML. The template is Markdown only. It has no placeholders and no template engine. `new` writes frontmatter itself and appends this body. `init` does not copy templates into the vault. `check` does not read the body and does not require a template.

A file `<vault>/types/<type>.md` replaces the embedded body for that type. A type with neither file is a missing template. That is `MISSING_TEMPLATE` when `new` writes the type. `check` does not report it.

## `mnemic new`

`mnemic new <type> <title>` creates one note. The filename slug is `deunicode` transliteration, then lowercase ASCII, with spaces and punctuation turned into `-`, repeated dashes collapsed, and dashes trimmed. A title that slugs to empty is `USAGE`.

The file lands in the type's pack folder. A `note` is unfiled, so it lands in `inbox/`. A filed catch-all `note` lives in `notes/`. A `journal` note lands at `journal/YYYY/YYYY-MM-DD.md`. A second journal on that same day lands at `journal/YYYY/YYYY-MM-DD-slug.md`. With no `--date`, `date` is the process local calendar day as `YYYY-MM-DD`, with no time and no offset. `--date` stores that ISO date the same way.

`id` is a Crockford ULID from the seedable generator, or `--id` when that flag is itself a Crockford ULID. Anything else is `BAD_ID`. `created` and `updated` are the same RFC 3339 timestamp from the clock. `created_by` is `--as` or `actor` in `mnemic.toml`. With neither, `new` is `USAGE` and the message names both `--as` and `actor`. `status` is `active`. `new` does not write `visibility`.

The same `--id` at the same path is a success no-op and does not rewrite the file. The same `--id` at a different path is `ID_EXISTS`. The new filename slug must not equal any other note's slug or alias, compared case-insensitively after NFC. That is `SLUG_COLLISION`, including a slug in another folder. Notes that already share a slug stay. `new` does not add another. The create is `O_EXCL` under the vault write lock. A destination that canonicalizes outside the vault is `OUTSIDE_VAULT` and is not written. `sources/raw/` is not a create destination.

## Vault overlays

`<vault>/types/*.toml` is the only overlay. The file stem is the type name, and that name is one path component. `check` loads that directory from the vault root at the start of a run. Files there are pack files. `check` does not treat them as notes. A symlink at `types/` or at a pack or template file, and a path that canonicalizes outside the vault, is `INVALID_PACK`. The finding names the vault-relative path and does not include the other file's text.

A new name adds a custom type. The same name replaces the built-in declaration whole, including `folder`, `schema_version`, `state`, and `field`. There is no field-by-field merge. The overlay's `schema_version` is the version `SCHEMA_VERSION_AHEAD` uses for that type, including when it is lower than the built-in it replaced. A lower version is not `INVALID_PACK`.

`folder` may change. Two types that claim the same folder string are `INVALID_PACK`, because a later `--fix` could not choose a folder. Malformed overlay TOML is `INVALID_PACK` and the process exits 2. There is no `[packs]` table.

## Frontmatter edits

`FrontmatterEdit` in `mnemic-core` inserts a top-level key, replaces a top-level key's scalar, appends one list item with `append_sequence_item`, or prepends a block with `create_block`. Insert and replace match column-0 keys of the root mapping. A nested key of the same name is left alone. Replace refuses a value that spans lines (a `|` or `>` block scalar, an indented continuation, or a flow or block collection) with `MultilineValue` and does not rewrite that block. It copies every byte it was not asked to change, including comments, key order, quoting, and the body. A `#` inside a quoted scalar is part of the value. A trailing comment outside quotes stays. `create_block` writes `---\n<keys>\n---\n` in front of a file that has no frontmatter and leaves the rest of the file untouched. The value text is the caller's YAML scalar. The editor does not invent quotes. It does not write files. A later verb passes the bytes to `WriteTxn::atomic_write`. A brand-new file may be written as a fresh block without this editor.

## Search

`mnemic search` scans the parsed note-file set from `VaultIndex`. It does not build an index, and it has no `MATCH` expression. The query is whitespace-separated tokens. Every token must match. Case folding is Unicode lowercase. Diacritics stay distinct. `-`, `"`, and `:` are characters in a token.

The scanned text is `title`, `aliases`, and the body, including a `## Timeline` section. Other frontmatter is not scanned. `--tag` reads the `tags` list only. A tag matches when it equals the flag or continues it with `/`, so `work` matches `work/x` and does not match `workspace`. Inline `#tag` is ignored. `--type` is the exact `type` string. `--since` compares `updated` and includes that ISO date or datetime. A date starts at midnight UTC.

Hits with a token in the title come first, then hits with more token occurrences, then a later `updated`, then `id`. `score` is that position starting at 1. The default limit is 20. `section` is `truth` before `## Timeline` and `timeline` from that heading on. `snippet` is the first matching title, alias, or line, at most 200 characters.

Deleting a note removes it from the next search. Root `AGENTS.md` and `README.md` are not notes, so a phrase that exists only there is not a hit. An entry that is not a regular file inside the vault is not a note, so it is not a hit and search does not error. The command does not take `--visibility`.

## Read verbs

`mnemic get <id|slug|alias|path>` prints one note. `mnemic links <note>` prints outgoing links, backlinks, and broken links. Both call `VaultIndex::resolve_argument` and `VaultIndex::link_sets`. They do not write files. A path that leaves the vault, including a symlink whose canonical path is outside, is `OUTSIDE_VAULT` and the file is not read. `log` uses the same argument check. `links` exits 0 when `broken` is non-empty. `check` remains the failing validator. An ambiguous slug or alias is `AMBIGUOUS` with `error.details.candidates`. A miss is `NOT_FOUND` for an id and for a slug.

## Move

`mnemic mv <note> <dest>` keeps the note `id`. It resolves `<note>` with `VaultIndex::resolve_argument` inside `with_write_lock`. When the slug changes it appends the old slug to `aliases` and stamps `updated` from the injected clock, in one `FrontmatterEdit` write, before the rename. A folder-only move does not add an alias. The rename uses `renameat2(RENAME_NOREPLACE)` on Linux and `renamex_np(RENAME_EXCL)` on macOS. An existing destination is `DEST_EXISTS`. `status: archived` is `NOTE_ARCHIVED`. An old slug that matches another note's slug or alias is `ALIAS_NOT_UNIQUE` and the file stays put. A destination slug that matches another note's slug or alias is `SLUG_COLLISION`. The note is not compared with itself. `type` does not change when the destination folder changes.

Unless `--no-rewrite` is set, `mv` then rewrites incoming wikilinks one file at a time with `atomic_write`. The match is case-insensitive after NFC. Fenced and inline code stay. A link that names an existing alias, such as `[[Kept]]`, stays. `[[slug]]`, `[[slug|label]]`, `[[slug#heading]]`, `![[slug]]`, and a path-style `[[people/slug]]` replace the target slug. A path-style link also replaces the prefix. The label and heading stay. Self-links in the moved note are included. `--no-rewrite` still adds the alias and renames.

A destination that exists as a directory, or ends in `/`, keeps the source basename. Any other destination must end in `.md` or the command is `USAGE`. The path must stay inside the vault. `sources/raw/` is `RAW_IMMUTABLE`. `.mnemic/` is `USAGE`. A path outside the vault is `OUTSIDE_VAULT`. Missing parent directories are created. `mv` does not overwrite a file.

Every crash point leaves a vault where the old slug still resolves. A later `mv` of the same arguments, when the source path is already gone, resolves that basename as a slug or alias. If that note is already at the destination, `mv` finishes the rewrite and exits 0. If the basename resolves to a different path, to nothing, or to more than one note, the command returns `NOT_FOUND` or `AMBIGUOUS` and writes nothing. It does not move an unrelated note. `MNEMIC_MV_CRASH=after-alias` and `MNEMIC_MV_CRASH=after-rename` exit 97 at those two points so recovery tests can stop the process. The env read is compiled only for debug builds, which is how `cargo test` builds the binary. Release builds ignore it. It is not part of normal use.
