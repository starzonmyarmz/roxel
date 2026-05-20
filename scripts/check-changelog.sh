#!/usr/bin/env bash
# Fail when a PR contains feat/fix/perf commits but CHANGELOG.md is unchanged.
# Skipped if any commit subject contains `[skip changelog]` or the PR carries
# the `no-changelog` label (label check happens in the workflow, not here).
#
# Args:
#   $1  base ref / SHA (commits before)
#   $2  head ref / SHA (commits after)
set -euo pipefail

BASE="${1:?base SHA required}"
HEAD="${2:?head SHA required}"

# Range of commits introduced by this PR.
subjects="$(git log --format='%s' "${BASE}..${HEAD}")"

if [ -z "$subjects" ]; then
    echo "changelog-check: no new commits"
    exit 0
fi

# Pull only the user-visible types out.
user_visible="$(echo "$subjects" | grep -E '^(feat|fix|perf)(\(|!|:)' || true)"

if [ -z "$user_visible" ]; then
    echo "changelog-check: no feat/fix/perf commits in this PR"
    exit 0
fi

# Honor `[skip changelog]` opt-out anywhere in a commit subject.
if echo "$user_visible" | grep -qF '[skip changelog]'; then
    echo "changelog-check: opt-out token found in commit subject"
    exit 0
fi

if git diff --name-only "${BASE}..${HEAD}" | grep -qx 'CHANGELOG.md'; then
    echo "changelog-check: CHANGELOG.md updated"
    exit 0
fi

echo "::error::PR has feat/fix/perf commits but CHANGELOG.md is unchanged."
echo "Add a bullet under '## [Unreleased]' in CHANGELOG.md, or include"
echo "'[skip changelog]' in a commit subject if this entry is intentional"
echo "(e.g. infra-only) and you've discussed it."
echo ""
echo "Offending commits:"
echo "$user_visible" | sed 's/^/  - /'
exit 1
