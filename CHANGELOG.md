# Changelog

All notable changes to Roxel are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Add bullets under `## [Unreleased]` as you land work. When a `v*` tag is
pushed, `.github/workflows/release.yml` renames the section to the new
version + date, regenerates the bullets from the tag's commit range,
commits the result back to `main`, and uses the same body for the GitHub
Release notes.

## [Unreleased]

- feat: open-world voxel grid — no fixed canvas size. Paint anywhere on X / Z, as high as memory allows; y < 0 is the only hard bound. Chunks allocate on first write and drop when empty.
- feat: two-band y=0 grid — per-voxel (with every-16 major) up to radius 128, every-16 only up to radius 512, hidden beyond. RGB axis triad always at world origin, with an optional Y-axis sky line as a vertical anchor.
- feat: footer reads `Size W×H×D` from the occupied AABB and `Zoom N voxels` from the live orbit radius.
- breaking: ground floor plane dropped; the y=0 grid is the only floor reference. `show_floor` / `floor_color` / `preview_outline` preferences removed (old `preferences.ron` files load silently — serde drops the unknown keys).
- polish: zoom uses √2-per-click steps (was ×2); lower bound fixed at radius 8 so big scenes still allow close inspection, upper bound capped at `max(2 × fit_radius, 64)`. Z+click recenters focus to the voxel under the cursor before zooming.
- feat: soft toast warning when a scene crosses 5M voxels or 4K loaded chunks; one-shot, clears with hysteresis.
- breaking: `.roxel` v1 format dropped. New format stores only `voxels` (no `version`, no `size`). v1 files happen to load when their `voxels` field matches but are not officially supported.
- breaking: New-project modal is a confirm dialog only — no grid-size picker.
- breaking: wall planes removed (visibility toggle, color preference, related commands gone). Old `preferences.ron` files with `show_walls` / `wall_color` load silently.
- breaking: `.vox` and `.qb` exports AABB-shift to origin so negative-coord scenes survive the round-trip; `.vox` refuses extents > 256 per axis (format cap).
- refactor: VoxelGrid storage is sparse — `HashMap<IVec3, Chunk>` instead of a pre-allocated 128³ flat array. Chunk entities spawn/despawn dynamically.
- feat: Save remembers the last-used file path; new Save As… entry (⇧⌘S) under File menu and command palette
- feat: Shape tool — primitive popup on the left rail; rail icon reflects the active primitive (Rectangle / Ellipse / Line); drop the Filled toggle (always filled)
- feat: Cmd/Ctrl+D clears the current selection
- fix: initial framing is panel-aware on startup and after every project rebuild — voxels no longer land offscreen behind side panels
- chore: optional `dev` cargo feature for Bevy dynamic linking (`cargo run --features dev`) for faster link times
- polish: left tool rail — transparent unselected buttons, gray selected, lighter-gray hover; tighter spacing; shorter status bar
- feat: brush-style hover ghost for the Shape tool before the first click
- polish: subtler brush + shape previews — outline alpha halved, shape silhouette draws only boundary edges instead of per-cell wireframes
- polish: drop per-cell wireframes from selection render; keep the marching-ants AABB
- polish: regroup left tool rail (Brush · Erase · Paint · Pick · Shape · Select · Move)
- polish: pin Cmd+K palette to the top of the canvas so it doesn't jump as results filter
- polish: move per-tool instructions from right sidebar into status bar; truncate when narrow
- feat: Cmd+K command palette — fuzzy-searchable surface for every action (file ops, tools, view toggles, palette + color switching, preferences)
- feat: import MagicaVoxel `.vox`, Qubicle `.qb`, and Goxel `.gox` files
- feat: export Goxel `.gox` and glTF `.glb` (Unity / Godot compatible)
- feat: in-app toast notifications for save / load / import / export
- feat: foreign-tool axis remap for `.vox` / `.gox` (Z-up ↔ Y-up) so MagicaVoxel and Goxel files open upright in both directions
- refactor: extract shared io helpers (`snap_to_allowed_size`, `LeReader`, `for_each_exposed_face`, test `tmp_path`)

## [0.3.2] - 2026-05-14

- release: 0.3.2
- polish: declutter palette UI, tighten right sidebar
- polish: themed Create / Cancel buttons in New-project modal
- style: cargo fmt
- refactor: extract reusable egui widget helpers from ui.rs
- docs: add CI status badge to README
- ci: run cargo test on push and pull request
- feat: animated marching-ants selection outline with x-ray wireframes
- fix: Select tool picks clicked voxel, not adjacent empty cell
- feat: macOS Help menu with Changelog link
- feat: Cmd/Ctrl + =/-/0 zoom and frame-view shortcuts
- ci: tag-driven CHANGELOG finalize (drop prepare-commit-msg hook)
- chore: add CHANGELOG.md and prepare-commit-msg hook
- polish: macOS .app distribution — bleed icon, About metadata, ad-hoc sign
- docs: trim README features list and drop project layout

## [0.3.1] - 2026-05-13

- chore: bump version to 0.3.1
- ci: tag-driven mac .app + windows .exe release
- polish: unify UI font sizes, spacing, and selectable rules
- chore: remove Scene stats panel from inspector
- feat: Move tool with drag + arrow-key selection translation
- feat: bidirectional shape extrude
- feat: bidirectional select extrude
- feat: Select tool with face-plane box marquee
