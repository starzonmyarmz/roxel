# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

- `cargo run` — launch the editor (dev profile; opt-level=1 for the crate, 3 for deps via `[profile.dev.package."*"]`)
- `cargo run --release` — release build, slow to compile, fast at runtime
- `cargo check` — fast type/borrow check; use this in iteration, not `cargo build`
- `cargo fmt` / `cargo clippy` — standard Rust toolchain

There are no tests in this crate.

## Architecture

Roxel is a single-window Bevy 0.18 app with a `bevy_egui` UI overlay and a `bevy_panorbit_camera` viewport camera. Everything is in one binary (`src/main.rs`) — no library crate, no workspace.

### Data flow

`VoxelGrid` (`grid.rs`) is the single source of truth: a heap-allocated `Box<[[[Option<Color8>; 64]; 64]; 64]>` resource. It carries a `dirty: bool` flag.

Mutations flow through `History::record` (`history.rs`), **not** `grid.set` directly. Recording wraps each `grid.set` in a `CellDelta`, dedupes per stroke via a `HashSet<(i32,i32,i32)>`, and appends to `History.current`. `History::begin` / `History::end` bracket a stroke; the LMB-release path in `tool_input_system` is responsible for calling `end`. The undo stack is capped at `MAX_UNDO = 200`; pushing a new stroke clears the redo stack.

`regenerate_mesh_system` (`mesh.rs`) polls `grid.dirty` each frame, rebuilds the cuboid mesh from scratch (greedy meshing is **not** implemented — each visible face is a quad), and clears the flag. Touching `grid` outside `History::record` still works but bypasses undo, so prefer the history path. Base colors are run through `srgb_to_linear` before upload — Bevy's pipeline expects linear vertex colors.

The mesher also consults `PreviewHide` (`mesh.rs`): when the erase preview is active, the targeted cell is filtered out of `build_mesh` so the voxel-to-be-removed visually disappears under the cursor. A change to `PreviewHide.cell` re-triggers mesh rebuild even when `grid.dirty` is false. `brush_preview_system` (`preview.rs`) drives the ghost cuboid for `Tool::Brush` and the `PreviewHide` cell for `Tool::Erase`; it's scheduled `before(regenerate_mesh_system)` so the mesher sees the current frame's hide cell.

### Tools and input

`tool_input_system` is the central tool dispatcher. It early-returns when egui wants the pointer (`is_pointer_over_area` or `wants_pointer_input`) or when the gizmo viewport rect contains the cursor — these gates exist to prevent painting through UI. `Tool::Eyedropper` is single-click; it auto-restores `tool.previous` on release **unless** Alt is held (Alt keeps the eyedropper sticky for repeated picks, driven by `alt_eyedropper_system`). The other tools are stroke-based and rely on `PointerState.stroking` plus `history.begin/record/end`.

Three pieces of stroke state in `PointerState` matter:
- `anchor` (`StrokeAnchor`) — locks the build plane axis for the duration of a drag so the picker can't slide onto a perpendicular face mid-stroke.
- `snapshot: Option<VoxelGrid>` — pre-stroke clone of the grid. Ray-picks during a stroke run against this snapshot, not the live grid. Voxels placed earlier in the same stroke are invisible to the picker, which is what kills runaway stacking. Pattern lifted from goxel; `VoxelGrid` derives `Clone` specifically for this.
- `last_placed: Option<IVec3>` — endpoint for `line3d` (3D Bresenham). Fills gaps when the cursor jumps between frames, and `Shift+click` runs a one-shot `line3d` stroke from `last_placed` to the new target without entering drag mode.

`picking.rs` is a DDA-style voxel raycaster fed by `cursor_ray`. `pick` returns the hit cell and surface normal so `Tool::Brush` can place into `hit.cell + hit.normal`.

### File I/O — async dialogs are mandatory

Synchronous `rfd::FileDialog` calls **block winit's event loop on macOS** (spinning beachball). All save/open/export buttons in `ui.rs` go through `PendingDialog`:

1. Button click → `pending.spawn(async move { rfd::AsyncFileDialog... })` on `AsyncComputeTaskPool`.
2. `poll_dialogs_system` (registered in `Update`) calls `block_on(future::poll_once(task))` each frame.
3. On `Some(DialogResult::*)`, dispatch to `io::project::{save,load}` / `io::vox::export` / `io::obj::export` / `io::ase::{import,export}`.

Buttons are disabled while `pending.is_active()` so only one dialog runs at a time. **Never** reintroduce sync `rfd::FileDialog::*` calls inside egui draw code.

### UI structure

`apply_style` (Startup) sets the dark theme. `ui_system` runs in the `EguiPrimaryContextPass` schedule (not `Update`) and lays out four panels: top bar (file/edit), bottom status bar, left tool rail, right inspector (color swatch + popup picker, palette selector with built-ins + `.ase` import/export, recent colors, scene stats). The active palette lives in `Palettes` (resource) indexed by `PaletteChoice`; both are `init_resource`'d in `main.rs`.

The left rail uses a hand-painted `tool_button` (manual `allocate_exact_size` + `painter`) rather than `egui::Button`, to keep icons crisp at small sizes and to render the active/hover/inactive states with custom strokes.

### Gizmo overlay

`gizmo.rs` runs a second `Camera3d` on `RenderLayers::layer(1)` with `clear_color: None`, drawing an orientation cube into a viewport rect computed by `update_gizmo_viewport` (scheduled `after(ui_system)` so it sees the final egui-occupied area). `GizmoRect` and `GizmoDrag` resources are read by `tool_input_system` to suppress tool clicks over the gizmo and to ignore tool input while the user is dragging the orientation cube.

### Bevy plugin / resource registration

`EguiPlugin` is added with `auto_create_primary_context: false` (set via `EguiGlobalSettings`); the gizmo's secondary camera is what makes this necessary. If you add a new resource consumed by a system, register it with `init_resource` in `main.rs` — most resources here are `#[derive(Default)]` and use that pattern.

## File format

`.roxel` projects are `ron`-serialized `ProjectFile { version: u32, size: [u32; 3], voxels: Vec<([i32; 3], Color8)> }`. Only occupied cells are stored. `version` is currently `1` and unchecked on load — bump and gate if the schema changes.
