#!/usr/bin/env bash
# Reject inline numeric literals in UI call sites that should resolve to
# `src/ui/tokens.rs`: radii, margins, spacing, strokes, font sizes and fixed
# sizes/offsets. Each pattern is one the tree is clean of, so the guard
# catches regressions. Zero and ratios (`* 0.5`, `/ 3.0`) aren't flagged.
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

# Margins and spacing: `Margin::same(8)`, `Margin { left: 12, .. }`,
# `add_space(6.0)`, `item_spacing.x = 4.0`.
scan "Margin::same/symmetric(<literal>)" 'Margin::(same|symmetric)\([0-9]'
scan "Margin { side: <literal> }" '(left|right|top|bottom): [1-9][0-9]*,'
scan "add_space(<literal>)" 'add_space\([0-9]'
scan "item_spacing = <literal>" 'item_spacing(\.[xy])? = [1-9]'

# Stroke widths and font sizes.
scan "Stroke::new(<literal>, ..)" 'Stroke::new\([0-9]'
scan "RichText::size(<literal>)" '\.size\([0-9]'
scan "FontId::*(<literal>)" 'FontId::(new|proportional|monospace)\([0-9]'

# Fixed sizes and offsets: `vec2(28.0, 28.0)`, `Vec2::splat(12.0)`,
# `set_width(300.0)`, `rect.left() + 10.0`, `font::SECTION + 6.0`.
scan "vec2(<literal>, ..)" 'vec2\([1-9][0-9]*\.[0-9]*,'
scan "vec2(.., <literal>)" 'vec2\([^()]*, *[1-9][0-9]*\.[0-9]*\)'
scan "Vec2::splat(<literal>)" 'Vec2::splat\([1-9]'
scan "set_*width/height(<literal>)" 'set_(min_|max_)?(width|height)\([0-9]'
scan "desired_width(<literal>)" 'desired_width\([0-9]'
scan "rect edge +/- <literal>" 'rect\.(left|right|top|bottom)\(\) [+-] [0-9]'
scan "<token> +/- <literal>" '(font|space|size|width|height|icon)::[A-Z_]+ [+-] [0-9]'
scan "circle radius <literal>" 'circle(_filled|_stroke)?\([^,]+, [0-9]'

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "ui-tokens: clean"
