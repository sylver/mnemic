# Architecture decision records

| ADR | Title | Status |
| --- | ----- | ------ |
| [0001](0001-advisory-write-lock.md) | Advisory write lock and durable rename | Accepted |
| [0002](0002-check-contract.md) | Check envelope and note file set | Accepted |
| [0003](0003-type-pack-and-frontmatter-edit.md) | Embedded type pack and span-preserving edits | Accepted |

## Rules

An ADR ships only in the pull request that changes the code it decides.

Write an ADR when a choice is hard to reverse, or when a later reader would otherwise guess why the code looks this way. Examples are a public contract, a storage layout, or a boundary between crates.

An ADR records the decision, the context, and the consequences. It does not record a task list, a status update, or a restatement of the code.
