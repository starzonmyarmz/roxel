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

- feat(camera): view-angle presets (Front / Back / Left / Right / Top / Iso) in the command palette
- feat(select): double-click selects the connected same-color blob via flood, instead of the AABB hull
- feat(ui): themed palette select dropdown matching the design-token system
- chore: add `scripts/release.sh` helper for tagged releases
- refactor(ui): promote inline literals (modal widths, dropdown sizes, toast dims, command-palette dims) to `tokens::{size, width, height}`
- refactor(io): dedup `tmp_path` test helper across obj/fbx/svg/ase by routing through `io::test_util`
- perf(tools): cache `MoveDragState.originals_set` at drag start so per-frame collision checks no longer rebuild a HashSet

## [0.5.0] - 2026-05-19

- chore: bump to 0.5.0
- docs: finalize CHANGELOG for v0.4.3 [skip ci]

## [0.4.3] - 2026-05-19

- chore: bump to 0.4.3
- docs: note Open Recent menu in README, CHANGELOG, and CLAUDE.md
- feat(menu): add native Open Recent submenu on macOS
- feat(ui): track recently opened/saved projects + Open Recent menu
- feat(io): add recent-files store module
- feat(select): double-click selects connected same-color voxels
- feat(ui): rename select tool label to "Marquee select"
- docs: add sample .vox models and marketing imagery [skip ci]
- feat!: rename project file extension from .roxel to .rox
- docs: finalize CHANGELOG for v0.4.0 [skip ci]
- fix: correct version from 4.0.x to 0.4.x
- docs: finalize CHANGELOG for v0.4.1 [skip ci]
- docs: finalize CHANGELOG for v0.4.2 [skip ci]
- docs: changelog entry for flyby gizmo hide
- fix: hide orientation gizmo during flyby
- feat: flyby camera mode
- docs: CLAUDE.md note for OriginAxesGizmos group + show_origin_axes gate
- feat: hide-origin pref, transparent PNG export, axes over grid
- feat: Cmd+A selects every occupied voxel
- feat: ghost paint tool color on hovered voxel
- Merge pull request #1 from starzonmyarmz/worktree-ci-drop-sccache
- ci: drop sccache from CI workflow
- docs: finalize CHANGELOG for v4.0.2 [skip ci]

## [0.4.0] - 2026-05-19

- feat: flyby camera mode — cinematic auto-orbit with bobbing pitch and breathing radius, toggled from the command palette, Esc to exit
- polish: orientation gizmo hides during flyby so it doesn't spin distractingly with the orbit
- feat: preference to hide origin RGB axis triad
- fix: PNG export now writes a true transparent background instead of black
- fix: PNG export no longer captures floor grid, origin axes, or selection overlay
- polish: origin axes render above the floor grid via dedicated gizmo group with depth bias

## [0.4.2] - 2026-05-18

- ci: drop sccache from release workflow + bump 0.4.2
- docs: finalize CHANGELOG for v0.4.1 [skip ci]

## [0.4.1] - 2026-05-18

- chore: bump to 0.4.1
- ci: push releases to itch.io via butler
- docs: add macOS Gatekeeper bypass callout to README
- docs: finalize CHANGELOG for v0.4.0 [skip ci]
- ci: speed up workflows via sccache, nextest, prebuilt cargo-bundle

## [0.4.0] - 2026-05-18

- chore: bump to 0.4.0
- fix: drop unenrolled GitHub Sponsors entry from FUNDING.yml
- polish: voxel-only floor, multi-band grid, smoother zoom
- Modify FUNDING.yml with funding usernames
- feat: open-world voxel grid
- polish: tool-rail hover at 25% of selected fill
- feat: Save/Save As split, shape primitive popup, Cmd+D deselect
- feat: panel-aware initial framing + bounded zoom range
- chore: add dev feature for Bevy dynamic linking
- feat: design tokens for UI spacing, fonts, radii
- polish: lighter tool rail — transparent buttons, gray selected, hover tint
- polish: viewport overlay cleanup + floor grid + shape hover preview
- polish: move tool hints from right sidebar to status bar
- feat: Cmd+K command palette
- docs: add hero screenshot to README
- docs: CHANGELOG + README + CLAUDE for new io paths and toasts
- feat: voxel imports, glTF/Goxel export, in-app toasts
- Revise README description of voxel editor
- docs: finalize CHANGELOG for v0.3.2 [skip ci]

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
