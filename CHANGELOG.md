# Changelog

What changed for people using Roxel, newest first. Code-only work stays in the git history.

Add bullets under `## [Unreleased]`, grouped as Added, Changed, Fixed or Removed, as you land work. `scripts/release` renames the section to the new version + date, and the release workflow uses the same bullets for the GitHub Release notes.

## [Unreleased]

## [1.5.0] - 2026-11-25

### Added

- Plugins: drop JavaScript plugins in the plugins folder and run their commands from the new Plugins menu or the command palette. Plugins are experimental and may still change.
- `rox run` runs a JavaScript plugin command on a model and saves the result, with `--param` to set the command's settings. Plugins are experimental and may still change.
- Plugin commands can ask for settings (numbers, checkboxes, colors and choices) in a form before they run. The form remembers what you used last time.
- Plugins can add buttons to the tool rail that run their commands, with a bundled icon or their own SVG.
- Plugins can add a section to the inspector that shows facts about the model or selection, with settings and buttons for their commands.

## [1.4.0] - 2026-11-24

### Added

- `rox` installs with Homebrew: `brew install starzonmyarmz/tap/rox`.
- Roxel installs with Homebrew on macOS: `brew install --cask starzonmyarmz/tap/roxel`.

## [1.3.2] - 2026-11-22

- New `rox` command-line tool: `rox info` prints a model's size and colors as JSON, and `rox render` saves an isometric PNG. Download it from the release as `rox-macos.zip`, `rox-windows.zip` or `rox-linux.tar.gz`.
- `rox convert <in> <out>` saves a model as `.rox`, `.vox`, `.gox`, `.glb`, `.obj` or `.svg`, chosen by the output's extension.
- `rox apply` builds or edits a model from a RON list of ops (boxes, cylinders, ellipsoids, lines, clears, recolors and single voxels), read from a file or piped in from a script.
- Smaller downloads: the macOS zip drops from 81 MB to 48 MB, and the Windows and Linux builds shrink too.

## [1.0.0] - 2026-11-21

- Roxel 1.0, the first stable release.
- The user guide has a new look that matches the app, with a front page and key-chip shortcuts.

## [0.16.0] - 2026-11-21

- macOS builds are now universal, so they run natively on Intel Macs too.
- New Windows installer (`Roxel-setup.exe`): Start menu entry, uninstaller, and `.rox` files open in Roxel.
- New Linux build (`Roxel-linux.tar.gz`) with an install script that adds Roxel to your app launcher.
- Each release now includes a `SHA256SUMS` file.

## [0.15.0] - 2026-11-20

- The docs cover every shortcut and the newer tools, Export Shot and settings, and no longer mention the removed floor color.
- Updated the engine to Bevy 0.20 and egui 0.36, built with Rust 1.97.1.

## [0.14.2] - 2026-11-19

- Crashes are written to `crash.log` in Roxel's settings folder, so packaged builds leave a report to attach to an issue.
- macOS: Save and Export dialogs open in the last folder you used instead of the default one.
- The docs explain what the update check sends and how to turn it off.
- Help → Keyboard Shortcuts no longer has its own shortcut (Cmd/Ctrl+/).
- The Keyboard Shortcuts sheet shows keys as chips with modifier icons, and the key column no longer runs under the scrollbar.
- Updated dependencies (including the update checker's HTTP client, which still uses rustls) and built with Rust 1.92.

## [0.14.1] - 2026-11-19

- Saves and exports write to a temp file and rename, so a crash mid-save can't truncate the file.
- An unreadable `palettes.ron` is kept as a backup instead of being overwritten by the next palette change.
- `.vox`/`.qb`/`.gox` imports can be undone and no longer wipe your undo history; a file that fails to parse leaves the scene untouched.
- Quitting, Open Recent and opening a file from Finder now ask before discarding unsaved changes.
- The unsaved-changes prompt offers Save, Don't Save and Cancel; Enter saves instead of discarding.
- Deleting a palette asks for confirmation.
- `.rox` projects are saved in a compact, versioned binary format (about 20× smaller), and saving an unchanged scene writes identical bytes. Older projects still open.
- Projects remember their palette: opening one switches to it, adding it as a user palette if you don't have it.
- Windows/Linux: Ctrl+N, Ctrl+O, Ctrl+S, Ctrl+Shift+S and Ctrl+, now work, and Ctrl+Y redoes.
- Windows/Linux: the menu pill now has the macOS File, Edit, View and Help menus, including Open Recent, camera views, Changelog and an About screen.
- Windows/Linux: the command palette shows Ctrl shortcuts instead of ⌘.
- Roxel opens a `.rox` file passed on the command line, and release builds no longer open a console window on Windows.
- The user guide is published at https://starzonmyarmz.github.io/roxel/.
- Keyboard shortcuts no longer act on the scene while you're typing in a text field or a dialog is open.
- The command palette lists every menu command, including Open Recent, color formats, Rename palette, onboarding and Check for Updates.
- Palette actions behave the same in the inspector and the command palette; New and Duplicate open the rename field.
- Every release is published to https://starzonmyarmz.itch.io/roxel after its GitHub Release, so the itch app can install and update Roxel.
- macOS: Paste from the Edit menu (or ⌘V) lands under the cursor, like it does on Windows and Linux.
- Edit → Select All (Cmd/Ctrl+A) is now in the menu.
- Undo, cut, paste, fill, delete and arrow-key nudges wait until you release the mouse, so a drag can always be undone.
- macOS: ⌘Z/X/C/V in a text field edit the text instead of the scene.
- Paint and Erase act on a selection only when you click a selected voxel.
- `.vox` import brings in every model in a MagicaVoxel scene, placed and rotated as in MagicaVoxel.
- `.vox` export says when a scene has more than 255 colors and some were merged.
- Malformed `.qb` and `.gox` files show an error instead of crashing.
- Clicking a face of the orientation cube turns the view to face it.
- Clicks reach voxels in large scenes viewed from far away.
- PNG exports and project thumbnails no longer include the brush or shape preview.
- A failed Roxel Shot capture no longer blocks Export Shot until restart.
- Erasing on a chunk edge previews correctly, and edits show up the same frame.
- Once an update is found, Check for Updates becomes "Download Roxel vX.Y.Z…"; a failed check retries on next launch, and pre-release versions compare correctly.
- Big scenes stay responsive: large edits, imports and pastes mesh in parallel and stream in over a few frames, and moving or resampling large selections no longer slows down as they grow.
- Idle frames do less work with a selection, shape preview or brush active.
- PNG and Roxel Shot exports save in the background instead of freezing the app.
- glTF and OBJ exports merge same-color faces, so files are much smaller.
- Preferences → Appearance adds UI scale (80–150%) and Reduce motion.
- Help → Keyboard Shortcuts (Cmd/Ctrl+/) lists every shortcut.
- Focus mode is in the View menu and the command palette.
- Selected chips, primary buttons and dim text meet WCAG AA contrast in both themes.
- Tools have one name everywhere: Eyedropper and Select, not Pick and Marquee select.

## [0.7.1] - 2026-06-03

- fix(ui): use theme color tokens in floating save button tint
- docs: finalize CHANGELOG for v0.7.1 [skip ci]
- chore: bump to 0.7.1
- chore: gitignore local .cargo/config.toml
- docs: add user guide site and keep-current workflow
- fix(ui): make canvas/shot color pickers color-space aware
- refactor(ui): collapse duplicated dialog-spawn and widget helpers
- fix(ui): pin prefs/shot row label column to a fixed width
- feat(ui): Export Shot tweak panel — live preview + art-direction knobs
- feat(io): Export Shot — polished social-media PNG (phase 1)
- docs: finalize CHANGELOG for v0.7.0 [skip ci]

## [0.7.0] - 2026-06-02

- chore: bump to 0.7.0
- feat(io): show .rox artwork thumbnail in macOS Finder
- feat(ui): ENTER key confirms/submits dialog windows
- refactor: split core logic into roxel-core workspace crate
- docs: finalize CHANGELOG for v0.6.5 [skip ci]

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
