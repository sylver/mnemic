# Agents

## Definition of done

A change is done when these commands pass on a clean tree.

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --locked`

Later work adds contract snapshots to that list.

## Where to look

- Coding standards are in [docs/coding-standards.md](docs/coding-standards.md).
- Pull requests use [.github/pull_request_template.md](.github/pull_request_template.md).
- Architecture decisions are in [docs/adr](docs/adr/README.md).

## Fixtures

Put shared test inputs in `fixtures/`. `fixtures/.gitkeep` keeps the directory in git until the first fixture lands.

## Vault template

A later unit fills `crates/mnemic/templates/AGENTS.md`. That path is the vault template. This file is the repo guide. They are different files.
