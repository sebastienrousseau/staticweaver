# ADR-005: Pin GitHub Actions by commit SHA

- Status: Accepted
- Date: 2026-11-07 (commit `a095385`, release v0.0.9)
- Issue: #54 (pinning item tracked in #51)

## Context and Problem Statement

Before v0.0.9 only the reusable `sebastienrousseau/pipelines`
workflows were referenced by commit SHA (from `20cffe0`); other
actions were referenced by tag. Issue #51 ("Dependabot
reconciliation sweep") listed a hardening item: "Pin every Action by
commit SHA (not tag), with `# vX.Y.Z` comment so Dependabot can
still PR", citing `step-security/secure-repo`. How should workflows
reference third-party actions?

## Considered Options

1. Reference actions by tag or branch (for example `@v7`).
2. Reference actions by full 40-character commit SHA, with the tag
   or branch kept as a trailing comment.

## Decision Outcome

Chosen option 2, in `a095385`: "Every third-party action is now
pinned by commit SHA with its tag or branch as a comment, the open
item of #51." `CHANGELOG.md` `[0.0.9]` records the same change.

Evidence at v0.0.9 (`71d204d`): every `uses:` line in
`.github/workflows/ci.yml`, `docs.yml`, `fuzz.yml`, `kani.yml` and
`release.yml` ends in a 40-character hexadecimal SHA, followed by a
comment such as `# v7`, `# stable` or `# main 2026-10-13`. This was
checked with `grep` for `uses:` lines lacking a 40-character SHA; it
returned none.

Rationale as recorded: the change is filed under issue #51, labelled
`type:security` and `type:supply-chain`, and the comment format keeps
Dependabot updates working (#51; `CHANGELOG.md` `[0.0.9]`). The
threat it addresses (maintainer, 2026-11-07) is both a moved or
rewritten tag and a compromised upstream: a retagged action has no
effect at all, and a compromised upstream only reaches the
repository through a SHA bump that is reviewed.

## Consequences

- Action updates arrive as Dependabot pull requests that change the
  SHA and its comment. `.github/dependabot.yml` groups all
  `github-actions` updates into one weekly PR (`github-actions`
  group, added in `f561311`).
- Floating refs such as `dtolnay/rust-toolchain` `# stable` and
  `# nightly` are now frozen to one commit of that action until
  Dependabot bumps them.
- The same commit added a top-level `permissions: contents: read` to
  `ci.yml` (least-privilege token); that is a related but separate
  control.

## More Information

- `.github/workflows/*.yml`, `uses:` lines
- `.github/dependabot.yml`, `github-actions` entry
- `CHANGELOG.md`, `[0.0.9]`
- Issue #51, "Hardening upgrade"
- Commits: `20cffe0`, `f561311`, `a095385`
