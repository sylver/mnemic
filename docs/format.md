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

Notes live in a type folder. The folder name is the note's `type`. `check` does not compare the folder name to `type`.

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
