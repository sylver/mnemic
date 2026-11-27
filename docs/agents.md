# Agent assets

Agent instructions for a mnemic vault live in `plugins/mnemic/skills/mnemic/`. `SKILL.md` is the skill an agent loads. `references/contract.md` and `references/types.md` are generated views of the envelope codes and the embedded core pack. The skill links only those two files.

`cargo run -p mnemic-core --bin mnemic-contract` resolves the workspace root from the crate manifest, then writes four files from any working directory:

- `docs/contract/check.md`
- `docs/contract/check.schema.json`
- `plugins/mnemic/skills/mnemic/references/contract.md`
- `plugins/mnemic/skills/mnemic/references/types.md`

An optional output directory replaces the `docs/contract` location only. The skill references still land under the workspace root.

`crates/mnemic-core/tests/contract_drift.rs` reads those committed files and fails with the file name when the bytes differ from the renderers. `crates/mnemic/tests/skill.rs` checks the skill frontmatter against the Agent Skills rules and the workspace version, rejects a body that is too long or that names removed commands, and checks that every clap subcommand and every long flag in a verb synopsis exists on that subcommand.
