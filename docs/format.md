# Note format

This is the v1 note format. `mnemic check` reads it. The JSON envelope is in [the check contract](contract/check.md).

## Vault root

A vault is a directory with `mnemic.toml`. The CLI walks from the path you pass, then from its parents, until it finds that file. `--vault` selects the root and does not walk.

`mnemic.toml` has two keys:

```toml
schema_version = 1
actor = "sylver"
```

`schema_version` is an integer. This CLI accepts `1`. A higher value is refused. `actor` is a non-empty name or bot id. `mnemic check` requires `actor`. `mnemic init` writes `actor` only when `--as` is passed, and only when it creates `mnemic.toml`.

## Init

`mnemic init` scaffolds the target directory. `--vault` is that directory and does not walk to a parent. Without `--vault`, the target is the current directory, also with no walk.

The command writes `mnemic.toml`, `AGENTS.md` from the binary template, `.gitignore` containing `.mnemic/`, and one `.gitkeep` in each empty type folder from the core pack. It does not copy templates into the vault, and it does not run `git init`.

Existing files stay as they are. A second run adds only what is missing. An existing `.gitignore` gains a `.mnemic/` line when that line is absent. The rest of the file is left as it was. If `AGENTS.md` already exists at the vault root, `init` does not edit it. `--json` lists that path in `data.skipped` and the command exits 0.

`init` refuses a target whose parent already has `mnemic.toml` (`NESTED_VAULT`). An existing `mnemic.toml` that is malformed (`BAD_CONFIG`) or newer than this CLI (`UNSUPPORTED_VAULT_VERSION`) is left in place.

## Note files

A note is a UTF-8 file whose name ends in `.md`, with these exceptions:

- The file name is `README.md` or `AGENTS.md`.
- Any path component, including the file name, starts with `.`. That covers `.mnemic/` and atomic-write temps such as `.welcome.mnemic-tmp-1`.
- The path is under `sources/raw/`. Those files are immutable raw sources. Tools read them and do not rewrite them.

The note-file set is that walk. Each entry is `symlink_metadata` before `canonicalize_within`, and that check runs before `read_dir` or a file read. A directory symlink is not followed. A file symlink is in the set only when its canonical path is a regular file inside the vault. A FIFO, socket, device, or path `canonicalize_within` rejects is not a note. `check` does not report it, and the path is not opened.

Notes live under a folder named by the type pack. `check` does not compare the folder name to `type`.

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

`VaultIndex::resolve` in `mnemic-core` is the one wikilink resolver. `check` uses it. Later verbs call the same method.

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

## Vault overlays

`<vault>/types/*.toml` is the only overlay. The file stem is the type name, and that name is one path component. `check` loads that directory from the vault root at the start of a run. Files there are pack files. `check` does not treat them as notes. A symlink at `types/` or at a pack or template file, and a path that canonicalizes outside the vault, is `INVALID_PACK`. The finding names the vault-relative path and does not include the other file's text.

A new name adds a custom type. The same name replaces the built-in declaration whole, including `folder`, `schema_version`, `state`, and `field`. There is no field-by-field merge. The overlay's `schema_version` is the version `SCHEMA_VERSION_AHEAD` uses for that type, including when it is lower than the built-in it replaced. A lower version is not `INVALID_PACK`.

`folder` may change. Two types that claim the same folder string are `INVALID_PACK`, because a later `--fix` could not choose a folder. Malformed overlay TOML is `INVALID_PACK` and the process exits 2. There is no `[packs]` table.

## Frontmatter edits

`FrontmatterEdit` in `mnemic-core` inserts a top-level key, replaces a top-level key's scalar, or prepends a block with `create_block`. Insert and replace match column-0 keys of the root mapping. A nested key of the same name is left alone. Replace refuses a value that spans lines (a `|` or `>` block scalar, an indented continuation, or a flow or block collection) with `MultilineValue` and does not rewrite that block. It copies every byte it was not asked to change, including comments, key order, quoting, and the body. A `#` inside a quoted scalar is part of the value. A trailing comment outside quotes stays. `create_block` writes `---\n<keys>\n---\n` in front of a file that has no frontmatter and leaves the rest of the file untouched. The value text is the caller's YAML scalar. The editor does not invent quotes. It does not write files. A later verb passes the bytes to `WriteTxn::atomic_write`. A brand-new file may be written as a fresh block without this editor.
