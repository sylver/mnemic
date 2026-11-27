# mnemic

[![CI](https://github.com/sylver/mnemic/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/sylver/mnemic/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/sylver/mnemic)](https://github.com/sylver/mnemic/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A command-line tool for a knowledge vault of plain Markdown notes, kept in a Git repository you own. Built for people and AI agents alike.

## Why mnemic

- 📄 **Plain files.** Each note is a Markdown file with YAML front matter. No database, no lock-in, and any editor works.
- 🔀 **Your repo, your sync.** A vault is a directory. You choose where it lives and where it pushes. mnemic never commits or pushes for you.
- 📦 **One static binary.** Prebuilt for macOS on Apple silicon and Linux x86_64. Nothing else to install.
- 🤖 **Agent-ready.** Every command speaks `--json` with one stable envelope and documented exit codes. Writes take a vault lock, so several agents can share one vault.

## Install

mnemic runs on macOS and Linux.

**Prebuilt binary.** Linux x86_64 (static musl):

```sh
asset=mnemic-x86_64-unknown-linux-musl
curl -fsSLO "https://github.com/sylver/mnemic/releases/latest/download/$asset"
curl -fsSLO "https://github.com/sylver/mnemic/releases/latest/download/$asset.sha256"
sha256sum -c "$asset.sha256"
mkdir -p ~/.local/bin && install -m 755 "$asset" ~/.local/bin/mnemic
```

On macOS (Apple silicon), set `asset=mnemic-aarch64-apple-darwin` and verify with `shasum -a 256 -c "$asset.sha256"`. Make sure `~/.local/bin` is on your `PATH`, or install to another directory that is.

**From source.** Needs Rust 1.100 or newer:

```sh
cargo install --git https://github.com/sylver/mnemic mnemic --locked
```

Check the install with `mnemic --version`.

## Quick start

A vault is any directory with a `mnemic.toml` file. Give it its own Git repository:

```sh
mkdir my-vault && cd my-vault
git init
mnemic init --as alice   # alice is recorded as the author of new notes
```

`init` writes `mnemic.toml`, a short `README.md`, one folder per note type, and a `.gitignore` entry for local state in `.mnemic/`. It never overwrites a file, so it is safe to run in an existing repository.

Add and connect notes:

```sh
mnemic new person "Ada Lovelace"            # people/ada-lovelace.md
mnemic new decision "Use plain Markdown"    # decisions/use-plain-markdown.md
mnemic log use-plain-markdown "2026-11-27 | Agreed with [[ada-lovelace]]"
mnemic links ada-lovelace                   # shows the backlink
mnemic search markdown
mnemic check                                # exit 0 means the vault is valid
```

Sync it like any repository:

```sh
git add -A && git commit -m "First notes"
git remote add origin <your-remote-url>
git push -u origin HEAD
```

Inside the vault, commands find `mnemic.toml` by walking up from the current directory. From anywhere else, pass `--vault`:

```sh
mnemic search ada --vault ~/my-vault
```

On another machine, clone your vault repository and run `mnemic` inside it.

## Commands

- `init`: scaffold a vault in a directory.
- `new`: create a note from its type template.
- `get`: print one note by id, slug, alias, or path.
- `search`: find notes where every token matches. Filter with `--type`, `--tag`, and `--since`.
- `links`: list outgoing links, backlinks, and broken links for a note.
- `log`: append a dated line to a note's timeline.
- `mv`: move or rename a note and rewrite the wikilinks that point to it.
- `check`: validate front matter and links. `--fix` fills in a missing `id` and `type`.
- `skill`: print the embedded agent skill and reference files.

Every command accepts `--json` and `--vault <path>`. Run `mnemic <command> --help` for the full options.

## Note types

| Type | Folder | Type | Folder |
| --- | --- | --- | --- |
| `person` | `people/` | `decision` | `decisions/` |
| `org` | `orgs/` | `topic` | `topics/` |
| `place` | `places/` | `source` | `sources/` |
| `project` | `projects/` | `journal` | `journal/` |
| `idea` | `ideas/` | `note` | `notes/`, `inbox/` |

`decision` and `journal` notes have a timeline for `mnemic log`. A vault can add or override types in `types/<name>.toml`; see [the format doc](docs/format.md).

## Built for agents

With `--json`, every command prints one envelope: `{"ok": true, "data": {…}, "error": null}` on success, or `ok: false` with an `error.code` on failure. Exit codes are stable: `0` success, `2` usage or validation error, `1` I/O error, and `75` (`LOCK_HELD`) when another writer holds the vault lock, so retry. The full contract is in [docs/contract/check.md](docs/contract/check.md), with a [JSON Schema](docs/contract/check.schema.json) for `check`.

### Use with agents

Install the `mnemic` binary first (see [Install](#install)). The skill in this repository is the same one each harness loads.

| Harness | Install |
| --- | --- |
| Claude Code | `claude plugin marketplace add sylver/mnemic`, then `claude plugin install mnemic@mnemic` |
| Codex | `plugin marketplace add sylver/mnemic` |
| Cursor | Team Marketplace import of `.cursor-plugin/marketplace.json` |
| Grok Bot | Install once as a global user skill |
| Other harnesses | `mnemic skill` |

## Documentation

- [Note format and vault layout](docs/format.md)
- [JSON contract and error codes](docs/contract/check.md)
- [Architecture decisions](docs/adr/README.md)
- [Agent skill guide](docs/agents.md)
- [Changelog](CHANGELOG.md)

## Contributing

Issues and pull requests are welcome. Read [AGENTS.md](AGENTS.md) for the definition of done and repository conventions. Pull request titles follow [Conventional Commits](https://www.conventionalcommits.org/).

## License

[MIT](LICENSE)
