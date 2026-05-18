# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

- `cargo run` — launch the editor (dev profile; opt-level=1 for the crate, 3 for deps via `[profile.dev.package."*"]`)
- `cargo run --release` — release build, slow to compile, fast at runtime
- `cargo check` — fast type/borrow check; use this in iteration, not `cargo build`
- `cargo test` — unit tests (inline `#[cfg(test)] mod tests` per source file)
- `cargo fmt` / `cargo clippy` — standard Rust toolchain

## Tests

Tests live as inline `#[cfg(test)] mod tests` blocks at the bottom of each `src/*.rs` module — there is no lib target and no `tests/` directory. Coverage focuses on pure logic: `grid` (sparse chunk allocate/drop, y<0 refusal, iter_occupied, bounding_box across negative coords, dirty-chunk seam propagation, perf-threshold latch math), `history` (record/undo/redo/dedup/cap), `shapes` (rect/ellipse/line2d/extrude), `picking` (DDA raycaster + y=0 fallback + step-cap termination in empty world), `mesh` (sRGB roundtrip, greedy quad counts, chunked vs monolithic equivalence across seams, negative coords), `camera` (fit_view, zoom step reciprocity, zoom_radius_limits lower/upper bounds), `theme` (canvas resolution + serde back-compat for older `preferences.ron` carrying now-removed `show_floor` / `floor_color` / `show_walls` / `wall_color` fields), `io::project` (sparse roundtrip, negative + far-coord roundtrip, v1 lax-deserialize), `io::vox` (AABB-shift on export, refusal beyond 256³, axis remap), `io::palettes` (user palette roundtrip, builtins not persisted). Avoid spinning up a Bevy `App` in tests — exercise the pure functions instead. File-IO tests use `std::env::temp_dir()`; do not add `tempfile` as a dep.

**Always add or update tests when adding or modifying a feature** — `cargo test` runs as a pre-commit gate (see below), so untested feature work is incomplete.

## Pre-commit hook

`.githooks/pre-commit` runs `cargo test` and is tracked in the repo. Fresh clones must opt in once:

```sh
git config core.hooksPath .githooks
```

A local copy at `.git/hooks/pre-commit` is what git invokes by default; the canonical, tracked copy is `.githooks/pre-commit`. Bypass with `git commit --no-verify` when intentional (rare).

## Architecture

Roxel is a single-window Bevy 0.18 app with a `bevy_egui` UI overlay and a `bevy_panorbit_camera` viewport camera. Everything is in one binary (`src/main.rs`) — no library crate, no workspace.

### Data flow

`VoxelGrid` (`grid.rs`) is the single source of truth. Storage is **sparse**: a `HashMap<IVec3, Chunk>` keyed by chunk coordinate. Each `Chunk` holds a flat `Box<[Option<Color8>]>` of `CHUNK_VOL = 32³` cells plus an occupancy `count`. Chunks allocate on first write to any of their cells and **drop** the moment the count hits zero. The only hard rule is `p.y >= 0`; writes below the floor are silently refused. There is no upper bound on X, Y, or Z — the open world is sized by the user's memory.

`CHUNK = 32`, `CHUNK_I = 32`, `CHUNK_VOL = 32_768`. `chunk_coord(p)` returns the chunk key (`div_euclid` per axis so negative coords work); `local_idx(p)` is the flat index into a chunk's cell array (`rem_euclid`).

Every `set` flips a global `dirty: bool`, inserts the owning chunk coord into `dirty_chunks: HashSet<IVec3>`, and inserts seam-neighbour chunk coords for cells on a chunk boundary — face occlusion changes there too. `dirty_chunks` is drained by `regenerate_mesh_system`. `total_count` and `warned_large` track the soft perf-warning latch (`large_scene_threshold_crossed` / `large_scene_warning_cleared` are the pure predicates; `perf_warn_system` in `main.rs` ticks them and fires a one-shot toast).

Mutations flow through `History::record` (`history.rs`), **not** `grid.set` directly. Recording wraps each `grid.set` in a `CellDelta`, dedupes per stroke via a `HashSet<(i32,i32,i32)>`, and appends to `History.current`. `History::begin` / `History::end` bracket a stroke; the LMB-release path in `tool_input_system` is responsible for calling `end`. The undo stack is capped at `MAX_UNDO = 200`; pushing a new stroke clears the redo stack. The picker reads pre-stroke values via `history.pre_stroke_value(p)` (per-cell shadow), so voxels placed earlier in the same stroke are invisible to the next pick — this is what kills runaway stacking, and replaces the old full-grid-clone snapshot.

`regenerate_mesh_system` (`mesh.rs`) drains `grid.dirty_chunks` each frame. For each dirty coord: if the chunk has data, spawn an entity on first sight (or update the existing mesh handle); if the chunk has emptied, despawn its entity. Chunk meshes live in `VoxelChunkMeshes { chunks: HashMap<IVec3, (Entity, Handle<Mesh>)>, material: Handle<StandardMaterial> }` — entities are created lazily and despawned on empty. The mesher is greedy: `greedy_quads_bounded` merges same-color same-direction faces inside a half-open `[min, max)` chunk-coord-aligned box while still querying the full grid for cross-bounds occlusion so emitted quads agree at chunk seams with the monolithic `greedy_quads` reference (which itself walks loaded chunks). Touching `grid` outside `History::record` still works but bypasses undo, so prefer the history path. Base colors are run through `srgb_to_linear` before upload — Bevy's pipeline expects linear vertex colors.

The mesher also consults `PreviewHide` (`mesh.rs`): when the erase preview is active, the targeted cell is filtered out of `build_mesh` so the voxel-to-be-removed visually disappears under the cursor. A change to `PreviewHide.cell` re-triggers a mesh rebuild for that chunk even when `grid.dirty` is false. `brush_preview_system` (`preview.rs`) drives the ghost cuboid for `Tool::Brush` and the `PreviewHide` cell for `Tool::Erase`; `shape_preview_system` (`shape_preview.rs`) drives the ghost mesh for `Tool::Shape`. Both are scheduled `before(regenerate_mesh_system)` so the mesher sees the current frame's hide state.

Both previews hide while the user is orbiting (RMB), dragging the gizmo, or already mid-stroke — checked via mouse state, `GizmoDrag.active`, and `PointerState.stroking`. Don't reintroduce a preview that flashes during orbit; it's distracting and was deliberately removed.

### Tools and input

`tool_input_system` is the central tool dispatcher. It early-returns when egui wants the pointer (`is_pointer_over_area` or `wants_pointer_input`) or when the gizmo viewport rect contains the cursor — these gates exist to prevent painting through UI. `Tool::Eyedropper` is single-click; it auto-restores `tool.previous` on release **unless** Alt is held (Alt keeps the eyedropper sticky for repeated picks, driven by `alt_eyedropper_system`). The other tools are stroke-based and rely on `PointerState.stroking` plus `history.begin/record/end`.

`Tool::Shape` (rectangle / ellipse / line, configurable via `ShapeOptions`) is two-click: first click anchors corner1 on the picked face, drag previews the 2D footprint, second click commits and lets the user drag along the face normal to extrude (`shapes::extrude` lifts the 2D cell set into a 3D box / cylinder / line column). The shape's in-progress state lives in `ShapeState`; `tool_input_system` checks `state.phase` to route input through the rectangle/ellipse/line cell generators in `shapes.rs`.

`Tool::Select` (`select.rs`) is the same two-phase face-plane drag as Shape but commits an `AABB` into `Selection` instead of writing voxels. `selection_render_system` draws the live AABB outline + filled-cell overlay; `selection_key_action_system` wires Backspace/Delete to `clear_aabb` and Esc to clear the selection.

`Tool::Move` (`select.rs` + `move_drag_system` in `tools.rs`) translates the contents of the active selection by integer cell offsets. Two input paths, both share `MoveDragState` (mouse) and the pure `select::move_selection` helper:
- **Mouse drag** — `move_drag_system` runs as its own `Update` system. LMB-press on a voxel inside the selection anchors a face plane (same `StrokeAnchor` machinery as Shape/Select); cursor motion projects to that plane and snaps to integer cells via `constrain_move_delta`. The drag locks the face-normal axis; Shift also locks Y so the move stays on the same horizontal plane. A move that would overlap a non-source occupied cell is refused, leaving the selection at the last valid delta. `history.abort` rolls partial writes back when the user RMB / Esc / switches tool mid-drag. Click on a bare voxel with no active selection creates an ad-hoc 1×1×1 selection that is cleared on release.
- **Arrow keys** — `move_selection_keys_system` calls `move_selection` once per `just_pressed`. ←/→ = ∓X, ↑/↓ = ∓Z, Shift+↑/↓ = ±Y. Collisions and below-floor shifts are rejected; X / Z are unbounded.

Both paths record exactly one history stroke per commit. Mid-drag, frames re-record the same touched cells repeatedly; `History::record` dedupes by overwriting the existing delta's `after` value so the final stroke contains one entry per cell regardless of how many frames the drag spanned.

Two pieces of stroke state in `PointerState` matter for non-shape tools:
- `anchor` (`StrokeAnchor`) — locks the build plane axis for the duration of a drag so the picker can't slide onto a perpendicular face mid-stroke.
- `last_placed: Option<IVec3>` — endpoint for `line3d` (3D Bresenham). Fills gaps when the cursor jumps between frames, and `Shift+click` runs a one-shot `line3d` stroke from `last_placed` to the new target without entering drag mode.

Pre-stroke values for the picker come from `history.pre_stroke_value(p)` — a per-cell shadow built up as cells are recorded during the stroke. This replaced an earlier full-grid `VoxelGrid` clone that paid 8 MB per stroke; in the open world a clone would be unbounded, so the per-cell path is the only viable approach.

`picking.rs` is a DDA-style voxel raycaster fed by `cursor_ray`. `pick` returns the hit cell and surface normal so `Tool::Brush` can place into `hit.cell + hit.normal`. The DDA caps at `MAX_DDA_STEPS = 1024` so an open-air ray terminates in an empty world; it also exits early when the cell drops below the floor going further down. When the ray misses every voxel it falls back to the floor plane at y=0 (unbounded on X/Z) so brushing on empty space still works.

### File I/O — async dialogs are mandatory

Synchronous `rfd::FileDialog` calls **block winit's event loop on macOS** (spinning beachball). All save/open/export buttons in `ui.rs` (and the macOS menu) go through `PendingDialog`:

1. Button click → `pending.spawn(async move { rfd::AsyncFileDialog... })` on `AsyncComputeTaskPool`.
2. `poll_dialogs_system` (registered in `Update`) calls `block_on(future::poll_once(task))` each frame.
3. On `Some(DialogResult::*)`, dispatch to `io::project::{save,load}` / `io::vox::{import,export}` / `io::qb::import` / `io::gox::{import,export}` / `io::obj::export` / `io::fbx::export` / `io::gltf::export` / `io::svg::export` / `io::ase::{import,export}` (PNG goes through `snapshot.rs` which spawns a transparent-clear render pass).

`io::fbx::export` writes binary FBX 7.4 (Geometry + Model + Connections + Definitions + GlobalSettings + footer with the canonical magic). Per-face quads with vertex colors via `LayerElementColor` (`ByPolygonVertex`/`Direct`). Y-up to match Bevy and Blender's default importer expectations. The ASCII variant accepted by Maya / 3ds Max / Unity but not Blender was the first attempt — do not revive it without a reason.

`io::gltf::export` writes glTF 2.0 binary (`.glb`): 12-byte header + JSON chunk + BIN chunk, all 4-byte aligned. Indexed triangle mesh, per-vertex `COLOR_0` as u8-normalized RGBA, Y-up (glTF spec default — Unity and Godot import upright with no extra transform). Per-face quads share the iteration path with FBX through `mesh::for_each_exposed_face`; do not reintroduce a separate grid-walk loop.

Foreign-tool axis handling: MagicaVoxel `.vox` and Goxel `.gox` are Z-up; both importer + exporter remap `(x, y_roxel, z_roxel) ↔ (x_vox, z_vox, y_vox)` so foreign files load upright and Roxel-exported files open upright in the target tool. Qubicle `.qb` is Y-up natively — no remap. `.gox` BL16 blocks are written as raw 16³ RGBA bytes; PNG-encoded BL16 (used by current Goxel versions) is rejected with a clear error rather than silently misparsing.

**AABB-shift on export for unsigned-coord formats.** `.vox` and `.qb` use unsigned coordinates starting at origin; the open-world grid can carry negative coords. On export, compute `grid.bounding_box()` and translate every emitted voxel by `-min` so the model's min corner lands at (0, 0, 0) in the target format. The axis remap (Z-up for `.vox`) applies after the shift. `.vox` additionally refuses export when any axis extent exceeds 256 (format cap — coords are stored in `u8`); user gets a toast. `.gox` and the mesh-based formats (`.obj`, `.fbx`, `.gltf`, `.svg`) handle negative coords natively and write them as-is.

Imports just `grid.set` each voxel at its source coordinate. There is no grid-resize step, no `snap_to_allowed_size` — the open-world grid will accept any IVec3 with `y >= 0`. `apply_import_system` (`main.rs`) only consumes the `PendingImport` flag now; it does not rebuild floor/walls (there's no walls, and the floor follows the camera). If the user wants to re-frame after import, they press Cmd+0.

Shared io helpers (use them; don't reroll):

- `crate::grid::iter_occupied()` — yields `(IVec3, Color8)` for every occupied cell across all loaded chunks. Use this in every exporter instead of nested loops.
- `crate::grid::bounding_box()` — `Option<(min, max)>` (inclusive) over occupied cells. Used by camera fit, AABB-shift exports, and the design-size footer readout.
- `crate::mesh::for_each_exposed_face(grid, |cell, face, rgba| ...)` — per-face quad iteration with occlusion culling, walks `iter_occupied`. Shared by `fbx::build_mesh` + `gltf::build_mesh`.
- `crate::io::reader::LeReader` — bounds-checked little-endian binary reader. Shared by `qb::import` + `gox::import`.
- `crate::io::test_util::tmp_path(name, ext)` — `#[cfg(test)]` helper that produces a unique temp-file path for io tests.

Buttons are disabled while `pending.is_active()` so only one dialog runs at a time. **Never** reintroduce sync `rfd::FileDialog::*` calls inside egui draw code.

`io::palettes` persists user-created palettes (only — built-ins are filtered out by `encode`) to `dirs::config_dir()/roxel/palettes.ron` as `Vec<StoredPalette>`. `Palettes::with_user_loaded()` appends them to the built-ins on startup. Any UI action that mutates a user palette must call `io::palettes::save(...)` after — there's no autosave system.

### macOS menu bar

`menu.rs` is gated by `#[cfg(target_os = "macos")]` and installs a native `muda` menu (App / File / Edit submenus with accelerators). It runs three `Update` systems chained `after` each other: `install_menu_system` (one-shot, sets up the menu and stores a `MenuStore` as a non-send resource), `poll_menu_events_system` (drains `MenuEvent` channel into `MenuQueue`), and `apply_menu_actions_system` (translates `MenuAction` variants into `PendingDialog` spawns, `History::undo/redo` calls, `NewProject.dialog_open = true`, etc.). `update_menu_enabled_system` greys out Undo/Redo when their stacks are empty.

The menu mirrors `ui.rs`'s File/Edit buttons — when adding a new dialog-driven action, wire it into both unless the action is mac-only.

### New-project flow

`NewProject { dialog_open, apply: bool }` (`grid.rs`) drives a confirm-only modal — there is no grid size to pick. On confirm, `apply` is set to `true`; `apply_new_project_system` (`main.rs`) consumes it the next frame to `grid.clear()` (drains every chunk into `dirty_chunks` so the mesher despawns their entities), clear history, drain `VoxelChunkMeshes.chunks` (despawning any leftover entities the mesher hasn't reached yet), and reset the camera to origin / `EMPTY_WORLD_RADIUS`. Don't poke `VoxelGrid` directly from UI code — go through `NewProject.apply` so the camera + history stay consistent.

### UI structure

`apply_egui_style` runs every frame at the top of `ui_system` using the current `Theme` resource. `ui_system` runs in the `EguiPrimaryContextPass` schedule (not `Update`) and lays out four panels: top bar (file/edit + Preferences button on the right), bottom status bar (right-aligned: `Design WxHxD` from `grid.bounding_box()` — em-dash when empty — plus voxel count and `Zoom N voxels` from the orbit radius), left tool rail, right inspector (color swatch + popup picker, palette selector with built-ins + user palettes + add/new/dup/rename/delete + drag-reorder, `.ase` import/export, recent colors, shape options, scene stats). The active palette lives in `Palettes` (resource) indexed by `PaletteChoice`; both are `init_resource`'d / `insert_resource`'d (`Palettes::with_user_loaded`) in `main.rs`.

Sections in the inspector are flat: bold title, then content, then a thin full-width divider — no card frames. The divider spans the full panel width by painting at `ui.clip_rect().x_range()` rather than `ui.available_width()`. Side-panel left/right edges are drawn as a single 0.5-px vline via `ctx.layer_painter(LayerId::new(Order::Middle, …))` so popups (Foreground) draw over them; the panel `egui::Frame` itself has no stroke.

`tool_button`, the big color swatch, palette swatches, and recent swatches use `egui::Button` wrapped in a `ui.scope` that zeroes `spacing.button_padding` and `spacing.interact_size`. This keeps them at their exact requested size while letting egui's AA tessellator render the rounded fills cleanly (the manual-painter version produced jaggies on Retina displays).

Egui labels disable text selection except on numeric values in the stats panel (so users can copy a count or hex without dragging the whole label).

### Design tokens

Every spacing, padding, corner radius, font size, icon size, swatch size, and stroke width in the UI must resolve to a value in `src/ui/tokens.rs` — not an inline literal. Submodules: `font` (SMALL/BODY/HEADING, no font under 12 pt), `radius` (XS/SM/MD/LG as `u8` for `CornerRadius::same`), `space` (scalar `f32` for `ui.add_space`), `gap` (Vec2 for `item_spacing`), `pad` (Vec2 for `button_padding`), `icon` (square sizes + `*_square()` helpers), `swatch` (recent/palette/hero), `stroke` (HAIR/NORMAL/ACCENT).

All values land on a 4-px grid and are even. The token guard tests in `tokens::tests` enforce this — if you add a new constant, keep it even and ≥ 12 pt for fonts, or extend the guards explicitly. **Never inline a literal radius, padding, gap, or font size in a UI call site.** If no token fits, add one rather than hardcoding. Colors stay in `Theme` (`theme.rs`) — they swap with theme mode, so they don't belong with the static tokens.

### UI widget helpers

Reusable egui widget helpers live in `src/ui/widgets.rs`:

- Structural: `section` (titled block + full-width divider), `prefs_row` (settings-modal label + content row), `modal_window` (centred themed `egui::Window` builder used by Preferences + New-project), `swatch_grid` (zero-padding `horizontal_wrapped` for swatch rows), `vertical_rule`.
- Buttons: `tool_button` (left rail), `icon_button` (top bar text + icon), `icon_only_button` (palette toolbar), `wide_action_button` ("Add current color" style full-width row), `dialog_button` (modal Create/Cancel rows; `primary` = accent fill), `chip_button` (generic selectable toggle, used by Theme: System/Light/Dark), `swatch_button` (foreground/palette/recent colour squares).
- Labels: `stat_row` (label + right-aligned monospace value), `hint_label` (dim italic body text), `status_label` (status-bar readout), `hex_label` / `hex_string` (canonical `#RRGGBB` rendering), `tool_label` (`Tool` → display string), `plane_color_row` (radio + custom-colour pref row).

**Prefer these over hand-rolling new one-off styles.** When adding UI, reach for an existing helper first. Only introduce a new inline pattern if no helper fits and the shape is genuinely single-use; if a second call site appears, promote it to `widgets.rs` rather than copying. Helpers own the `ui.scope` + `spacing_mut` boilerplate, the themed strokes/fills, the corner radii, and the title font selection — duplicating those inline drifts the look over time. The same rule applies to modal frames (`modal_window`) and section dividers (`section`): never reach for `egui::Window::new` or hand-painted hlines directly.

### Toast notifications

User-facing success/error feedback goes through `crate::ui::toast::Toasts` — a `Resource` holding a capped `VecDeque<Toast>` (max 4 visible, oldest evicted). Call sites use `toasts.success(msg)` / `toasts.error(msg)` / `toasts.info(msg)`; `toast_lifetime_system` ticks each toast's `remaining` field down by `time.delta_secs()` and removes expired ones. Success TTL is 3.5 s, error 6 s (errors linger for readability).

`draw_toasts` runs last in `ui_system` and anchors the stack to **bottom-center of the canvas** (`ctx.available_rect()` after all panels have been registered), pivot `CENTER_BOTTOM`, so newest toast sits closest to the action and the stack grows upward without colliding with the status bar.

**Never reintroduce `eprintln!` for user-facing I/O errors.** All save/open/export/import paths in `ui/dialogs.rs` (and `snapshot.rs`'s PNG observer) emit toasts; terminal output is invisible to packaged-app users. Internal diagnostics (dropped-voxel counts, multi-model warnings) can still go to stderr — those aren't actionable.

### Theme + Preferences

`Theme` (`theme.rs`) is a `Resource` carrying every egui color slot (bg / panel / surface / surface_hover / accent / accent_dim / text / text_dim / border / faint) plus a `mode: ThemeMode::{Light, Dark}` discriminator. `Theme::dark()` (UI bg `#191A2E`) and `Theme::light()` are the two presets.

`Preferences` (`theme.rs`) carries:
- `theme: ThemePref { Light, Dark, System }`
- `canvas_bg: CanvasBgPref { MatchTheme, Custom([u8; 3]) }` — viewport clear color. `MatchTheme` resolves to a near-neutral grey (`canvas_match_color`) rather than the bluish UI panel bg, so voxel hues read truly.
- `show_floor_grid: bool` (default true) — Minecraft-style grid lines on the y=0 plane.
- `show_origin_axes: bool` (default true) — RGB axis triad at world origin (red X, green Y, blue Z). When false, the entire triad is hidden including the long Y extension.
- `show_y_axis: bool` (default true) — extends the green Y-axis up into the sky as a vertical origin anchor. Gated by `show_origin_axes`.

Every field after `theme` is `#[serde(default = "...")]` so older `preferences.ron` files load without wiping user state. **Keep that invariant**: any new field must have a `#[serde(default)]` provider; otherwise pre-existing prefs files become unparseable and silently revert to `Default`. Removed fields (`show_floor`, `floor_color`, `show_walls`, `wall_color`, `preview_outline`) are silently dropped at load time — ron ignores unknown struct fields. `theme::tests::preferences_loads_after_floor_fields_removed` guards this.

`Preferences` is loaded on startup via `load_preferences()` and saved via `save_preferences()` whenever the user changes a value in the Preferences modal. The file lives at `dirs::config_dir()/roxel/preferences.ron`. Theme + pref changes propagate through `refresh_theme_system` (every frame, `NonSendMarker` for main-thread `WINIT_WINDOWS` access — resolves `ThemePref::System` against `winit::Window::theme()`) and `apply_canvas_bg_system` in `main.rs`, which diffs before writing so we don't dirty assets every frame.

### Grid + origin

There is no floor plane. `floor_grid_system` (`main.rs`) draws the y=0 grid as immediate-mode `Gizmos` in a 3×-orbit-radius window centered on the camera focus. Two LOD bands: per-voxel lines (with every-16 major) up to `GRID_VOXEL_RADIUS = 128`, every-16 chunk lines only up to `GRID_CHUNK_RADIUS = 512`, hidden beyond. Gated on `show_floor_grid`.

`draw_origin_system` draws an RGB axis triad at (0, 0, 0) through a dedicated `OriginAxesGizmos` gizmo group (configured in `configure_origin_axes_gizmos` with `depth_bias = -1.0` so the triad reads cleanly on top of the floor grid where both sit at y≈0). Gated on `show_origin_axes`. The green Y leg extends 10,000 units up into the sky when `show_y_axis` is also set so the user never loses the origin column.

### Fonts

`install_fonts` (`theme.rs`) registers five static TTFs embedded via `include_bytes!`:

- Nunito 400 / 500 / 600 / 700 (proportional default = 400; named families `Nunito500` / `Nunito600` / `Nunito700` for explicit weights)
- DM Mono 400 (monospace default; used by `.monospace()` RichText for hex codes and stat values)

For real bold (not faux), reference `egui::FontFamily::Name(NUNITO_700_FAMILY.into())` instead of `.strong()` — `.strong()` only adds an extra stroke pass, it doesn't switch font family.

**Critical scheduling**: `font_setup` runs in `PreUpdate` between `EguiPreUpdateSet::InitContexts` and `EguiPreUpdateSet::BeginPass`. `Context::set_fonts` only takes effect on the next `begin_pass`, so if fonts are installed inside `EguiPrimaryContextPass` (which is after `begin_pass`), the first frame will panic with `"FontFamily::Name(\"Nunito700\") is not bound to any fonts"`.

### App icon

The icon lives in `assets/icons/`: `roxel.svg` (source), `roxel-256.png` (embedded via `include_bytes!`), `roxel.icns` (for bundling), and the `roxel.iconset/` directory of PNGs used to build the `.icns`.

`set_window_icon` (`icon.rs`) reads the embedded PNG and applies it two ways:
1. `winit::Window::set_window_icon` — works on Windows/Linux, **no-ops on macOS** for unbundled binaries.
2. `NSApplication::setApplicationIconImage` (via `objc2` + `objc2-app-kit`) — this is the only way to get a dock icon for `cargo run --release` on macOS.

The system accesses `WINIT_WINDOWS` via the thread-local `bevy::winit::WINIT_WINDOWS` (it's not a regular `NonSend` resource in Bevy 0.18). It must run on the main thread, enforced by a `NonSendMarker` system param.

For packaged builds, `[package.metadata.bundle]` in `Cargo.toml` points `cargo-bundle` at `roxel.icns`.

### Camera

`spawn_camera` (`camera.rs`) places the orbit camera at the isometric direction `(1, 1, 1).normalize() * EMPTY_WORLD_RADIUS` so a fresh project doesn't open looking down a single axis. `EMPTY_WORLD_RADIUS = 32.0` is the spawn radius and the empty-world fallback used everywhere a fit-view-style answer is required (frame-view on empty, zoom-limits on empty, new-project reset).

`frame_view_system` (`Cmd/Ctrl+0`) computes the AABB of all occupied voxels via `grid.bounding_box()` and is **panel-aware** — it uses `ViewportRect` (the egui-occupied rect, updated `after(ui_system)`) to fit the cluster inside the visible viewport, not the full window. On an empty scene it falls back to focus=origin, radius=`EMPTY_WORLD_RADIUS`. The bottom-bar zoom readout uses `cam.radius` (the current smoothed value, not `target_radius`) rounded to a voxel count (`Zoom N voxels`) — reading the target lied during long lerps from huge radii.

`zoom_click_system` is wired through `KeyZ` + LMB-just-pressed: multiplies `target_radius` by `ZOOM_STEP_IN = 1/√2` (zoom in) or `ZOOM_STEP_OUT = √2` (zoom out, when Alt is also held), clamped to `[zoom_lower_limit, zoom_upper_limit]`. The click also recenters `target_focus` to whatever's under the cursor (picked voxel, or floor plane) so the zoom converges on the user's point of interest. `tool_input_system` early-returns while `Z` is held so the click doesn't also paint.

`FlybyState { active, t }` drives the auto-orbit "drone" view. `flyby_system` (`camera.rs`) overwrites `target_yaw` / `target_pitch` / `target_radius` (and pins `target_focus` to `fit_view`'s centroid) every frame from pure parametric path fns (`flyby_yaw`, `flyby_pitch`, `flyby_radius`). PanOrbitCamera has no public `enabled: bool`, so clobbering the targets each tick is what lets the cinematic win over user RMB-drag without modifying the crate. Painting is gated separately (`tool_input_system` early-returns and aborts any in-flight stroke when `flyby.active`); brush + shape previews hide. Esc or a second palette toggle ends the flyby; mouse input does NOT cancel (so screen recordings aren't ruined by accidental clicks). Tunables `FLYBY_*` live in `camera.rs`.

`zoom_radius_limits` derives the camera's allowed radius range from the current scene. Lower bound is fixed at `ZOOM_LOWER_LIMIT = 8.0` (independent of cluster size, so big scenes still allow close inspection); upper bound is `max(fit_radius * ZOOM_OUT_MULTIPLIER, ZOOM_OUT_FLOOR)` = `max(fit * 2, 64)` so empty scenes stay orbit-able and big scenes don't fly off into void.

### Cursor hints

`ui_system` updates `egui::CursorIcon` each frame based on the active modifier (checked only when the pointer is not over an egui area):

| Condition | Cursor |
|-----------|--------|
| RMB held (orbit) | `Grabbing` |
| Gizmo dragged | `Grabbing` |
| Gizmo hovered | `Grab` |
| `Z` held (no Alt) | `ZoomIn` |
| `Alt` + `Z` held | `ZoomOut` |
| `Space` held (LMB up) | `Grab` |
| `Space` + LMB held | `Grabbing` |
| `Alt` held alone | `PointingHand` (sticky eyedropper) |
| otherwise | `Crosshair` |

### Gizmo overlay

`gizmo.rs` runs a second `Camera3d` on `RenderLayers::layer(1)` with `clear_color: None`, drawing an orientation cube into a viewport rect computed by `update_gizmo_viewport` (scheduled `after(ui_system)` so it sees the final egui-occupied area). `GizmoRect`, `GizmoDrag`, and `GizmoHover` resources are read by `tool_input_system` to suppress tool clicks over the gizmo, by the preview systems to hide while dragging, and by `ui_system` to switch the cursor to grab/grabbing.

### Bevy plugin / resource registration

`EguiPlugin` is added with `auto_create_primary_context: false` (set via `EguiGlobalSettings`); the gizmo's secondary camera is what makes this necessary. If you add a new resource consumed by a system, register it with `init_resource` in `main.rs` — most resources here are `#[derive(Default)]` and use that pattern. `Palettes` is the exception (registered with `insert_resource(Palettes::with_user_loaded())` so user palettes load on startup).

## File format

### Snapshot (PNG export)

`snapshot.rs` spawns a one-shot offscreen `Camera3d` at the main camera's transform/projection, renders into an `Image` at physical window resolution with `ClearColorConfig::Custom(Color::NONE)`, and captures it via Bevy's `Screenshot` pipeline. The snapshot camera forces `Tonemapping::None` — the tonemap fullscreen pass writes `alpha=1` to every output pixel, which would clobber the transparent clear and produce an opaque-black background. Voxel materials are `unlit` so colors are unaffected by skipping tonemap.

While the snapshot is in flight, `SnapshotInProgress` is set. `floor_grid_system`, `draw_origin_system`, and `selection_render_system` early-return on the snapshot frame so gizmo overlays don't appear in the captured image. `start_snapshot_system` is ordered `.before(...)` each of those so they see the flag on the same frame. The observer that runs after `ScreenshotCaptured` clears the flag.

`.roxel` projects are `ron`-serialized `ProjectFile { voxels: Vec<([i32; 3], Color8)> }`. Only occupied cells are stored. No `version`, no `size` — the open-world grid has neither. Coordinates are signed; a model can sit anywhere relative to the origin and round-trip exactly.

The previous (`version`, `size`, `voxels`) layout was dropped wholesale. ron's struct deserializer silently ignores unknown fields, so an older v1 file happens to load if its `voxels` field matches — the extra `version`/`size` are dropped on the floor. This is not a compat code path; it's a side effect of lax deserialization. Do not add explicit v1 handling.
