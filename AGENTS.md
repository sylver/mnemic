# Agents

## Definition of done

A change is done when these commands pass on a clean tree.

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --locked`

`cargo test --locked` includes the contract drift test. It fails when `docs/contract/check.schema.json` or `docs/contract/check.md` differ from the envelope types. A later verb extends those types, regenerates both files, and updates the `--json` snapshots in the same change.

## Where to look

- Coding standards are in [docs/coding-standards.md](docs/coding-standards.md).
- Pull requests use [.github/pull_request_template.md](.github/pull_request_template.md).
- Architecture decisions are in [docs/adr](docs/adr/README.md).
- Vault writes use [docs/write-primitive.md](docs/write-primitive.md).

## Writes

Use `with_write_lock`. Resolve the target inside it. Do not invent a write path. The lock and the rename are local filesystems only. External writers bypass the lock.

`FrontmatterEdit` returns bytes. It does not write files. Pass those bytes to `WriteTxn::atomic_write` inside `with_write_lock`. Do not round-trip frontmatter through a YAML serializer.

## Fixtures

Put shared test inputs in `fixtures/`. `fixtures/.gitkeep` keeps the directory in git until the first fixture lands.

## Vault template

`crates/mnemic/templates/AGENTS.md` is the vault template `mnemic init` writes. This file is the repo guide. They are different files. If a vault-root `AGENTS.md` already exists, `init` skips it, lists `AGENTS.md` in `data.skipped`, and exits 0.

## Type templates and overlays

Core body templates are compiled into `mnemic-core` beside the core pack. A vault overlays types from `<vault>/types/*.toml`. The same file stem replaces a built-in declaration whole. `<vault>/types/<type>.md` replaces that type's body template. See [the format doc](docs/format.md) and [ADR-0004](docs/adr/0004-templates-and-vault-overlays.md).
