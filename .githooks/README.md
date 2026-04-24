# Repo-local git hooks

Installed by `make init`, which runs:

```sh
git config core.hooksPath .githooks
chmod +x .githooks/*
```

## What's here

| Hook | Purpose |
| :--- | :--- |
| `pre-commit` | Enforces `commit.gpgsign=true`, blocks secret-shaped filenames, runs `cargo fmt --all -- --check` if cargo is on PATH. |
| `commit-msg` | Validates Conventional Commits subject (`feat:`, `fix:`, `chore:`, …). |

Both are POSIX `sh` — they run identically on macOS, Linux, WSL2, and
Git for Windows' bundled Git Bash.

## Opting out of a specific check

If you need to land a commit that a hook blocks (e.g. a deliberate WIP),
`git commit --no-verify` skips all hooks. Use sparingly.

## Signing, simplified

```sh
git config --global commit.gpgsign true
git config --global user.signingkey <key-id>   # GPG or SSH key
git config --global tag.gpgsign true
```

The `pre-commit` hook refuses to run unless `commit.gpgsign` is set.
Override per-repo if you really need to, but the project convention is
"always signed".
