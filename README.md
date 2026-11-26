# mnemic

`mnemic` is a command-line tool for a vault of markdown notes. The files in the vault are the source of truth. This repository is the program. `sylver/brain` is one such vault. It is not a place for CLI code.

`mnemic init` creates a new vault. It does not import an existing private vault.

Supported platforms are macOS and Linux.

## Install

A release binary is not published yet. Install from a clone with Cargo.

You need a Rust toolchain that provides `cargo`. From the repository root:

```bash
cargo install --path crates/mnemic
```

That puts `mnemic` on your `PATH`. To build without installing:

```bash
cargo build --release
```

The binary is `target/release/mnemic`. The checks below use `mnemic` on `PATH`.

## Check the fixtures

From the repository root, after the install step:

```bash
mnemic check fixtures/good
```

Exit 0 means that fixture vault passes.

## Create a vault

```bash
mnemic init --as <name> --vault /path/to/vault
```

`--as` writes `actor` into `mnemic.toml` when that file is created. `mnemic new` needs an author. If you omit `--as` and `mnemic.toml` has no `actor`, a later `new` exits with a usage error. You can set `actor` in `mnemic.toml` instead of passing `--as`.

`init` also writes `AGENTS.md`, a `.gitignore` line for `.mnemic/`, and one folder per core type. `.mnemic/` is local tool state. `mnemic.toml` is the vault config. Pass `--vault` as the directory to create. Without `--vault`, `init` uses the current directory and does not walk to a parent vault.

## Commands

These are the v1 commands. Names match `mnemic --help`.

- `check` checks core frontmatter in a vault or one note. `--fix` inserts a missing id, and a missing type when the folder maps to one type.
- `init` scaffolds a vault in the target directory.
- `log` appends one timeline line to a note.
- `search` searches parsed notes. Tokens are literal and combined with AND.
- `get` prints one note by id, slug, alias, or vault-relative path.
- `links` lists outgoing links, backlinks, and broken links for one note.
- `new` creates a note from a type template.
- `mv` moves or renames a note and rewrites incoming wikilinks.

There is no `index` command, no `migrate` command, and no `mcp` command.

## Further reading

- Note format, vault root, and `mnemic.toml` keys are in [docs/format.md](docs/format.md).
- The JSON envelopes for the commands are in [docs/contract/check.md](docs/contract/check.md). The schema is [docs/contract/check.schema.json](docs/contract/check.schema.json).
- How to change this repository is in [AGENTS.md](AGENTS.md).
