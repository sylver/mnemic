# Ingest

An item is stored only when all four of these exist:

- a stable id from the source (a video id, or a post id inside a day's file)
- a time (publish date, or the UTC day for a batch)
- a canonical URL
- the verbatim text in a raw file

A search summary is not an item. Do not write a raw file for it, do not create a source note, and do not move the cursor because of it.

## Titles

The title is the identity. `new` slugs it, and a later run `get`s that slug so it does not create a second note.

- YouTube: `<locator> <YYYY-MM-DD> <videoId>`. Example: `yt:grandAngle 2026-11-20 dQw4w9WgXcQ`. The date is in the title so two ids that differ only by case do not share a slug.
- X: `<locator> <YYYY-MM-DD>` for one completed UTC day. Example: `x:EtherRawl 2026-11-26`.
- Any other kind: `<locator> <YYYY-MM-DD> <stable-id>` when the kind has an item id, otherwise `<locator> <YYYY-MM-DD>`.

`<locator>` is the channel's `kind:handle`, with the handle's original case.

## Raw layout

Root: `sources/raw/shadows/<slug>/`. `<slug>` is the person's name slug, not the handle and not `shadow-<slug>`.

- YouTube: `sources/raw/shadows/<slug>/yt-<handle-slug>/<videoId>.txt`. The file is the transcript as plain UTF-8 text. `<handle-slug>` is the handle lowercased, with characters other than letters and digits turned into dashes.
- X: `sources/raw/shadows/<slug>/x-<handle-slug>/<YYYY-MM-DD>.jsonl`. One JSON object per line. Each object has `id`, `time`, `url`, and `text`.
- Other kinds: the same folder shape, `sources/raw/shadows/<slug>/<kind>-<handle-slug>/<stable-id>.txt`, plain UTF-8.

Plain UTF-8 text and JSONL only. Never video, audio, thumbnails, images, or raw HTML. After the file is written, do not change it. `check` compares `sha256` on the source note to these bytes.

Write the raw file first. Then record its vault-relative path and the hex sha256 on the source note.

## Cursor and gaps

The channel `cursor` is the id of the newest item that is fully stored (raw file and source note). For X that id is the `YYYY-MM-DD` of the last completed day stored. For YouTube it is the video id.

- Move `cursor` only after those items are stored.
- On create, the window is about 12 months, and it stops at the last completed UTC day.
- On refresh, fetch items newer than `cursor`.
- A failed page is a gap. Log `YYYY-MM-DD | <locator> | gap <reason>` on the channel. Leave `cursor` at the last stored item. Do not skip the hole on a later run without saying so in a new gap line.
- A finished fetch logs `YYYY-MM-DD | <locator> | stored N items, cursor <id>`. `N` is how many source notes this run created. Zero is allowed when nothing new was published.
- `last_run_at` is the local date `YYYY-MM-DD` when the run stored items or recorded a clean empty fetch. Do not stamp it on a run that blocked before any read.

When the read tool is missing, set the channel `state` to `blocked` and `blocked` to that tool's name. Do not invent items and do not move `cursor`.

## YouTube

Need a channel listing and a transcript tool. Fetch uploads in the window, newest video id first when the listing provides order.

For each video not already stored: write the transcript to the `.txt` path, then create the source note. Skip a video whose transcript tool returns nothing, and log a gap that names the video id, unless the tool itself is missing. A missing tool blocks the whole channel.

Store the transcript text only.

## X

Need a raw post reader that returns the post records, not a summary. Fetch only UTC days that have already ended. The current UTC day is never fetched, so the day's JSONL is complete the first time it is written and is not edited later.

One source note per account per completed day. The JSONL holds that day's posts. The source note URL is the account's day or profile URL you actually fetched. The cursor is that date.

If the reader is missing, block the channel and name the reader. Do not fill the day from search snippets.
