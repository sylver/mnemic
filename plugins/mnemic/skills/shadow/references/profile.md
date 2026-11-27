# Shadow profile

The shadow note body is the profile. The headings below match the body template in `assets/types/shadow.md`. Keep them, in this order. A domain the shadow tracks is an extra section after Claims.

## Operating rules

- Write from stored source notes only. A stored note has a raw file under `sources/raw/` and a `source` note that points at it.
- One claim is one sentence. End it with a wikilink to the source note, for example `The engine is unfinished [[ada-example-letter]].`
- The wikilink target is that note's slug. `mnemic check` fails when the target does not resolve.
- A sentence the opened notes do not support starts with `Extrapolation:`. It has no wikilink.
- Do not cite a channel, a URL, or a raw path in place of the source note.
- An empty section stays the template line `Nothing in the archive yet.`
- Folding adds or revises sentences. Do not drop a cited sentence and leave the claim standing without a link.
- After a source is folded, set its `state` to `read`. Leave it `unread` until then.
- Edit the body in the shadow file. Change frontmatter with `mnemic set`. Append timeline lines with `mnemic log`. Do not rewrite older timeline lines.

## Frame

How this person frames the situation: what they treat as the scene, the actors, and the stakes. Each sentence cites a source.

## Questions

Questions they return to. Phrase them as questions they ask, and cite the item where they ask or press them.

## Tests

Checks they apply before they accept a claim: numbers they want, comparisons they run, what they count as a finished argument. Cite the item that shows the test.

## Refusals

What they will not use as evidence, will not decide from, or will not do. Cite the item that shows the refusal.

## Distinctions

Contrasts they keep apart (two words, two mechanisms, two groups). Cite the item that draws the line.

## Updates

Places they changed their mind. Cite the later source. Name the earlier source in the same sentence when it is stored.

## Claims

Statements they treat as true. Each one cites a source. This is the section consult answers from first.

## Domain sections

When items carry `domain/<name>`, add `## Domain: <name>` after Claims. Under it, put that subject's tests and claims, each with a wikilink. Do not invent a domain that no stored item uses.
