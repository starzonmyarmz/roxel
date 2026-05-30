# CLAUDE.md

Guidance for Claude Code working in this repo.

## Commands

- `cargo run` — dev build (opt-level=1 crate, 3 deps)
- `cargo run --release` — release
- `cargo check` — iterate with this, not `cargo build`
- `cargo test` — unit tests
- `cargo fmt` / `cargo clippy`

## Tests

Tests are inline `#[cfg(test)] mod tests` at the bottom of each `src/*.rs`. No `tests/` dir, no lib target. Coverage focuses on pure logic (grid, history, shapes, picking, mesh, camera, theme, io::*). **Don't spin up a Bevy `App` in tests** — exercise pure functions. File-IO tests use `std::env::temp_dir()`; don't add `tempfile`.

**Always add/update tests when adding/modifying a feature** — `cargo test` is a pre-push gate.

## Git hooks

Tracked in `.githooks/`. Opt in once per clone: `git config core.hooksPath .githooks`.

- `pre-commit` — `cargo fmt --all -- --check`
- `pre-push` — `cargo clippy --all-targets --no-deps -- -D warnings`, then `cargo test`

CI re-runs all three (fmt, clippy `-D warnings`, test), so `--no-verify` is caught upstream. Clippy is a hard gate — keep the tree warning-free.

## Architecture

Single-window Bevy 0.18 app, `bevy_egui` UI, `bevy_panorbit_camera` viewport. One binary (`src/main.rs`), no lib crate, no workspace.

Documentation is split by subdirectory so context windows stay small — Claude Code lazy-loads each subdir's `CLAUDE.md` only when reading files in or under that directory. Open the relevant one before editing:

- **`src/CLAUDE.md`** — core data flow (`VoxelGrid`, chunks, `History`, mesher, picking, preview), tools, camera, gizmo, canvas chrome, lighting, color space, snapshot, onboarding, updater, command-palette dispatch. Also `.rox` project format and new-project flow.
- **`src/io/CLAUDE.md`** — file I/O: async dialog rule, axis remaps, AABB-shift, format specifics (`.vox`/`.qb`/`.gox`/`.obj`/`.fbx`/`.gltf`/`.svg`/`.ase`), shared helpers, persisted resources (`palettes.ron`, `recent.ron`), macOS native menu.
- **`src/ui/CLAUDE.md`** — egui surface: panels and floating surfaces, design tokens, widget helpers, theme + preferences, fonts, icons, toasts, focus mode, mac titlebar, command-palette draw, onboarding overlay.

## Cross-cutting invariants

Apply everywhere — read before editing any module.

- **Mutations through `History::record`**, never `grid.set` directly. Undo cap `MAX_UNDO = 200`. New stroke clears redo.
- **Async file dialogs only.** Sync `rfd::FileDialog` blocks winit on macOS (beachball). Go through `PendingDialog`. Never reintroduce sync `rfd::FileDialog::*` in egui draw code.
- **`p.y >= 0`** — writes below floor silently refused at `VoxelGrid::set`.
- **Design tokens are mandatory.** Every spacing, padding, radius, font size, icon size, swatch size, stroke width, fixed widget size, container width/height must resolve to `src/ui/tokens.rs` — not an inline literal. Add a token if none fits. Colors live in `Theme` (`theme.rs`).
- **Prefer widget helpers** in `src/ui/widgets.rs` (`section`, `modal_window`, `tool_button`, etc.) over hand-rolling egui windows / hlines.
- **No `eprintln!` for user-facing I/O errors.** Use `Toasts` — terminal output is invisible in packaged apps. Internal diagnostics (dropped-voxel counts) can still go to stderr.
- **`Preferences` field changes:** every field after `theme` is `#[serde(default)]`. New fields need a default provider or older `preferences.ron` becomes unparseable and reverts to `Default`.
- **App-level write paths:** `NewProject.apply` clears the grid (don't poke `VoxelGrid` from UI). `Palettes` mutations must call `io::palettes::save(...)` — no autosave.
- **Buttons render icons only**, never Unicode glyphs or emoji.

## Bevy registration

`EguiPlugin` added with `auto_create_primary_context: false` (via `EguiGlobalSettings`) — required by the gizmo secondary camera. New resources: register with `init_resource` in `main.rs`. `Palettes` is the exception — uses `insert_resource(Palettes::with_user_loaded())` so user palettes load on startup.
