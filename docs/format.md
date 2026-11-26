# Note format

This is the v1 note format. `mnemic check` reads it. The JSON envelope is in [the check contract](contract/check.md).

## Vault root

A vault is a directory with `mnemic.toml`. The CLI walks from the path you pass, then from its parents, until it finds that file. `--vault` selects the root and does not walk.

`mnemic.toml` has two keys:

```toml
schema_version = 1
actor = "sylver"
```

`schema_version` is an integer. This CLI accepts `1`. A higher value is refused. `actor` is a non-empty name or bot id.

## Note files

A note is a UTF-8 file whose name ends in `.md`, with these exceptions:

- The file name is `README.md` or `AGENTS.md`.
- Any path component, including the file name, starts with `.`. That covers `.mnemic/` and atomic-write temps such as `.welcome.mnemic-tmp-1`.
- The path is under `sources/raw/`. Those files are immutable raw sources. Tools read them and do not rewrite them.

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

Text after the closing fence is the body. A `## Timeline` section is the place for later events. `check` does not read the body.

A wikilink is `[[ULID]]` or `[[ULID|label]]`. This version does not resolve links.

## Type pack

The core pack is compiled into the CLI. A vault cannot add or replace types. Each type is one TOML file with this shape:

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

## Frontmatter edits

`FrontmatterEdit` in `mnemic-core` inserts a top-level key, replaces a top-level key's scalar, or prepends a block with `create_block`. Insert and replace match column-0 keys of the root mapping. A nested key of the same name is left alone. Replace refuses a value that spans lines (a `|` or `>` block scalar, an indented continuation, or a flow or block collection) with `MultilineValue` and does not rewrite that block. It copies every byte it was not asked to change, including comments, key order, quoting, and the body. A `#` inside a quoted scalar is part of the value. A trailing comment outside quotes stays. `create_block` writes `---\n<keys>\n---\n` in front of a file that has no frontmatter and leaves the rest of the file untouched. The value text is the caller's YAML scalar. The editor does not invent quotes. It does not write files. A later verb passes the bytes to `WriteTxn::atomic_write`. A brand-new file may be written as a fresh block without this editor.
