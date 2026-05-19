#!/usr/bin/env bash
# Bump version in Cargo.toml, commit, tag, and push.
# The "v*" tag push triggers .github/workflows/release.yml, which builds
# the macOS/Windows artifacts, finalizes CHANGELOG.md, and creates the
# GitHub Release. Nothing else to do locally.
#
# Usage: scripts/release.sh <major|minor|patch>
#        scripts/release.sh patch --dry-run

set -euo pipefail

bump=""
dry_run=0
assume_yes=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    -y|--yes)  assume_yes=1 ;;
    major|minor|patch) bump="$arg" ;;
    *)
      echo "unknown arg: $arg" >&2
      echo "usage: $0 <major|minor|patch> [--dry-run] [-y|--yes]" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$bump" ]]; then
  echo "usage: $0 <major|minor|patch> [--dry-run] [-y|--yes]" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

cargo_toml="Cargo.toml"
[[ -f "$cargo_toml" ]] || { echo "no Cargo.toml at $repo_root" >&2; exit 1; }

run() {
  if (( dry_run )); then
    printf '+ %s\n' "$*"
  else
    "$@"
  fi
}

# --- preflight ---------------------------------------------------------------

branch="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$branch" != "main" ]]; then
  echo "must be on main (current: $branch)" >&2
  exit 1
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "working tree not clean; commit or stash first" >&2
  git status --short >&2
  exit 1
fi

echo "fetching origin..."
git fetch --tags origin main

local_sha="$(git rev-parse HEAD)"
remote_sha="$(git rev-parse origin/main)"
if [[ "$local_sha" != "$remote_sha" ]]; then
  echo "local main is not in sync with origin/main" >&2
  echo "  local:  $local_sha" >&2
  echo "  remote: $remote_sha" >&2
  exit 1
fi

# --- compute next version ----------------------------------------------------

current="$(awk -F'"' '/^version[[:space:]]*=/ { print $2; exit }' "$cargo_toml")"
if [[ -z "$current" ]]; then
  echo "could not read version from $cargo_toml" >&2
  exit 1
fi

IFS='.' read -r major minor patch <<<"$current"
if ! [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ && "$patch" =~ ^[0-9]+$ ]]; then
  echo "current version '$current' is not X.Y.Z" >&2
  exit 1
fi

case "$bump" in
  major) major=$((major + 1)); minor=0; patch=0 ;;
  minor) minor=$((minor + 1)); patch=0 ;;
  patch) patch=$((patch + 1)) ;;
esac

next="$major.$minor.$patch"
tag="v$next"

if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  echo "tag $tag already exists locally" >&2
  exit 1
fi
if git ls-remote --tags --exit-code origin "refs/tags/$tag" >/dev/null 2>&1; then
  echo "tag $tag already exists on origin" >&2
  exit 1
fi

echo
if (( dry_run )); then
  echo "DRY RUN — no files written, no commits, no push."
else
  echo "LIVE RUN — this will commit, tag, and push to origin."
fi
echo "  current: $current"
echo "  next:    $next"
echo "  tag:     $tag"
echo

if ! (( dry_run )) && ! (( assume_yes )); then
  read -r -p "proceed? [y/N] " reply
  case "$reply" in
    y|Y|yes|YES) ;;
    *) echo "aborted."; exit 1 ;;
  esac
fi

# --- mutate ------------------------------------------------------------------

# Only the first `version = "..."` line (the [package] one). BSD/macOS sed
# doesn't support -i without an extension arg, so write to a tmp file.
tmp="$(mktemp)"
awk -v new="$next" '
  !done && /^version[[:space:]]*=[[:space:]]*"[^"]+"/ {
    sub(/"[^"]+"/, "\"" new "\"")
    done = 1
  }
  { print }
' "$cargo_toml" > "$tmp"

if (( dry_run )); then
  echo "--- Cargo.toml diff ---"
  diff -u "$cargo_toml" "$tmp" || true
  rm -f "$tmp"
else
  mv "$tmp" "$cargo_toml"
fi

# Refresh Cargo.lock so the roxel package entry matches the new version.
# `cargo update -p roxel` is fast and doesn't touch unrelated deps.
run cargo update -p roxel --offline >/dev/null 2>&1 || run cargo update -p roxel

run git add Cargo.toml Cargo.lock
run git commit -m "chore: bump to $next"
run git tag -a "$tag" -m "$tag"
run git push origin main
run git push origin "$tag"

if (( dry_run )); then
  echo "dry run complete — no changes made"
else
  echo
  echo "pushed $tag — GitHub Actions will build artifacts, finalize CHANGELOG, and create the release."
  echo "watch: https://github.com/starzonmyarmz/roxel/actions"
fi
