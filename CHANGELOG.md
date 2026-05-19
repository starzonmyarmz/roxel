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

- breaking: project file extension renamed from `.roxel` to `.rox`; existing files must be renamed
- feat: Open Recent menu — last 10 opened or saved `.rox` projects persist to `recent.ron`, available from the File menu (in-app and native macOS menu bar)

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
