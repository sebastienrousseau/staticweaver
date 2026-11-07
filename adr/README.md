# Architecture Decision Records

Decisions that shape staticweaver, in the
[MADR](https://adr.github.io/madr/) format. Each record cites the
file, section or commit its claims come from; claims without
evidence in the repository were confirmed by the maintainer on
2026-11-07 and are marked "(maintainer, 2026-11-07)".

| ADR | Title | Status | File |
| :--- | :--- | :--- | :--- |
| ADR-001 | Forbid `unsafe` code | Accepted | [0001-forbid-unsafe.md](0001-forbid-unsafe.md) |
| ADR-002 | Reject a bytecode VM | Accepted | [0002-reject-bytecode-vm.md](0002-reject-bytecode-vm.md) |
| ADR-003 | Sync by default, async opt-in | Accepted | [0003-sync-default-async-opt-in.md](0003-sync-default-async-opt-in.md) |
| ADR-004 | Workspace split (core / std / async / cli) | Accepted | [0004-workspace-split.md](0004-workspace-split.md) |
| ADR-005 | Pin GitHub Actions by commit SHA | Accepted | [0005-sha-pinned-github-actions.md](0005-sha-pinned-github-actions.md) |
| ADR-006 | Drop `askama_escape` | Accepted | [0006-drop-askama-escape.md](0006-drop-askama-escape.md) |

## Adding a record

Copy the structure of an existing record, take the next number, and
add a row to the table above.
