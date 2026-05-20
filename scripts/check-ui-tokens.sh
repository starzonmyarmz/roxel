#!/usr/bin/env bash
# Reject inline numeric literals in UI call sites that should resolve to
# `src/ui/tokens.rs`. Scoped narrowly to patterns that are currently clean
# so the guard catches *regressions* without requiring a full cleanup pass.
#
# When you legitimately need a new token, add it to `tokens.rs` and
# reference it (e.g. `radius::XS`) instead of inlining a literal.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

fail=0

scan() {
    local label="$1"
    local pattern="$2"
    # Search src/, exclude tokens.rs (the one file allowed to hold literals).
    local hits
    hits="$(grep -rEn "$pattern" src/ 2>/dev/null | grep -v 'src/ui/tokens.rs' || true)"
    if [ -n "$hits" ]; then
        echo "::error::Inline UI literal forbidden ($label). Use a token from src/ui/tokens.rs."
        echo "$hits"
        fail=1
    fi
}

# Corner radius literals: `CornerRadius::same(4)` etc. Must use `radius::*`.
scan "CornerRadius::same(<literal>)" 'CornerRadius::same\([0-9]'

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "ui-tokens: clean"
