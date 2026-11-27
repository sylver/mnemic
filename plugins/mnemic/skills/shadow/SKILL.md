---
name: shadow
description: >-
  Build and use a shadow of one person in a mnemic vault. Install the shadow
  and channel types, create the shadow from named channels, refresh it from
  new items, consult it with citations, or answer in that person's voice. Use
  when the user asks for a shadow, a persona, or to keep someone's published
  thinking current in the vault.
license: MIT
compatibility: Requires the mnemic CLI and git. YouTube needs a transcript tool. X needs a raw post reader.
metadata:
  mnemic-version: "0.1.1" # x-release-please-version
---

# shadow

A shadow is one note that compiles how a person thinks, from what they publish. Channels are the places that publication comes from. Each stored item is a `source` note plus a raw text file. Claims cite those notes.

Follow the mnemic skill for the JSON envelope. Pass `--json` on every call. Pass `--as <actor>` on every `new`. Use one stable actor for the vault.

Read [references/profile.md](references/profile.md) before writing a profile. Read [references/ingest.md](references/ingest.md) before storing an item.

## Preflight

Run `mnemic --version`. It must match `metadata.mnemic-version` in this file. Git must be available. This skill does not commit by itself. After `check` exits 0, commit with subject `mnemic(<actor>): shadow <mode> <slug>`.

`<slug>` is the person's name run through the same slug rules as `new` (lowercase ASCII, punctuation collapsed to dashes). It never comes from a handle. The shadow note title is `Shadow: <Name>`, so its filename slug is `shadow-<slug>`.

## Retry rule

Exit 75 is `LOCK_HELD`. Retry the same command with backoff, about three attempts inside 30 seconds (for example after 2s, then 8s, then 16s). If it still fails, report `error.details.pid` and `error.details.since` and quote `error.message`. Never delete `.mnemic/write.lock`. Never retry exit 1 or exit 2.

## Locator rule

A channel locator is `kind:handle`.

- `kind` matches `[a-z0-9-]+`. It is already lowercase. `x` and `yt` are the usual kinds. Other lowercase kinds are allowed.
- `handle` is non-empty, contains no whitespace, and keeps the case the user gave.

Refuse any other locator and ask again. Do not create a channel for it.

## Raw files

`sources/raw/` holds plain UTF-8 text and JSONL only: transcripts and post records. Never store video, audio, thumbnails, images, or raw HTML there. After a raw file is written, do not edit it. A later day is a new file. Details are in [references/ingest.md](references/ingest.md).

## Install

Copy the four type files into `<vault>/types/`. Their names are `shadow.toml`, `shadow.md`, `channel.toml`, and `channel.md`. Never write a type file whose bytes you composed yourself.

1. Read `assets/types/` beside this `SKILL.md` when that directory exists. Those bytes are the only source.
2. When `assets/types/` is absent, download each file from the release tag `v` plus `metadata.mnemic-version`. Never from `main`. The URL for a file is `https://raw.githubusercontent.com/sylver/mnemic/v<mnemic-version>/plugins/mnemic/skills/shadow/assets/types/<file>`.
3. Save downloads outside the vault. Any response that is not HTTP 200 stops the install. The error must name that URL. Write no type file from this run, including files that already returned 200.
4. For each file, compare it with `<vault>/types/<file>`:
   - Absent: copy it in.
   - Same bytes: do nothing.
   - Different bytes: leave the vault file untouched and report its path.

Run `mnemic check --json` after a copy. `INVALID_PACK` names the bad file. Stop and show it.

## Create

Ask for the person's name when you do not have it. Ask for every channel as `kind:handle`. Apply the locator rule and the name rule before any write.

1. `mnemic get <person-slug> --json`. When that note exists and its type is `person`, include `--field person=<person-slug>` in the next command.
2. Create the shadow. Omit `--field person` when there is no person note.

```bash
mnemic new shadow "Shadow: <Name>" --as <actor> --field person=<person-slug> --json
```

3. Create one channel per locator. `<url>` is the public page for that handle when you know it.

```bash
mnemic new channel "<locator>" --as <actor> --field shadow=shadow-<slug> --field locator=<locator> --field url=<url> --json
```

4. Backfill about 12 months, ending at the last completed day. Store each item with the ingest rules. Before each `new`, `mnemic get <item-slug> --json`. `ok: true` means the item is already stored: skip it. `NOT_FOUND` means write the raw file, then:

```bash
mnemic new source "<title>" --as <actor> --field url=<url> --field path=<path> --field sha256=<sha256> --field tags=shadow/<slug> --field tags=domain/<domain> --json
```

`<path>` is vault-relative, under `sources/raw/shadows/<slug>/`. `<sha256>` is the hex digest of that file. Repeat `--field tags` once per tag. Use `domain/<domain>` only when the item belongs to a domain the shadow tracks.

5. Advance each channel only across items that are stored. `<now>` is the local date `YYYY-MM-DD`, which `last_run_at` accepts.

```bash
mnemic set <channel> --field cursor=<id> --field last_run_at=<now> --json
```

6. Write the profile from the stored items, using [references/profile.md](references/profile.md). Edit the shadow body only. Do not hand-edit frontmatter and do not rewrite timeline lines.
7. Set each source you folded to `read`:

```bash
mnemic set <source> --field state=read --json
```

8. `mnemic check --json` must exit 0 before the commit.

## Refresh

Find channels with `mnemic links <shadow> --json` and keep backlinks whose type is `channel`. You can also `mnemic get <channel> --json` when you already know the slug.

For each channel:

1. Read `cursor`, `state`, and `blocked`. Fetch items newer than `cursor`, using [references/ingest.md](references/ingest.md). Store them the same way as create, including `get` before `new`.
2. When the read tool is missing, do not move `cursor`.

```bash
mnemic set <channel> --field state=blocked --field blocked=<tool> --json
```

`<tool>` is the missing program, such as the transcript tool or the raw post reader.

3. When the fetch finishes, set `ready` and move `cursor`. Do not pass `blocked` on this call. An empty value is `USAGE`, so a tool name from an earlier block can remain. `state=ready` is what the next refresh trusts.

```bash
mnemic set <channel> --field state=ready --field cursor=<id> --field last_run_at=<now> --json
```

4. Append one timeline line. The summary has no `|`.

```bash
mnemic log <channel> "YYYY-MM-DD | <locator> | stored N items, cursor <id>" --json
```

A page that failed is a gap. Leave `cursor` on the last stored item and log the gap instead:

```bash
mnemic log <channel> "YYYY-MM-DD | <locator> | gap <reason>" --json
```

5. Fold `unread` sources for this shadow into the profile. Find them with search, then `get` each hit and keep `state` `unread`. After the profile cites one, set that source to `read`.
6. `mnemic check --json`, then commit.

## Consult

1. `mnemic get shadow-<slug> --json` and read the profile.
2. Search the question text. `search` needs a non-empty query and accepts one `--tag`.

```bash
mnemic search "<question>" --type source --tag shadow/<slug> --json
```

When the question fits one domain, run this as well and open only hits that appear in both results:

```bash
mnemic search "<question>" --type source --tag domain/<domain> --json
```

3. `mnemic get <source> --json` for those hits only. Do not open the rest of the vault to answer.
4. Answer from those notes. Every claim ends with a wikilink to a source note you opened, such as `[[x-ada-2026-11-26]]`. A step the notes do not support starts with `Extrapolation:`.

## Persona

Same reads as consult. Answer in the person's voice, in the first person, until the user ends the persona. Every answer keeps the citation rule: a wikilink to an opened source, or `Extrapolation:` on anything past those notes. Do not drop citations for tone.

## Done when

- Install reported every vault type file it left in place, and wrote none from memory.
- Create and refresh stored each item once. A second run's `get` skips items already present.
- `cursor` moved only across stored items. A gap is a timeline line and the cursor stays put.
- Consult and persona cite a source note or mark extrapolation.
- `mnemic check --json` exits 0, and the commit subject is `mnemic(<actor>): shadow <mode> <slug>`.
