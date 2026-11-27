# Council

One question, two or more shadows, then a moderator synthesis. Each shadow answers alone. The moderator never blends their notes into one voice.

## Members

Use the shadows the user named. Resolve each name with `mnemic get` and keep the note only when `type` is `shadow`.

When the user names a domain `<d>` and not the people, build the member set from three searches. There is no `list` subcommand. `search` requires a non-empty query, matches title, aliases, and body only, and returns at most `--limit` hits (default 20). Pass `--limit 10000`. If `data.hits` has that many rows, the page is full: add 10000 to the limit and run the same search again. Stop when the row count is below the limit. A full page is not a complete member set.

Union the slugs. One shadow found twice is one member.

1. Shadows that carry the tag `domain/<d>` on frontmatter `tags`:

```bash
mnemic search shadow --type shadow --tag domain/<d> --limit 10000 --json
```

The query `shadow` matches every title that starts with `Shadow:`.

2. Shadows whose `domains` field lists `<d>`. Create stores domains there, not in `tags`. Search every shadow, then `get` each hit and keep it when `frontmatter.domains` contains `<d>` as one entry.

```bash
mnemic search shadow --type shadow --limit 10000 --json
```

```bash
mnemic get <shadow> --json
```

3. Sources tagged `domain/<d>`. Create puts `domain/<d>` and `shadow/<slug>` on the source, not on the shadow. The query `source` matches the built-in source body `A source.` `get` each hit. From `frontmatter.tags`, take each tag `shadow/<slug>` and keep `<slug>` only when `mnemic get shadow-<slug> --json` returns type `shadow`.

```bash
mnemic search source --type source --tag domain/<d> --limit 10000 --json
```

```bash
mnemic get <source> --json
```

Fewer than two distinct shadows: tell the user the council needs at least two shadows. Do not run the isolated passes. Do not write a note, a timeline line, or a commit.

## Isolated passes

One pass per member. When the harness can start a subagent, each pass is its own subagent. A pass receives the question, that shadow's slug, and nothing about the other passes.

The pass may open:

- `shadow-<slug>`
- source notes that match the consult search for that slug

It may not open another shadow, another pass's notes, or the rest of the vault. It follows the consult citation rule: every claim ends with a wikilink to a source note that pass opened, or the sentence starts with `Extrapolation:`.

## Moderator output

Write these sections, in this order, and no others.

### Question

The question, unchanged.

### Positions

One subsection per shadow, titled with the person's name. Under it, that pass's answer. Keep its wikilinks and its `Extrapolation:` sentences. Do not add a citation the pass did not open.

### Agreements

Claims two or more positions share, restated in the moderator's words. Point at the shadows with `[[shadow-<slug>]]`. Do not invent a shared claim.

### Disagreements

Claims where positions conflict. Name each shadow with a wikilink and the claim it holds. A difference of emphasis is not a disagreement.

### Cruxes

The narrow facts or distinctions the disagreement turns on. One crux is one sentence. If nothing turns the disagreement, write `No crux yet.`

### What would move each shadow

One line per member: `[[shadow-<slug>]]` and the evidence that would change that position. Take it from what that pass treated as a test or a gap. Do not score or vote.

### Extrapolation flags

List every `Extrapolation:` sentence from the positions, attributed with `[[shadow-<slug>]]`. When there are none, write `None.`

## Saving

Show the moderator output and stop. The council is not a vault note until the user asks to save it.

When they ask:

1. `mnemic new note "Council: <question>" --as <actor> --field tags=council --json`. Retry exit 75 with the skill retry rule. Any other failure stops the save. Write no timeline line.
2. Replace the new note's body with the moderator output. Add `[[shadow-<slug>]]` for every member and `[[<source-slug>]]` for every source the positions cited, if a section does not already contain that link. Leave frontmatter to the `new` command.
3. The council slug is the filename stem of `data.path` (for a title `Council: Is the engine unfinished`, the slug is `council-is-the-engine-unfinished`).
4. For each member, `mnemic log shadow-<slug> "<date> | [[<council-slug>]] | council: <question>" --json`. `<date>` is the local calendar date. If the question contains `|`, replace each `|` with a space in the log summary only. The question inside the note body stays unchanged.
5. `mnemic check --json`. Exit 0 is required before the commit. The subject is `mnemic(<actor>): shadow council <council-slug>`.

Do not save on a retry of the same question. Do not save because the output looks finished. The user has to ask.

## Template

```markdown
## Question

<question>

## Positions

### <Name>

<pass, with [[source-slug]] citations or Extrapolation: sentences>

## Agreements

- <shared claim> ([[shadow-<slug>]], [[shadow-<slug>]])

## Disagreements

- [[shadow-<slug>]] holds <claim>. [[shadow-<slug>]] holds <claim>.

## Cruxes

- <one fact or distinction>

## What would move each shadow

- [[shadow-<slug>]]: <evidence>

## Extrapolation flags

- [[shadow-<slug>]]: <sentence>
```
