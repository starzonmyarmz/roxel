# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

- `cargo run` — launch the editor (dev profile; opt-level=1 for the crate, 3 for deps via `[profile.dev.package."*"]`)
- `cargo run --release` — release build, slow to compile, fast at runtime
- `cargo check` — fast type/borrow check; use this in iteration, not `cargo build`
- `cargo test` — unit tests (inline `#[cfg(test)] mod tests` per source file)
- `cargo fmt` / `cargo clippy` — standard Rust toolchain

## Tests

Tests live as inline `#[cfg(test)] mod tests` blocks at the bottom of each `src/*.rs` module — there is no lib target and no `tests/` directory. Coverage focuses on pure logic: `grid` (bounds/get/set/clear/resize/chunk-dirty propagation), `history` (record/undo/redo/dedup/cap), `shapes` (rect/ellipse/line2d/extrude), `picking` (DDA raycaster + floor fallback), `mesh` (sRGB roundtrip, greedy quad counts, chunked vs monolithic equivalence), `theme` (canvas/plane resolution + serde back-compat for older `preferences.ron`), `io::project` (save/load roundtrip), `io::palettes` (user palette roundtrip, builtins not persisted). Avoid spinning up a Bevy `App` in tests — exercise the pure functions instead. File-IO tests use `std::env::temp_dir()`; do not add `tempfile` as a dep.

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

`VoxelGrid` (`grid.rs`) is the single source of truth. Storage is always allocated at `MAX_GRID = 128` (a flat `Box<[Option<Color8>]>` of 128³ cells, ~8 MB) so resize never reallocates. The active edit box is `VoxelGrid.size` (one of `ALLOWED_SIZES = [32, 64, 96, 128]`, default 32); `in_bounds`/`get`/`set` reject cells outside `[0, size)` even though the underlying storage extends to 128. `set` flips a global `dirty: bool` *and* marks the owning chunk's flag in `chunk_dirty` (a `Box<[bool]>` indexed by `chunk_flat_idx`). Cells on a chunk boundary also flag the neighbour across the seam — face occlusion changes there too.

`CHUNK = 32`, `MAX_CHUNKS_PER_AXIS = 4`. Every `ALLOWED_SIZES` value divides `CHUNK` evenly so the chunked mesher needs no partial-chunk handling. If you add a new allowed size, keep that invariant or the mesher will under-cover the grid.

Mutations flow through `History::record` (`history.rs`), **not** `grid.set` directly. Recording wraps each `grid.set` in a `CellDelta`, dedupes per stroke via a `HashSet<(i32,i32,i32)>`, and appends to `History.current`. `History::begin` / `History::end` bracket a stroke; the LMB-release path in `tool_input_system` is responsible for calling `end`. The undo stack is capped at `MAX_UNDO = 200`; pushing a new stroke clears the redo stack.

`regenerate_mesh_system` (`mesh.rs`) rebuilds only chunks whose `chunk_dirty` flag is set, into the per-chunk `Mesh` handles held in `VoxelChunkMeshes`. Each chunk owns its own `VoxelMesh` entity (spawned once in `setup_scene` for all `MAX_CHUNKS_PER_AXIS³ = 64` slots). The mesher is greedy: `greedy_quads_bounded` merges same-color same-direction faces inside a half-open `[min, max)` box while still querying the full grid for cross-bounds occlusion so emitted quads agree at chunk seams with the monolithic `greedy_quads` reference path. Touching `grid` outside `History::record` still works but bypasses undo, so prefer the history path. Base colors are run through `srgb_to_linear` before upload — Bevy's pipeline expects linear vertex colors.

The mesher also consults `PreviewHide` (`mesh.rs`): when the erase preview is active, the targeted cell is filtered out of `build_mesh` so the voxel-to-be-removed visually disappears under the cursor. A change to `PreviewHide.cell` re-triggers a mesh rebuild for that chunk even when `grid.dirty` is false. `brush_preview_system` (`preview.rs`) drives the ghost cuboid for `Tool::Brush` and the `PreviewHide` cell for `Tool::Erase`; `shape_preview_system` (`shape_preview.rs`) drives the ghost mesh for `Tool::Shape`. Both are scheduled `before(regenerate_mesh_system)` so the mesher sees the current frame's hide state.

Both previews hide while the user is orbiting (RMB), dragging the gizmo, or already mid-stroke — checked via mouse state, `GizmoDrag.active`, and `PointerState.stroking`. Don't reintroduce a preview that flashes during orbit; it's distracting and was deliberately removed.

### Tools and input

`tool_input_system` is the central tool dispatcher. It early-returns when egui wants the pointer (`is_pointer_over_area` or `wants_pointer_input`) or when the gizmo viewport rect contains the cursor — these gates exist to prevent painting through UI. `Tool::Eyedropper` is single-click; it auto-restores `tool.previous` on release **unless** Alt is held (Alt keeps the eyedropper sticky for repeated picks, driven by `alt_eyedropper_system`). The other tools are stroke-based and rely on `PointerState.stroking` plus `history.begin/record/end`.

`Tool::Shape` (rectangle / ellipse / line, configurable via `ShapeOptions`) is two-click: first click anchors corner1 on the picked face, drag previews the 2D footprint, second click commits and lets the user drag along the face normal to extrude (`shapes::extrude` lifts the 2D cell set into a 3D box / cylinder / line column). The shape's in-progress state lives in `ShapeState`; `tool_input_system` checks `state.phase` to route input through the rectangle/ellipse/line cell generators in `shapes.rs`.

`Tool::Select` (`select.rs`) is the same two-phase face-plane drag as Shape but commits an `AABB` into `Selection` instead of writing voxels. `selection_render_system` draws the live AABB outline + filled-cell overlay; `selection_key_action_system` wires Backspace/Delete to `clear_aabb` and Esc to clear the selection.

`Tool::Move` (`select.rs` + `move_drag_system` in `tools.rs`) translates the contents of the active selection by integer cell offsets. Two input paths, both share `MoveDragState` (mouse) and the pure `select::move_selection` helper:
- **Mouse drag** — `move_drag_system` runs as its own `Update` system. LMB-press on a voxel inside the selection anchors a face plane (same `StrokeAnchor` machinery as Shape/Select); cursor motion projects to that plane and snaps to integer cells via `constrain_move_delta`. The drag locks the face-normal axis; Shift also locks Y so the move stays on the same horizontal plane. A move that would overlap a non-source occupied cell is refused, leaving the selection at the last valid delta. `history.abort` rolls partial writes back when the user RMB / Esc / switches tool mid-drag. Click on a bare voxel with no active selection creates an ad-hoc 1×1×1 selection that is cleared on release.
- **Arrow keys** — `move_selection_keys_system` calls `move_selection` once per `just_pressed`. ←/→ = ∓X, ↑/↓ = ∓Z, Shift+↑/↓ = ±Y. Collisions and out-of-bounds shifts are rejected.

Both paths record exactly one history stroke per commit. Mid-drag, frames re-record the same touched cells repeatedly; `History::record` dedupes by overwriting the existing delta's `after` value so the final stroke contains one entry per cell regardless of how many frames the drag spanned.

Three pieces of stroke state in `PointerState` matter for non-shape tools:
- `anchor` (`StrokeAnchor`) — locks the build plane axis for the duration of a drag so the picker can't slide onto a perpendicular face mid-stroke.
- `snapshot: Option<VoxelGrid>` — pre-stroke clone of the grid. Ray-picks during a stroke run against this snapshot, not the live grid. Voxels placed earlier in the same stroke are invisible to the picker, which is what kills runaway stacking. Pattern lifted from goxel; `VoxelGrid` derives `Clone` specifically for this. Note: cloning 8 MB per stroke is acceptable at current scale; if `MAX_GRID` grows, revisit.
- `last_placed: Option<IVec3>` — endpoint for `line3d` (3D Bresenham). Fills gaps when the cursor jumps between frames, and `Shift+click` runs a one-shot `line3d` stroke from `last_placed` to the new target without entering drag mode.

`picking.rs` is a DDA-style voxel raycaster fed by `cursor_ray`. `pick` returns the hit cell and surface normal so `Tool::Brush` can place into `hit.cell + hit.normal`. When the ray misses every voxel it falls back to the floor plane at y=0 so brushing on empty space still works.

### File I/O — async dialogs are mandatory

Synchronous `rfd::FileDialog` calls **block winit's event loop on macOS** (spinning beachball). All save/open/export buttons in `ui.rs` (and the macOS menu) go through `PendingDialog`:

1. Button click → `pending.spawn(async move { rfd::AsyncFileDialog... })` on `AsyncComputeTaskPool`.
2. `poll_dialogs_system` (registered in `Update`) calls `block_on(future::poll_once(task))` each frame.
3. On `Some(DialogResult::*)`, dispatch to `io::project::{save,load}` / `io::vox::export` / `io::obj::export` / `io::fbx::export` / `io::svg::export` / `io::ase::{import,export}` (PNG goes through `snapshot.rs` which spawns a transparent-clear render pass).

`io::fbx::export` writes binary FBX 7.4 (Geometry + Model + Connections + Definitions + GlobalSettings + footer with the canonical magic). Per-face quads with vertex colors via `LayerElementColor` (`ByPolygonVertex`/`Direct`). Y-up to match Bevy and Blender's default importer expectations. The ASCII variant accepted by Maya / 3ds Max / Unity but not Blender was the first attempt — do not revive it without a reason.

Buttons are disabled while `pending.is_active()` so only one dialog runs at a time. **Never** reintroduce sync `rfd::FileDialog::*` calls inside egui draw code.

`io::palettes` persists user-created palettes (only — built-ins are filtered out by `encode`) to `dirs::config_dir()/roxel/palettes.ron` as `Vec<StoredPalette>`. `Palettes::with_user_loaded()` appends them to the built-ins on startup. Any UI action that mutates a user palette must call `io::palettes::save(...)` after — there's no autosave system.

### macOS menu bar

`menu.rs` is gated by `#[cfg(target_os = "macos")]` and installs a native `muda` menu (App / File / Edit submenus with accelerators). It runs three `Update` systems chained `after` each other: `install_menu_system` (one-shot, sets up the menu and stores a `MenuStore` as a non-send resource), `poll_menu_events_system` (drains `MenuEvent` channel into `MenuQueue`), and `apply_menu_actions_system` (translates `MenuAction` variants into `PendingDialog` spawns, `History::undo/redo` calls, `NewProject.dialog_open = true`, etc.). `update_menu_enabled_system` greys out Undo/Redo when their stacks are empty.

The menu mirrors `ui.rs`'s File/Edit buttons — when adding a new dialog-driven action, wire it into both unless the action is mac-only.

### New-project flow

The user picks a size in the egui modal driven by `NewProject { dialog_open, picker_size, apply }` (`grid.rs`). On confirm, `apply` is set to `Some(size)`; `apply_new_project_system` (`main.rs`) consumes it the next frame to call `grid.resize`, clear history, replace the floor + wall meshes with new sizes, and recenter the `PanOrbitCamera`. Don't poke `VoxelGrid.size` directly from UI code — go through `NewProject.apply` so the camera + planes stay consistent.

### UI structure

`apply_egui_style` runs every frame at the top of `ui_system` using the current `Theme` resource. `ui_system` runs in the `EguiPrimaryContextPass` schedule (not `Update`) and lays out four panels: top bar (file/edit + Preferences button on the right), bottom status bar (right-aligned voxel/grid/zoom stats), left tool rail, right inspector (color swatch + popup picker, palette selector with built-ins + user palettes + add/new/dup/rename/delete + drag-reorder, `.ase` import/export, recent colors, shape options, scene stats). The active palette lives in `Palettes` (resource) indexed by `PaletteChoice`; both are `init_resource`'d / `insert_resource`'d (`Palettes::with_user_loaded`) in `main.rs`.

Sections in the inspector are flat: bold title, then content, then a thin full-width divider — no card frames. The divider spans the full panel width by painting at `ui.clip_rect().x_range()` rather than `ui.available_width()`. Side-panel left/right edges are drawn as a single 0.5-px vline via `ctx.layer_painter(LayerId::new(Order::Middle, …))` so popups (Foreground) draw over them; the panel `egui::Frame` itself has no stroke.

`tool_button`, the big color swatch, palette swatches, and recent swatches use `egui::Button` wrapped in a `ui.scope` that zeroes `spacing.button_padding` and `spacing.interact_size`. This keeps them at their exact requested size while letting egui's AA tessellator render the rounded fills cleanly (the manual-painter version produced jaggies on Retina displays).

Egui labels disable text selection except on numeric values in the stats panel (so users can copy a count or hex without dragging the whole label).

### Theme + Preferences

`Theme` (`theme.rs`) is a `Resource` carrying every egui color slot (bg / panel / surface / surface_hover / accent / accent_dim / text / text_dim / border / faint) plus a `mode: ThemeMode::{Light, Dark}` discriminator. `Theme::dark()` (UI bg `#191A2E`) and `Theme::light()` are the two presets.

`Preferences` (`theme.rs`) carries:
- `theme: ThemePref { Light, Dark, System }`
- `canvas_bg: CanvasBgPref { MatchTheme, Custom([u8; 3]) }` — viewport clear color. `MatchTheme` resolves to a near-neutral grey (`canvas_match_color`) rather than the bluish UI panel bg, so voxel hues read truly.
- `floor_color`, `wall_color: PlaneColorPref` — same shape as canvas. `MatchTheme` resolves to a luminance-shifted neutral (`plane_match_color`) so floor/walls read as distinct surfaces against the canvas without tinting voxels.
- `show_floor: bool` (default true), `show_walls: bool` (default false) — toggle the floor + back/left wall planes.
- `preview_outline: bool` (default true) — draws a 1.01-scale contrast-aware gizmo cube around the brush/shape preview.

Every field after `theme` is `#[serde(default = "...")]` so older `preferences.ron` files load without wiping user state. **Keep that invariant**: any new field must have a `#[serde(default)]` provider; otherwise pre-existing prefs files become unparseable and silently revert to `Default`. The `floor_color` field also aliases the older `plane_color` name for the same reason. `theme::tests::preferences_loads_with_missing_new_fields` guards this.

`Preferences` is loaded on startup via `load_preferences()` and saved via `save_preferences()` whenever the user changes a value in the Preferences modal. The file lives at `dirs::config_dir()/roxel/preferences.ron`. Theme + pref changes propagate through `refresh_theme_system` (every frame, `NonSendMarker` for main-thread `WINIT_WINDOWS` access — resolves `ThemePref::System` against `winit::Window::theme()`) and the `apply_canvas_bg_system` / `apply_floor_color_system` / `apply_wall_color_system` / `apply_floor_visibility_system` / `apply_walls_visibility_system` systems in `main.rs`, which diff before writing so we don't dirty assets every frame.

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

`spawn_camera` (`camera.rs`) places the orbit camera at an isometric angle so a fresh project doesn't open looking down a single axis. `frame_view_system` (`Cmd/Ctrl+0`) computes the AABB of all occupied voxels and is **panel-aware** — it uses `ViewportRect` (the egui-occupied rect, updated `after(ui_system)`) to fit the cluster inside the visible viewport, not the full window. The bottom-bar zoom % readout is derived from the same `target_radius` that `zoom_click_system` mutates.

`zoom_click_system` is wired through `KeyZ` + LMB-just-pressed: halves (zoom in) or doubles (zoom out, when Alt is also held) `PanOrbitCamera.target_radius`, clamped to `zoom_lower_limit`. `tool_input_system` early-returns while `Z` is held so the click doesn't also paint.

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

`.roxel` projects are `ron`-serialized `ProjectFile { version: u32, size: [u32; 3], voxels: Vec<([i32; 3], Color8)> }`. Only occupied cells are stored. `version` is currently `1` and unchecked on load — bump and gate if the schema changes. The `size` field is written as the current `VoxelGrid.size`; load resets `VoxelGrid.size` accordingly so saving a 32³ project and loading it back doesn't blow it up to 128³.
