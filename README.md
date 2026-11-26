# mnemic

`mnemic` is a command-line tool for a vault of markdown notes. The files in the vault are the source of truth. This repository is the program. `sylver/brain` is one such vault. It is not a place for CLI code.

`mnemic init` creates a new vault. It does not import an existing private vault.

Supported platforms are macOS and Linux.

## Install

Releases are cut by [release-please](https://github.com/googleapis/release-please). Merging a conventional-commit pull request to `main` opens or updates a Release pull request. That pull request bumps `[workspace.package].version` in `Cargo.toml`, updates `Cargo.lock`, and writes `CHANGELOG.md`. Merging the Release pull request on GitHub creates the `vX.Y.Z` tag, the GitHub Release (with the changelog), and the macOS arm64 and Linux musl binaries. A manual `git tag` is not required. Pushing a `v*` tag by hand still publishes binaries when the tag matches `Cargo.toml`. This repository does not publish to crates.io.

### Cargo, from Git

You need a Rust toolchain that provides `cargo`.

```bash
cargo install --git https://github.com/sylver/mnemic mnemic --locked
```

That builds the `mnemic` package in `crates/mnemic` and puts `mnemic` on your `PATH`.

From a clone of this repository, the same package installs with:

```bash
cargo install --path crates/mnemic --locked
```

To build without installing:

```bash
cargo build --release
```

The binary is `target/release/mnemic`. The checks below use `mnemic` on `PATH`.

### GitHub release binary

The tag is `v` plus the workspace `version` in `Cargo.toml` (today `v0.1.0`). After the Release pull request merges, or after a person pushes that tag, the release has two binaries and a SHA-256 file next to each one:

- `mnemic-aarch64-apple-darwin` (macOS arm64)
- `mnemic-x86_64-unknown-linux-musl` (Linux x86_64, static musl)

Linux:

```bash
ver=0.1.0
asset=mnemic-x86_64-unknown-linux-musl
curl -fsSL -O "https://github.com/sylver/mnemic/releases/download/v${ver}/${asset}"
curl -fsSL -O "https://github.com/sylver/mnemic/releases/download/v${ver}/${asset}.sha256"
sha256sum -c "${asset}.sha256"
install -m 755 "$asset" "$HOME/.local/bin/mnemic"
```

macOS arm64 uses `asset=mnemic-aarch64-apple-darwin` and `shasum -a 256 -c "${asset}.sha256"` instead of `sha256sum`. `mnemic --version` prints the same version as `Cargo.toml`.

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

`init` also writes `AGENTS.md`, a `.gitignore` line for `.mnemic/`, and one `.gitkeep` in each empty type folder from the core pack. `.mnemic/` is local tool state. `mnemic.toml` is the vault config. Pass `--vault` as the directory to create. Without `--vault`, `init` uses the current directory and does not walk to a parent vault.

## Commands

These are the v1 commands.

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
- The generated JSON contract (envelopes and error codes) is in [docs/contract/check.md](docs/contract/check.md), with the schema in [docs/contract/check.schema.json](docs/contract/check.schema.json).
- How to change this repository is in [AGENTS.md](AGENTS.md).
