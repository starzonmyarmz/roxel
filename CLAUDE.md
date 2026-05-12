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
3. On `Some(DialogResult::*)`, dispatch to `io::project::{save,load}` / `io::vox::export` / `io::obj::export` / `io::fbx::export` / `io::svg::export` / `io::ase::{import,export}` (PNG goes through `snapshot.rs` which spawns a transparent-clear render pass).

`io::fbx::export` writes binary FBX 7.4 (Geometry + Model + Connections + Definitions + GlobalSettings + footer with the canonical magic). Per-face quads with vertex colors via `LayerElementColor` (`ByPolygonVertex`/`Direct`). Y-up to match Bevy and Blender's default importer expectations. The ASCII variant accepted by Maya / 3ds Max / Unity but not Blender was the first attempt — do not revive it without a reason.

Buttons are disabled while `pending.is_active()` so only one dialog runs at a time. **Never** reintroduce sync `rfd::FileDialog::*` calls inside egui draw code.

### UI structure

`apply_egui_style` runs every frame at the top of `ui_system` using the current `Theme` resource. `ui_system` runs in the `EguiPrimaryContextPass` schedule (not `Update`) and lays out four panels: top bar (file/edit + Preferences button on the right), bottom status bar, left tool rail, right inspector (color swatch + popup picker, palette selector with built-ins + `.ase` import/export, recent colors, scene stats). The active palette lives in `Palettes` (resource) indexed by `PaletteChoice`; both are `init_resource`'d in `main.rs`.

Sections in the inspector are flat: bold title, then content, then a thin full-width divider — no card frames. The divider spans the full panel width by painting at `ui.clip_rect().x_range()` rather than `ui.available_width()`. Side-panel left/right edges are drawn as a single 0.5-px vline via `ctx.layer_painter(LayerId::new(Order::Middle, …))` so popups (Foreground) draw over them; the panel `egui::Frame` itself has no stroke.

`tool_button`, the big color swatch, palette swatches, and recent swatches use `egui::Button` wrapped in a `ui.scope` that zeroes `spacing.button_padding` and `spacing.interact_size`. This keeps them at their exact requested size while letting egui's AA tessellator render the rounded fills cleanly (the manual-painter version produced jaggies on Retina displays).

### Theme + Preferences

`Theme` (`theme.rs`) is a `Resource` carrying every egui color slot (bg / panel / surface / surface_hover / accent / accent_dim / text / text_dim / border / faint) plus a `mode: ThemeMode::{Light, Dark}` discriminator. `Theme::dark()` and `Theme::light()` are the two presets.

`Preferences { theme: ThemePref }` (`ThemePref::{Light, Dark, System}`) is loaded on startup via `load_preferences()` and saved via `save_preferences()` whenever the user changes a value in the Preferences modal. The file lives at `dirs::config_dir()/roxel/preferences.ron`.

`refresh_theme_system` runs every frame with a `NonSendMarker` (forces main-thread scheduling so `WINIT_WINDOWS` is accessible). It re-resolves `ThemePref::System` via `winit::Window::theme()` so the UI tracks live OS appearance changes without restart.

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

### Cursor hints

`ui_system` updates `egui::CursorIcon` each frame based on the active modifier (checked only when the pointer is not over an egui area):

| Condition | Cursor |
|-----------|--------|
| RMB held | `Move` (orbit) |
| `Z` held (no Alt) | `ZoomIn` |
| `Alt` + `Z` held | `ZoomOut` |
| `Space` held | `Grab` (or `Grabbing` if LMB also held) |
| `Alt` held alone | `PointingHand` (sticky eyedropper) |
| otherwise | `Crosshair` |

The `Z`-modifier zoom is wired through `zoom_click_system` (`camera.rs`): on `KeyZ` + LMB-just-pressed, it halves (zoom in) or doubles (zoom out, when Alt is also held) `PanOrbitCamera.target_radius`, clamped to `zoom_lower_limit`. `tool_input_system` early-returns while `Z` is held so the click doesn't also paint.

### Gizmo overlay

`gizmo.rs` runs a second `Camera3d` on `RenderLayers::layer(1)` with `clear_color: None`, drawing an orientation cube into a viewport rect computed by `update_gizmo_viewport` (scheduled `after(ui_system)` so it sees the final egui-occupied area). `GizmoRect` and `GizmoDrag` resources are read by `tool_input_system` to suppress tool clicks over the gizmo and to ignore tool input while the user is dragging the orientation cube.

### Bevy plugin / resource registration

`EguiPlugin` is added with `auto_create_primary_context: false` (set via `EguiGlobalSettings`); the gizmo's secondary camera is what makes this necessary. If you add a new resource consumed by a system, register it with `init_resource` in `main.rs` — most resources here are `#[derive(Default)]` and use that pattern.

## File format

`.roxel` projects are `ron`-serialized `ProjectFile { version: u32, size: [u32; 3], voxels: Vec<([i32; 3], Color8)> }`. Only occupied cells are stored. `version` is currently `1` and unchecked on load — bump and gate if the schema changes.
