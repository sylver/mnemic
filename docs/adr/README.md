# Architecture decision records

| ADR | Title | Status |
| --- | ----- | ------ |
| [0001](0001-advisory-write-lock.md) | Advisory write lock and durable rename | Accepted |
| [0002](0002-check-contract.md) | Check envelope and note file set | Accepted |
| [0003](0003-type-pack-and-frontmatter-edit.md) | Embedded type pack and span-preserving edits | Accepted |
| [0004](0004-templates-and-vault-overlays.md) | Body templates and whole-replace vault overlays | Accepted |
| [0005](0005-init-scaffold.md) | Init scaffolds one directory and leaves existing files | Accepted |
| [0006](0006-vault-index.md) | One vault index and one resolver | Accepted |
| [0007](0007-timeline-log.md) | Timeline lines are appended under one lock | Accepted |
| [0008](0008-get-and-links.md) | get and links share the vault index | Accepted |
| [0009](0009-search-scans-the-vault.md) | Search scans the parsed vault | Accepted |
| [0010](0010-new-note.md) | new files one note under the write lock | Accepted |
| [0011](0011-check-fix.md) | check --fix inserts only derived id and type | Accepted |
| [0012](0012-move-rename.md) | Move renames without replacing, then rewrites links | Accepted |
| [0013](0013-vault-readme-not-agents.md) | Vault root gets a static README, not AGENTS.md | Accepted |
| [0014](0014-set-writes-declared-fields-only.md) | set writes declared fields only | Accepted |
| [0015](0015-default-vault-user-config.md) | Default vault lives in user config, below the directory walk | Accepted |

## Rules

An ADR ships only in the pull request that changes the code it decides.

Write an ADR when a choice is hard to reverse, or when a later reader would otherwise guess why the code looks this way. Examples are a public contract, a storage layout, or a boundary between crates.

An ADR records the decision, the context, and the consequences. It does not record a task list, a status update, or a restatement of the code.
