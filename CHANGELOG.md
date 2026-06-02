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

- perf: dev profile compiles deps at 0 instead of 3, faster iteration
- chore: split workspace — core logic (grid, history, io, shapes, color) into `roxel-core`
- ci: add `--workspace` to clippy so it covers the new lib crate

## [0.6.5] - 2026-06-01

- chore: bump to 0.6.5
- fix(macos): open .rox from Finder on a cold launch
- feat(tools): S-key cycles through shape primitives
- feat(io): import .vox palette as a new swatch palette
- fix(icons): white-fill globe.svg so selected Sphere tints correctly
- feat(shape): add Sphere primitive
- fix(ci): clear Linux-only clippy errors in floating menu
- docs: finalize CHANGELOG for v0.6.4 [skip ci]

## [0.6.4] - 2026-05-31

- chore: bump to 0.6.4
- merge: ui.rs split + macOS Finder open into main
- docs(changelog): note ui.rs split into inspector.rs + floating.rs
- docs(ui): note ui_system is now a dispatcher; bodies in sibling modules
- refactor(ui): extract inspector panel into ui/inspector.rs
- refactor(ui): extract menu-pill body into floating::pill_menu_contents
- refactor(ui): extract tool-island body into floating::tool_island_contents
- docs: drop .fbx from supported-format list
- perf(grid): memoize bounding box for per-frame inspector
- perf(canvas): bound origin-triad voxel probe to the origin cube
- refactor(ecs): bundle 8+ arg systems into SystemParam structs
- ci(lint): make clippy a hard gate and clear warnings
- feat(io): track unsaved changes and guard Open/New
- refactor(tools): merge Fill into Paint with double-click flood
- feat(brush): Shift-lock brush stroke to a straight axis
- feat(ui): show real-time shape/selection info in palette panel
- docs: finalize CHANGELOG for v0.6.3 [skip ci]

## [0.6.3] - 2026-05-29

- chore: bump to 0.6.3
- feat(select): fill a selection with the current color
- feat(prefs): persist last dir, shape, and update-check opt-out
- chore(deps): update Cargo.lock to latest compatible patches
- feat(ui): nest camera items under a View → Camera submenu
- fix(ui): refine palette switcher preview swatches and headers
- feat(ui): make under-swatch color readout selectable
- test(menu): cover recent_item_label formatting
- feat(ui): apply Color Format everywhere; finish View menu toggles
- fix(ui): hover-highlight palette swatches like the recent-color strip
- fix(ui): remove per-color pick entries from command palette
- fix(ui): rename "Color Space" menu to "Color Format"
- fix(ui): enable the Color Space submenu
- feat(ui): move color-space format from Preferences to the View menu
- fix(ui): hide tool island under modal; scrim covers full window
- fix(ui): modal renders above scrim again; hide chrome instead of layering
- feat(ui): Esc closes the Preferences and Discard modals
- fix(ui): modal scrim now covers the tool island and gizmo
- docs: changelog for UI polish pass
- refactor(ui): split color picker and modals out of ui.rs
- feat(ui): dim backdrop behind open modals
- refactor(ui): share one modal_frame across all floating sheets
- refactor(ui): replace inline literals with design tokens
- refactor(theme): extract hover_fill, dedupe three inlined blends
- feat(theme): add semantic status colors, make toasts theme-aware
- docs: finalize CHANGELOG for v0.6.2 [skip ci]

## [0.6.2] - 2026-05-29

- chore: bump to 0.6.2
- use monospace font for swatch hover tooltip hex color
- fix(select): keep selection through camera orbit/pan
- fix(select): occlude marching ants behind voxels
- fix(select): select-all picks occupied voxels, not bounding box
- fix(ui): add spaces around × in sidebar size label
- fix(ui): hide gizmo in focus mode, show during flyby
- chore: clear all clippy warnings
- Don't switch to eyedropper when Opt is used with Z
- feat(ui): animate palette swatch reorder, drop gray placeholder
- fix(ui): thin left inspector edge, drop unselected swatch outline
- fix(ui): disable egui shape feathering to kill dark corner fringe
- feat(ui): palette panel rework — Cmd+K switcher, swatch grid, editable built-ins
- fix(ui): remove semi-transparent border from unselected swatches
- fix(render): apply face shade in sRGB space
- feat(ui): simplify color inspector to one Color section
- fix(ui): remove italics from hint labels
- docs: add changelog entries for click-outside close and unified shortcut chip color
- feat(ui): close command palette on click-outside
- feat(ui): overhaul command palette — no titlebar, per-key shortcut chips, hidden scrollbar, 10-row height
- docs: finalize CHANGELOG for v0.6.1 [skip ci]

## [0.6.1] - 2026-05-28

- chore: bump to 0.6.1
- fix(render): match palette swatch with Tonemapping::None on main camera
- feat(color): shift-click swatches for multi-color dithered fills
- fix(icon): macOS squircle + safe area master at 1024×1024
- feat(edit): double / halve density + blue info toasts
- docs: backfill Unreleased with post-0.6.0 changes
- fix(ui): toasts fade uniformly with multiply_opacity
- feat(ui): click-to-toggle shape picker + tight new-project modal
- feat(ui): coral brand accent + always-expanded section headers
- feat(gizmo): etched cube with muted face colors + grout lines
- docs: finalize CHANGELOG for v0.6.0 [skip ci]

## [0.6.0] - 2026-05-26

- chore: bump to 0.6.0
- feat(ui): brand teal accent + finalized tool button look
- feat(ui): collapsible inspector sections
- refactor(ui): drop show_status_chip + show_tool_labels prefs
- feat(ui): accent halo + white icon on selected tool
- feat(ui): tinted toasts with leading icon, drop side accent bar
- feat(ui): shadow-only dialogs, larger modal title, denser padding
- feat(ui): uppercase tracked section headers
- feat(ui): elevation tiers + neutral panel + brand accent
- refactor(updater): parse release JSON with serde_json
- chore(io): drop binary FBX exporter
- docs: split CLAUDE.md by subdir for lazy-load token savings
- docs: trim CLAUDE.md ~48% while preserving invariants
- feat(ui): ArrowUp/Down step numeric color inputs (Shift = ×10)
- feat(ui): accent preview outline + target highlight for erase/paint
- style(ui): drop floating-surface borders, prefer Monaco, fix canvas-bg seed
- feat(ui): canvas chrome overhaul + system blue accent + onboarding redesign
- feat(ui): canvas-first redesign with floating chrome
- feat(ui): first-launch coachmark tour
- docs: finalize CHANGELOG for v0.5.1 [skip ci]

## [0.5.1] - 2026-05-20

- chore: bump to 0.5.1
- refactor(tools): group tool_input_system params into SystemParams
- refactor(ui): tokenize inline spacing literals
- refactor(tools): extract stroke_anchor_from_hit helper
- refactor(clipboard): extract shared execute_paste helper
- feat(select): copy / cut / paste for selection
- ci: enforce CHANGELOG entry for feat/fix/perf PRs
- ci: add ui-tokens guard
- docs: add PR template + CONTRIBUTING guide
- chore: pin rust toolchain to 1.91.1
- chore: split git hooks into pre-commit (fmt) + pre-push (test)
- ci: add fmt + clippy jobs
- feat(updater): GitHub release update checker
- docs: trim CHANGELOG entry for color-space feature
- feat(ui): space-aware inspector readout + custom color picker
- feat(color): color-space conversions + Preferences pref
- feat(tools): shape aspect lock + long-press primitive picker
- docs: changelog + CLAUDE.md updates for sweep
- perf(tools): cache MoveDragState.originals_set
- refactor(io): dedup tmp_path test helper
- refactor(ui): promote inline literals to size/width/height tokens
- feat(camera): view-angle presets (Front/Back/Left/Right/Top/Iso)
- style: cargo fmt
- feat(select): double-click selects connected voxels, not AABB hull
- feat(ui): themed palette select dropdown
- chore: add release.sh helper script
- docs: finalize CHANGELOG for v0.5.0 [skip ci]

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
