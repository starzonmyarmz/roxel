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

- fix(ui): add spaces around × in sidebar size label
- fix(ui): hide gizmo in focus mode; show during flyby when UI is visible
- feat(ui): palette swatches animate into place when dragging to reorder; the drop target is now empty space instead of a gray placeholder
- fix(ui): thin the left inspector edge — drop egui's redundant 1px separator, keep the hairline
- fix(ui): unselected palette swatches drop their outline entirely
- fix(ui): disable egui shape feathering — removes the dark fringe on rounded corners of light-coloured buttons, chips, and menu items
- fix(ui): palette swatches wrap into fixed rows instead of running off and forcing the inspector to max width
- fix(ui): swatch drag-to-reorder and right-click-remove work again; while dragging, the swatch lifts to a cursor ghost and neighbours shift to open a gap at the drop target
- feat(ui): "add current colour" is now a `+` cell at the end of the swatch grid; the `…` actions menu sits on the Palette title row with roomier item padding
- feat(ui): palette switcher rows preview colours as narrow, tall swatches, with clearer spacing between groups
- fix(ui): widen the "Discard edits?" modal so Cancel / Discard / Save as new fit on one row instead of clipping
- fix(ui): palette switcher no longer closes on the click that opened it
- fix(ui): inspector edge line no longer draws across the command palette / palette switcher
- fix(render): apply face shade in sRGB space so light-coloured faces are no longer over-darkened on sides and bottom
- feat(ui): reworked palette panel — palette switching moved to a Cmd+K-style popover; sidebar drops the dropdown for a single `…` actions menu (switch / new / duplicate-or-save-as / rename / delete / .ase)
- feat(ui): built-in palettes are now editable — edits are scratch and prompt "Save as new palette" to keep; switching away from a dirty built-in confirms before discarding
- fix(ui): make the left inspector resizable again — restore the separator handle (was hidden, leaving the panel stuck at width)
- feat(ui): simplified color inspector — one Color section (swatch + hex + recent strip); numeric editing now lives only in the picker popup
- feat(ui): moved color-space format (Hex/RGB/HSL/HSB/OKLCH) to Preferences → Color → Format
- feat(ui): overhauled command palette — no title bar, hidden scrollbar, 10-row height, per-key shortcut chips with Lucide modifier icons, search icon in surface frame, redesigned footer chips
- feat(ui): close command palette by clicking outside the window
- fix(ui): unify shortcut chip color so Esc, numbers, and Lucide modifier icons share the same primary text color

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
