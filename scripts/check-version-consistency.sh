#!/usr/bin/env bash
set -euo pipefail

# ---------------------------------------------------------------------------
# check-version-consistency.sh — assert README install snippets match
# Cargo.toml `version`. Wired into CI so a Cargo.toml bump without a
# README bump fails the build, instead of shipping a release whose
# install instructions reference the previous version (the v0.0.3
# regression).
# ---------------------------------------------------------------------------

cd "$(git rev-parse --show-toplevel)"

CARGO_VERSION=$(grep -E '^version = ' Cargo.toml | head -1 | cut -d '"' -f2)
if [[ -z "$CARGO_VERSION" ]]; then
  echo "::error::could not extract version from Cargo.toml" >&2
  exit 2
fi

# Pull every `staticweaver = "x.y.z"` and `staticweaver = { version = "x.y.z" ... }`
# from README.md. Two patterns because TOML allows both shapes.
mapfile -t README_VERSIONS < <(
  grep -oE 'staticweaver[[:space:]]*=[[:space:]]*"[0-9]+\.[0-9]+\.[0-9]+"' README.md \
    | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'
  grep -oE 'staticweaver[[:space:]]*=[[:space:]]*\{[[:space:]]*version[[:space:]]*=[[:space:]]*"[0-9]+\.[0-9]+\.[0-9]+"' README.md \
    | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'
  # Pinned form: `staticweaver = "=x.y.z"` (the "Is it production-ready?" prose example)
  grep -oE 'staticweaver[[:space:]]*=[[:space:]]*"=[0-9]+\.[0-9]+\.[0-9]+"' README.md \
    | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'
)

if [[ ${#README_VERSIONS[@]} -eq 0 ]]; then
  echo "::error::no staticweaver = \"…\" snippets found in README.md — broke the version-consistency check itself?" >&2
  exit 2
fi

FAIL=0
for v in "${README_VERSIONS[@]}"; do
  if [[ "$v" != "$CARGO_VERSION" ]]; then
    echo "::error::README references staticweaver $v but Cargo.toml is $CARGO_VERSION"
    FAIL=1
  fi
done

if [[ $FAIL -eq 1 ]]; then
  cat <<EOM >&2

==============================================================================
  Version drift between Cargo.toml and README.md install snippets.
  Bump every \`staticweaver = "x.y.z"\` in README.md to match Cargo.toml.
  This check exists because the v0.0.3 release shipped with README install
  snippets stuck at "0.0.2" — see commit history on feat/v0.0.4.
==============================================================================
EOM
  exit 1
fi

echo "OK: README and Cargo.toml both at $CARGO_VERSION (${#README_VERSIONS[@]} snippets verified)"

# ---------------------------------------------------------------------------
# The MSRV. `rust-version` in Cargo.toml is the floor; every README
# statement of it must name the same one (1.89 and 1.89.0 are equal).
# Each pattern must still match, so a reworded statement fails here
# instead of silently dropping out of the check.
# ---------------------------------------------------------------------------
MSRV=$(grep -E '^rust-version = ' Cargo.toml | head -1 | cut -d '"' -f2)
norm() { if [[ $1 =~ ^[0-9]+\.[0-9]+$ ]]; then echo "$1.0"; else echo "$1"; fi; }
MSRV_PATTERNS=(
  '\*\*MSRV: Rust [0-9]+\.[0-9]+(\.[0-9]+)?\+\*\*'
  '\| \*\*MSRV\*\* \(declared .rust-version.\) \| [0-9]+\.[0-9]+(\.[0-9]+)? \|'
  '\| \*\*MSRV\*\* \| Rust [0-9]+\.[0-9]+(\.[0-9]+)? \|'
  'MSRV [0-9]+\.[0-9]+(\.[0-9]+)?, networking is opt-in'
  '\*\*MSRV is [0-9]+\.[0-9]+(\.[0-9]+)?\*\*'
)
for p in "${MSRV_PATTERNS[@]}"; do
  hit=$(grep -oE "$p" README.md | head -1 || true)
  if [[ -z "$hit" ]]; then
    echo "::error::README.md has no MSRV statement matching: $p"
    FAIL=1
    continue
  fi
  v=$(grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' <<<"$hit" | head -1)
  if [[ "$(norm "$v")" != "$(norm "$MSRV")" ]]; then
    echo "::error::README states MSRV $v ('$hit') but Cargo.toml rust-version is $MSRV"
    FAIL=1
  fi
done
if [[ $FAIL -eq 1 ]]; then
  exit 1
fi
echo "OK: README MSRV statements match rust-version $MSRV (${#MSRV_PATTERNS[@]} checked)"
