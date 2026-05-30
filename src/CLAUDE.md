# src/

Core data flow, tools, camera, rendering, app systems. egui surface lives in `src/ui/`, file I/O in `src/io/`.

## Data flow

`VoxelGrid` (`grid.rs`) is the single source of truth. Sparse storage: `HashMap<IVec3, Chunk>` keyed by chunk coord. `CHUNK = 32`, `CHUNK_VOL = 32_768`. Chunks allocate on first write, **drop the moment occupancy hits zero**. Only hard rule: `p.y >= 0` (writes below floor silently refused). No upper bound on X/Y/Z — open world sized by user memory. `chunk_coord` uses `div_euclid`, `local_idx` uses `rem_euclid` (negative coords work).

Every `set` flips `dirty`, inserts owning chunk into `dirty_chunks`, plus seam-neighbour chunks for boundary cells (face occlusion changes there). `dirty_chunks` drained by `regenerate_mesh_system`.

**Mutations flow through `History::record`, not `grid.set` directly.** Recording wraps each set in a `CellDelta`, dedupes per-stroke via `HashSet<(i32,i32,i32)>`. `History::begin`/`end` bracket a stroke; LMB-release path in `tool_input_system` calls `end`. Undo cap `MAX_UNDO = 200`; new stroke clears redo. Picker reads pre-stroke values via `history.pre_stroke_value(p)` (per-cell shadow) — voxels placed earlier in the same stroke are invisible to the next pick. This kills runaway stacking and replaces the old full-grid-clone snapshot (unbounded in open world).

## Mesher

`regenerate_mesh_system` drains `dirty_chunks` each frame. Chunks with data → spawn/update entity; emptied → despawn. Entities live in `VoxelChunkMeshes`. Mesher is greedy: `greedy_quads_bounded` merges same-color same-direction faces in a `[min, max)` chunk-aligned box but queries the **full grid** for cross-bounds occlusion (quads agree at seams). Base colors run through `srgb_to_linear` before upload — Bevy expects linear vertex colors.

`PreviewHide` (`mesh.rs`): erase preview hides target cell; paint preview swaps it in place via `PreviewHide.recolor`. Change to either re-triggers chunk rebuild even when `grid.dirty` is false. Driven by `brush_preview_system` (`preview.rs`) and `shape_preview_system` (`shape_preview.rs`), both scheduled `before(regenerate_mesh_system)`.

All tool preview outlines go through `PreviewGizmos` group (`preview.rs`): `depth_bias = -1.0`, `line.width = 2.5`, `perspective = false`. **Depth bias is mandatory** — without it, outlines flush against a neighbour's face get z-occluded and silently vanish. Color is `accent_outline_color(&theme)` (system blue). Don't reintroduce a luminance-contrast outline — reads as generic edge, not tool affordance.

Previews hide during orbit (RMB), gizmo drag, or mid-stroke — checked via mouse, `GizmoDrag.active`, `PointerState.stroking`. Don't reintroduce a preview that flashes during orbit.

## Picking

`picking.rs` — DDA voxel raycaster fed by `cursor_ray`. Caps at `MAX_DDA_STEPS = 1024` (open-air ray terminates in empty world). Falls back to y=0 floor plane (unbounded X/Z) on miss so brushing on empty space works.

## Tools and input

`tool_input_system` (`tools.rs`) is the central dispatcher. Early-returns when egui wants the pointer or the gizmo viewport rect contains the cursor.

- **`Tool::Paint`** (`P`, bucket icon) — the single recolor tool. Three gestures, all sampling the color pool per cell into one history stroke each:
  - **Drag / single click** — freehand recolor of each occupied voxel touched (per-voxel stroke machinery, the default fall-through path).
  - **Double-click a voxel** — flood-fills its 6-connected region via `select::fill_region` (mirrors the Select tool's double-click-to-pick-region gesture; uses `PointerState.last_press_cell`/`last_press_secs`/`last_press_color` + `DOUBLE_CLICK_SECS`). The first click already recolored the seed, so the flood matches against `last_press_color` — the seed's color *before* that first click — not the seed's current color (which would spread nowhere). `fill_region(start, match_color, pool)` floods cells equal to `match_color` with `start` always included (caller guarantees `start` occupied); it shares `recolor_cells` with `recolor_selection` and walks `connected_region(start, color)`, the explicit-color flood that `connected_same_color` now delegates to.
  - **Active selection** — a click (anywhere) fills the whole selection via `select::recolor_selection`; `F` does the same from the keyboard (`selection_key_action_system`), and the **Edit → Fill Selection** menu item carries the `F` accelerator for discoverability.

  The selection-fill and double-click paths are handled ahead of the anchor/stroke machinery in `tool_input_system` and early-return so they never enter drag mode. The old dedicated `Tool::Fill` (bucket, `G`) and the command-palette "Fill selection" entry were folded in here — recoloring now lives on one tool. The **Edit menu "Fill Selection" item is kept** (with `F`): on macOS AppKit routes the native key-equivalent → `MenuAction::FillSelection`; on Win/Linux there is no native menu, so `selection_key_action_system` owns `F`. Both gate on egui keyboard focus. (Erase still clears selection contents on click-inside; that's a separate path.)
- **`Tool::Eyedropper`** — single-click, auto-restores `tool.previous` on release unless Alt held (sticky via `alt_eyedropper_system`).
- **`Tool::Shape`** (rect/ellipse/line, `ShapeOptions`) — two-click: anchor on picked face → drag 2D footprint → click commits → drag along face normal to extrude (`shapes::extrude`). State in `ShapeState`. Shift during footprint drag locks aspect ratio via `constrain_shape_corner2` (rect/ellipse → square; line → nearest 45° in face plane).
- **`Tool::Select`** (`select.rs`) — same two-phase face-plane drag as Shape, commits a region into `Selection`. AABB hull (drag) or per-cell mask (double-click → `connected_same_color` flood). `selection_render_system` draws marching ants around AABB or along `silhouette_edges` of mask. All region ops consult mask first, fall back to AABB. Backspace/Delete → `clear_selection`; Esc → clear selection.
- **`Tool::Move`** (`select.rs` + `move_drag_system` in `tools.rs`) — translates selection by integer offsets. Mouse drag uses `StrokeAnchor` face plane; Shift locks Y. Overlap with non-source occupied cell refused. `history.abort` on RMB/Esc/tool-switch. Arrow keys: ←/→ = ∓X, ↑/↓ = ∓Z, Shift+↑/↓ = ±Y. One history stroke per commit — `History::record` dedupes mid-drag by overwriting existing delta's `after`.

**`PointerState`** carries `anchor` (`StrokeAnchor` locks build plane axis to one of X/Y/Z during drag), `last_placed` (endpoint for `line3d` 3D Bresenham — fills jump gaps; Shift+click runs one-shot `line3d` from `last_placed` to target), and `last_press_cell`/`last_press_secs` (Paint double-click-to-flood detection).

**`RecentColors`** (`tools.rs`) — `Vec<Color8>` capped at 8 with LRU eviction (duplicate removes and repushes to front).

**`MoveDragState`** — active drag, anchor plane, applied delta, original cells/AABB, `HashSet` of pre-drag occupied cells for O(1) collision checks, prev-frame write state for mid-drag undo restoration. A flag distinguishes selection-less click-drag from selection-anchored drag (cleared on commit).

## Shapes

`shapes.rs` — per-axis 2D cell rasterizers. Axis-agnostic: `other_axes(axis)` returns the u/v pair, `cell_from(u, v, axis_value, axis)` rebuilds the 3D coord.

- `rect_cells(c1, c2, axis, filled)` — filled rect or 4-wall outline.
- `ellipse_cells(c1, c2, axis, filled)` — midpoint edge test; filled checks inside the ellipse equation per cell.
- `line2d_cells(c1, c2, axis)` — axis-aligned 2D Bresenham.
- `extrude(cells, axis, thickness, sign)` — replicates 2D footprint along normal axis.

## Shape picker (long-press)

`ui.rs` — `LONG_PRESS_SECS = 0.18` on the rail button opens the picker; release-over-option commits. Quick click selects via `tool_button`'s `clicked()` path. Press state in `egui::Memory` keyed by `shape_resp.id.with("press_hold")` (`f64::NAN` = not pressed). Release frame still paints the picker so `released && r.contains_pointer()` can fire. Picker is bare `egui::Area::fade_in(false)` + manual `Frame::popup` (not `egui::Popup` — its fade-in is hardcoded). Options are hand-painted (`painter().rect` + `Image::paint_at`), not `egui::Button`, so hover fill tracks `r.contains_pointer()` regardless of press source.

## Clipboard

`clipboard.rs` — selection → `Stamp { cells, origin, aabb }`. Cmd+C/X/V (egui-keys-gated). Cut = copy + clear in one stroke. Paste anchor priority: cursor pick (`hit.cell + hit.normal`) → selection AABB min → stamp origin. Command-palette `Paste` skips cursor branch. Pastes below `y = 0` refused.

## Camera

`spawn_camera` (`camera.rs`) — orbit at `(1, 1, 1).normalize() * EMPTY_WORLD_RADIUS` (isometric). `EMPTY_WORLD_RADIUS = 32.0` is spawn + empty-world fallback. **Forces `Tonemapping::None`** so unlit voxel colors render as the exact sRGB the user picked — Bevy's default `TonyMcMapface` curve desaturates/darkens output and would diverge from the egui palette swatch.

`frame_view_system` (Cmd/Ctrl+0) — AABB via `grid.bounding_box()`, **panel-aware** (uses `ViewportRect` updated `after(ui_system)`). Empty scene → focus=origin, radius=`EMPTY_WORLD_RADIUS`. Zoom readout uses `cam.radius` (smoothed, not `target_radius`) rounded to voxel count — target lied during long lerps.

`zoom_click_system` (`KeyZ` + LMB just-pressed) — multiplies `target_radius` by `ZOOM_STEP_IN = 1/√2` (or `ZOOM_STEP_OUT = √2` with Alt), clamped. Click recenters `target_focus` to picked voxel or floor. `tool_input_system` early-returns while Z held.

`FlybyState { active, t }` — auto-orbit "drone" view. `flyby_system` overwrites `target_yaw`/`pitch`/`radius` every frame (PanOrbitCamera has no public `enabled`, so clobbering targets is how cinematic wins over RMB-drag). Painting gated separately. Esc or palette-toggle ends; mouse does NOT cancel (screen recordings).

`zoom_radius_limits` — lower fixed `ZOOM_LOWER_LIMIT = 8.0` (big scenes still allow close inspection); upper `max(fit_radius * 2, 64)`.

## Gizmo overlay

`gizmo.rs` — second `Camera3d` on `RenderLayers::layer(1)`, `clear_color: None`, draws orientation cube into viewport rect from `update_gizmo_viewport` (`after(ui_system)`). `GizmoRect`/`GizmoDrag`/`GizmoHover` read by `tool_input_system` (suppress clicks), preview systems (hide while dragging), `ui_system` (cursor swap).

## Canvas chrome

**No floor plane.** Chrome stack = dot grid + vignette, both gated on `show_floor_grid`. `show_floor_grid` / `show_origin_axes` are toggled from the **View** menu (native `CheckMenuItem`s on macOS — checked state synced from prefs in `update_menu_enabled_system`; floating pill on Win/Linux), not a Preferences row.

`floor_dots_system` (`main.rs`) — y=0 intersection dots via `FloorDotsGizmos` group (`line.width = 3.0`, `perspective = false`). Each dot is a tiny `+` cross of `tick = 0.05` segments — thick screen-space-constant width makes the short cross render as a round dot. Spacing always 1 voxel (no LOD bands). Window half-extent `(radius * 1.5).clamp(8, 96)` around camera focus; per-dot alpha fades quadratically (`floor_dot_alpha`). No upper radius cap — extreme zoom kills alpha.

`vignette_system` (`EguiPrimaryContextPass` `after(ui_system)`) — 5-vertex `egui::Mesh` (4 dark corners + transparent center) on `Order::Background`. Pseudo-radial darken. Corner alpha light (28/255 dark, 14/255 light).

`draw_origin_system` — RGB axis triad at (0,0,0) via `OriginAxesGizmos` group (`depth_bias = -1.0` reads cleanly over floor dots). Gated on `show_origin_axes`. `triad_fade(near_count)` — full alpha empty, zero at ~8 voxels in 4-cell cube around origin. Wayfinder for empty start, not always-on chrome.

## Lighting

`lighting.rs::spawn_lights` — single `DirectionalLight` anchored at origin (open-world grid has no centroid). `illuminance = 1_500.0`, `shadows_enabled = false`, 2-cascade config, `maximum_distance = 400.0`. Spawned once in `setup_scene`. Voxel materials are `unlit`, so color render is independent of light direction — the directional light affects only any non-unlit gizmo / chrome geometry.

## Color space

`color_space.rs` — sRGB u8 storage everywhere. `ColorSpace` enum (`Hex`/`Rgb`/`Hsl`/`Hsb`/`Oklch`) selects how colors read out; the active space is a persisted preference (`Preferences.color_space`) chosen from the **View → Color Format** menu (native `CheckMenuItem`s on macOS — the active option is checked, synced from prefs in `update_menu_enabled_system`; floating menu pill on Win/Linux), not an inspector control. `ColorSpace::format(rgb)` is the single source of truth for non-editable color strings (under-swatch readout, swatch/recent/palette hover tips); the picker edit fields use `ColorEditBuffer::populate` per-channel slots instead. `widgets::hex_string` is hex-only and now serves just the Preferences custom-canvas-color row. Conversions: `parse_hex`, `rgb_to_hsl`/`hsl_to_rgb`, `rgb_to_hsb`/`hsb_to_rgb`, `rgb_to_oklch`/`oklch_to_rgb`. OKLCH path reuses `mesh::srgb_to_linear`/`linear_to_srgb` so the voxel-color pipeline and the inspector roundtrip match. sRGB roundtrip is within ±1/255.

`ColorEditBuffer` (same file) — string slots per space. Repopulated when `CurrentColor` or active space changes so keystrokes don't roundtrip through `Color8` mid-edit (drops hue on greys / quantises OKLCH chroma). Commit on `lost_focus`; invalid silently reverts.

## Snapshot (PNG export)

`snapshot.rs` — one-shot offscreen `Camera3d` at main camera's transform/projection, renders into `Image` at physical window res with `ClearColorConfig::Custom(Color::NONE)`, captures via `Screenshot` pipeline. **Forces `Tonemapping::None`** — tonemap fullscreen pass writes `alpha=1` to every pixel, clobbering transparent clear. Voxel materials are `unlit` so colors are unaffected.

`SnapshotInProgress` set while in flight. `floor_dots_system`, `vignette_system`, `draw_origin_system`, `selection_render_system` early-return on snapshot frame. `start_snapshot_system` ordered `.before(...)` each. `ScreenshotCaptured` observer clears flag.

## `.rox` project format

`ron`-serialized `ProjectFile { voxels: Vec<([i32; 3], Color8)> }`. Only occupied cells. No `version`, no `size` — open world has neither. Signed coords roundtrip exactly.

Old (`version`, `size`, `voxels`) layout was dropped. ron silently ignores unknown fields, so older v1 files happen to load if `voxels` matches. **Not a compat code path** — side effect of lax deserialization. Don't add explicit v1 handling.

## New-project flow

`NewProject { dialog_open, apply }` (`grid.rs`) — confirm-only modal, no grid size. On confirm, `apply = true`; `apply_new_project_system` consumes next frame: `grid.clear()` (drains chunks → mesher despawns), clear history, drain `VoxelChunkMeshes.chunks`, reset camera to origin / `EMPTY_WORLD_RADIUS`. **Don't poke `VoxelGrid` directly from UI** — go through `NewProject.apply`.

## Onboarding

`onboarding.rs` — first-launch coachmark tour. `Onboarding { active, step, anchors_ready, pending_persist, autostart_fired }`. `TOUR_STEPS` is a `&[TourStep]` const with 4 entries: `Viewport`, `ToolRail`, `ColorPalette`, `GizmoCube` (`AnchorId`). `OnboardingAnchors` captures widget rects per frame inside `ui_system`; `Viewport` and `GizmoCube` reuse `ViewportRect` / `GizmoRect` to avoid duplicate plumbing.

Cards do NOT highlight anchored widgets or draw arrows — proximity to the anchor is the only spatial cue. Dismissal persists via `preferences.onboarding_seen`. Relaunchable from the macOS Help submenu and the `?` button in the non-mac top bar. Modal pauses during dialogs.

Systems: `onboarding_autostart_system` (fires once when prefs say unseen) and `onboarding_overlay_system` (draws cards in `EguiPrimaryContextPass`).

## Updater

`updater.rs` — background GitHub Releases check via `ureq`. `REPO = "starzonmyarmz/roxel"`, `RATE_LIMIT_SECS = 24h`, `HTTP_TIMEOUT_SECS = 10`. `UpdateCheck(UpdateState)` resource is the state machine (idle/checking/ready/failed). `start_check(state, manual)` spawns the HTTP task on `AsyncComputeTaskPool`; `poll_update_check_system` drains via `block_on(future::poll_once)`. `startup_check_system` honours `preferences.last_update_check` rate limit; manual check from command palette ignores it.

`parse_tag` accepts `v1.2.3` and `1.2.3`. `is_newer` is plain tuple compare. `parse_release_json` uses `serde_json` to deserialize `tag_name` + `html_url` (extra GitHub fields ignored). Toast on newer version; `open_url` shells out to `open`/`xdg-open`/`start`.

## Command-palette dispatch

`ui/command_palette.rs` owns both the resource and the dispatch (draw logic is documented in `src/ui/CLAUDE.md`). `CommandPalette { open, search, selected, just_opened, pending }`. `CommandAction` enum is the dispatch surface; `Category` (`File`/`Edit`/`Tools`/`Shape`/`View`/`Palette`/`Color`/`Preferences`/`Help`) groups entries.

`fuzzy_match(haystack, query)` — case-insensitive subsequence with light scoring (start-of-word bonus, contiguous bonus, gap penalty). `matches_entry` searches `label` then `keywords` (small kw penalty so label hits win). `build_catalog(&CatalogState)` is a pure builder taking a world snapshot. `rank(entries, query)` sorts enabled-first then by score desc — disabled entries still appear, just below.

`command_palette_shortcut_system` runs **before** egui to capture Cmd/Ctrl+K before any focused text edit consumes it.

## Cursor hints

`ui_system` updates `egui::CursorIcon` each frame (only when pointer not over egui):

| Condition | Cursor |
|-----------|--------|
| RMB held / gizmo dragged | `Grabbing` |
| Gizmo hovered / Space (LMB up) | `Grab` |
| Space + LMB | `Grabbing` |
| `Z` (no Alt) | `ZoomIn` |
| `Alt`+`Z` | `ZoomOut` |
| `Alt` alone | `PointingHand` (sticky eyedropper) |
| otherwise | `Crosshair` |

## App icon

Lives in `assets/icons/`: `roxel.svg` (1024×1024 master, squircle + transparent corners per macOS app-icon convention), `roxel-1024.png` (embedded — derived from the SVG via `sips`), `roxel.icns` (bundling), `roxel.iconset/`.

`set_window_icon` (`icon.rs`) applies the embedded PNG two ways:
1. `winit::Window::set_window_icon` — Win/Linux only; **no-ops on macOS** for unbundled binaries.
2. `NSApplication::setApplicationIconImage` (objc2 + objc2-app-kit) — only way to get a dock icon for `cargo run --release` on macOS.

Accesses `WINIT_WINDOWS` via thread-local `bevy::winit::WINIT_WINDOWS` (not a regular NonSend resource in Bevy 0.18). Main-thread only, enforced by `NonSendMarker`.

Packaged builds: `[package.metadata.bundle]` in `Cargo.toml` points cargo-bundle at `roxel.icns`.
