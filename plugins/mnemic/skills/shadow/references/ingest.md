# Ingest

The caller supplies items. This file is how those items are stored. The skill does not fetch.

An item is stored only when all four of these exist:

- a stable id (an item id, or the day for a day-batched feed)
- a time
- a canonical URL
- the verbatim content in a raw file

A search summary is not an item. Do not write a raw file for it, do not create a source note, and do not move the cursor because of it.

## Titles

The title is the identity. `new` slugs it, and a later run `get`s that slug so it does not create a second note.

- An item with its own id: `<locator> <YYYY-MM-DD> <itemId>`.
- A day batch: `<locator> <YYYY-MM-DD>`.

The date is in the title so two ids that differ only by case do not share a slug.

`<locator>` is the channel's `kind:handle`, with the handle's original case.

## Raw layout

One file per stored item, under the channel folder:

- `sources/raw/shadows/<slug>/<channel-slug>/<itemId>.txt`
- `sources/raw/shadows/<slug>/<channel-slug>/<YYYY-MM-DD>.jsonl`

`<slug>` is the person's name slug, not the handle and not `shadow-<slug>`. `<channel-slug>` is the channel note's slug.

A `.txt` file is the verbatim content as plain UTF-8. A `.jsonl` file is one JSON object per line. Each object has `id`, `time`, `url`, and `text`. Write a day batch only for a day the caller reports complete, so that file is never edited.

Plain UTF-8 text and JSONL only. Never video, audio, thumbnails, images, or raw HTML. After the file is written, do not change it. `check` compares `sha256` on the source note to these bytes.

Write the raw file first. Then record its vault-relative path and the hex sha256 on the source note.

## Cursor and gaps

The channel `cursor` is an opaque string the caller returns for the newest stored item. Store it after those items are stored. Hand that same string back on the next refresh. Never parse it.

- Move `cursor` only after those items are stored.
- On create and on add channel, the window is about 12 months, and it stops at the last completed day.
- On refresh, ask the caller for items newer than `cursor`.
- A failed page is a gap. Log `YYYY-MM-DD | <locator> | gap <reason>` on the channel. Leave `cursor` at the last stored item. Do not skip the hole on a later run without saying so in a new gap line.
- A finished read logs `YYYY-MM-DD | <locator> | stored N items, cursor <id>`. `N` is how many source notes this run created. Zero is allowed when nothing new was published.
- `last_run_at` is the local date `YYYY-MM-DD` when the run stored items or recorded a clean empty read. Do not stamp it on a run that blocked before any read.

When the caller cannot reach the channel, set `state` to `blocked` and `blocked` to a reason that names what is missing (a tool, auth, access, or a paywall). Do not invent items and do not move `cursor`.

## Examples

These are storage shapes. None of them names a tool the caller must install.

- A video transcript, one item: title `<locator> <YYYY-MM-DD> <itemId>`, file `sources/raw/shadows/<slug>/<channel-slug>/<itemId>.txt`.
- A day of posts, one batch: title `<locator> <YYYY-MM-DD>`, file `sources/raw/shadows/<slug>/<channel-slug>/<YYYY-MM-DD>.jsonl`.
- A Patreon post, one item: the same `<itemId>.txt` shape.
- A podcast transcript, one item: the same `<itemId>.txt` shape.
