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
- `pre-push` — `cargo test`

CI re-runs both, so `--no-verify` is caught upstream.

## Architecture

Single-window Bevy 0.18 app, `bevy_egui` UI, `bevy_panorbit_camera` viewport. One binary (`src/main.rs`), no lib crate, no workspace.

### Data flow

`VoxelGrid` (`grid.rs`) is the single source of truth. Sparse storage: `HashMap<IVec3, Chunk>` keyed by chunk coord. `CHUNK = 32`, `CHUNK_VOL = 32_768`. Chunks allocate on first write, **drop the moment occupancy hits zero**. Only hard rule: `p.y >= 0` (writes below floor silently refused). No upper bound on X/Y/Z — open world sized by user memory. `chunk_coord` uses `div_euclid`, `local_idx` uses `rem_euclid` (negative coords work).

Every `set` flips `dirty`, inserts owning chunk into `dirty_chunks`, plus seam-neighbour chunks for boundary cells (face occlusion changes there). `dirty_chunks` drained by `regenerate_mesh_system`.

**Mutations flow through `History::record`, not `grid.set` directly.** Recording wraps each set in a `CellDelta`, dedupes per-stroke via `HashSet<(i32,i32,i32)>`. `History::begin`/`end` bracket a stroke; LMB-release path in `tool_input_system` calls `end`. Undo cap `MAX_UNDO = 200`; new stroke clears redo. Picker reads pre-stroke values via `history.pre_stroke_value(p)` (per-cell shadow) — voxels placed earlier in the same stroke are invisible to the next pick. This kills runaway stacking and replaces the old full-grid-clone snapshot (which would be unbounded in open world).

`regenerate_mesh_system` drains `dirty_chunks` each frame. Chunks with data → spawn/update entity; emptied → despawn. Entities live in `VoxelChunkMeshes`. Mesher is greedy: `greedy_quads_bounded` merges same-color same-direction faces in a `[min, max)` chunk-aligned box but queries the **full grid** for cross-bounds occlusion (quads agree at seams). Base colors run through `srgb_to_linear` before upload — Bevy expects linear vertex colors.

`PreviewHide` (`mesh.rs`): erase preview hides target cell; paint preview swaps it in place via `PreviewHide.recolor`. Change to either re-triggers chunk rebuild even when `grid.dirty` is false. Driven by `brush_preview_system` (`preview.rs`) and `shape_preview_system` (`shape_preview.rs`), both scheduled `before(regenerate_mesh_system)`.

All tool preview outlines go through `PreviewGizmos` group (`preview.rs`): `depth_bias = -1.0`, `line.width = 2.5`, `perspective = false`. **Depth bias is mandatory** — without it, outlines flush against a neighbour's face get z-occluded and silently vanish. Color is `accent_outline_color(&theme)` (system blue). Don't reintroduce a luminance-contrast outline — reads as generic edge, not tool affordance.

Previews hide during orbit (RMB), gizmo drag, or mid-stroke — checked via mouse, `GizmoDrag.active`, `PointerState.stroking`. Don't reintroduce a preview that flashes during orbit.

### Tools and input

`tool_input_system` is the central dispatcher. Early-returns when egui wants the pointer or the gizmo viewport rect contains the cursor.

- **`Tool::Eyedropper`** — single-click, auto-restores `tool.previous` on release unless Alt held (sticky via `alt_eyedropper_system`).
- **`Tool::Shape`** (rect/ellipse/line, `ShapeOptions`) — two-click: anchor on picked face → drag 2D footprint → click commits → drag along face normal to extrude (`shapes::extrude`). State in `ShapeState`. Shift during footprint drag locks aspect ratio via `constrain_shape_corner2` (rect/ellipse → square; line → nearest 45° in face plane).
- **Long-press shape picker** (`ui.rs`) — `LONG_PRESS_SECS = 0.18` on the rail button opens the picker; release-over-option commits. Quick click selects via `tool_button`'s `clicked()` path. Press state in `egui::Memory` keyed by `shape_resp.id.with("press_hold")` (`f64::NAN` = not pressed). Release frame still paints the picker so `released && r.contains_pointer()` can fire. Picker is bare `egui::Area::fade_in(false)` + manual `Frame::popup` (not `egui::Popup` — its fade-in is hardcoded). Options are hand-painted (`painter().rect` + `Image::paint_at`), not `egui::Button`, so hover fill tracks `r.contains_pointer()` regardless of press source.
- **`Tool::Select`** (`select.rs`) — same two-phase face-plane drag as Shape, commits a region into `Selection`. AABB hull (drag) or per-cell mask (double-click → `connected_same_color` flood). `selection_render_system` draws marching ants around AABB or along `silhouette_edges` of mask. All region ops consult mask first, fall back to AABB. Backspace/Delete → `clear_selection`; Esc → clear selection.
- **`Tool::Move`** (`select.rs` + `move_drag_system` in `tools.rs`) — translates selection by integer offsets. Mouse drag uses `StrokeAnchor` face plane; Shift locks Y. Overlap with non-source occupied cell refused. `history.abort` on RMB/Esc/tool-switch. Arrow keys: ←/→ = ∓X, ↑/↓ = ∓Z, Shift+↑/↓ = ±Y. One history stroke per commit — `History::record` dedupes mid-drag by overwriting the existing delta's `after`.

**Clipboard** (`clipboard.rs`) — selection → `Stamp { cells, origin, aabb }`. Cmd+C/X/V (egui-keys-gated). Cut = copy + clear in one stroke. Paste anchor priority: cursor pick (`hit.cell + hit.normal`) → selection AABB min → stamp origin. Command-palette `Paste` skips cursor branch. Pastes below `y = 0` refused.

`PointerState` carries `anchor` (`StrokeAnchor` — locks build plane axis during drag) and `last_placed` (endpoint for `line3d` 3D Bresenham — fills jump gaps; Shift+click runs one-shot `line3d` from `last_placed` to target).

`picking.rs` — DDA voxel raycaster fed by `cursor_ray`. Caps at `MAX_DDA_STEPS = 1024` (open-air ray terminates in empty world). Falls back to y=0 floor plane (unbounded X/Z) on miss so brushing on empty space works.

### File I/O — async dialogs are mandatory

Sync `rfd::FileDialog` **blocks winit's event loop on macOS** (beachball). All save/open/export goes through `PendingDialog`:

1. Click → `pending.spawn(async { rfd::AsyncFileDialog... })` on `AsyncComputeTaskPool`
2. `poll_dialogs_system` (Update) calls `block_on(future::poll_once(task))` each frame
3. On `Some(DialogResult::*)` → dispatch to `io::project/vox/qb/gox/obj/fbx/gltf/svg/ase` (PNG via `snapshot.rs`)

Buttons disabled while `pending.is_active()`. **Never reintroduce sync `rfd::FileDialog::*` in egui draw code.**

**Format notes:**
- `io::fbx::export` — binary FBX 7.4, Y-up, per-face quads with `LayerElementColor` (`ByPolygonVertex`/`Direct`). Don't revive the ASCII variant (Blender rejects).
- `io::gltf::export` — `.glb` (12-byte header + JSON + BIN chunks, 4-byte aligned). Indexed triangles, `COLOR_0` as u8-norm RGBA, Y-up.
- **Axis remap**: `.vox` and `.gox` are Z-up → swap `(x, y_roxel, z_roxel) ↔ (x_vox, z_vox, y_vox)` on import + export. `.qb` is Y-up natively. `.gox` BL16 written as raw 16³ RGBA; PNG-encoded BL16 rejected with clear error.
- **AABB-shift on export** for unsigned-coord formats (`.vox`, `.qb`): translate emitted voxels by `-grid.bounding_box().min` so model min lands at (0,0,0). `.vox` refuses export when any axis extent > 256 (u8 cap) — user gets a toast. Mesh formats (`.obj`/`.fbx`/`.gltf`/`.svg`) and `.gox` handle negative coords natively.
- Imports `grid.set` each voxel at source coord. No resize step, no `snap_to_allowed_size`. `apply_import_system` just consumes the `PendingImport` flag. Cmd+0 to re-frame.

**Shared io helpers — use them, don't reroll:**
- `grid::iter_occupied()` — `(IVec3, Color8)` over all chunks. Use in every exporter.
- `grid::bounding_box()` — `Option<(min, max)>` inclusive.
- `mesh::for_each_exposed_face(grid, |cell, face, rgba| ...)` — per-face quad iteration with occlusion culling. Shared by fbx/gltf.
- `io::reader::LeReader` — bounds-checked LE binary reader. Shared by qb/gox import.
- `io::test_util::tmp_path(name, ext)` — `#[cfg(test)]` temp-file helper.

`io::palettes` persists user palettes (built-ins filtered) to `{config_dir}/roxel/palettes.ron`. Mutations must call `io::palettes::save(...)` — no autosave.

`io::recent` persists recent `.rox` paths (cap `MAX_RECENT = 10`) to `{config_dir}/roxel/recent.ron`. `RecentFiles` resource (`ui/dialogs.rs`) is hydrated on startup; `poll_dialogs_system` pushes after successful open/save. Drives in-app Open Recent submenu (`ui.rs`) and macOS native File menu (`menu.rs`).

### macOS menu bar

`menu.rs` (`#[cfg(target_os = "macos")]`) installs native `muda` menu. Four chained Update systems: `install_menu_system` (one-shot), `poll_menu_events_system` (drains `MenuEvent` → `MenuQueue`), `apply_menu_actions_system` (translates to `PendingDialog`/`History`/`NewProject`), `update_recent_menu_system` (rebuilds when `RecentFiles` changes). `update_menu_enabled_system` greys undo/redo when stacks empty. Open Recent reuses `MAX_RECENT` pre-allocated `MenuItem`s (muda doesn't support clean runtime creation).

Menu mirrors `ui.rs` File/Edit — wire new dialog actions into both unless mac-only.

### New-project flow

`NewProject { dialog_open, apply }` (`grid.rs`) — confirm-only modal, no grid size. On confirm, `apply = true`; `apply_new_project_system` consumes next frame: `grid.clear()` (drains chunks → mesher despawns), clear history, drain `VoxelChunkMeshes.chunks`, reset camera to origin / `EMPTY_WORLD_RADIUS`. **Don't poke `VoxelGrid` directly from UI** — go through `NewProject.apply`.

### UI structure

`apply_egui_style` runs every frame at top of `ui_system` using current `Theme`. `ui_system` runs in `EguiPrimaryContextPass` (not Update). One anchored panel + two floating surfaces:

- **Left inspector** (`SidePanel::left`) — color swatch + picker, palette selector with add/dup/rename/delete/reorder, `.ase` import/export, recent colors, shape options, scene stats. "Status" top section (Size/Voxels/Zoom) gated on `prefs.show_status_chip`.
- **Floating tool island** (`ui/floating.rs::tool_island`) — right-center pivot. Icon-only; `prefs.show_tool_labels` adds dim caption. Shape picker opens to the left (`Align2::RIGHT_TOP`).
- **Floating menu pill** (`ui/floating.rs::pill_menu`) — top-center, Win/Linux only, gated on `prefs.show_floating_menu_bar`. macOS uses native `muda` menu.

Both floating surfaces share `pill_frame` + `floating_area`. `space::FLOAT_GAP` is canvas-edge inset.

**Focus mode**: Backquote (`` ` ``) flips `UiVisible.0` (`ui/visibility.rs`), hiding inspector + floating surfaces. Toasts/modals still render. Backquote (not Tab) avoids egui focus-traversal collision. Gated on `ctx.wants_keyboard_input()` so it types literally into focused fields.

**macOS titlebar**: primary window has `titlebar_transparent` + `titlebar_show_title = false` + `fullsize_content_view`. Inspector reserves `height::MAC_TITLEBAR_GUTTER = 28` px top inner padding on macOS.

Inspector sections are flat (bold title → content → full-width divider, no card frames). Divider paints at `ui.clip_rect().x_range()` (not `available_width()`) with `painter.round_to_pixel_center(...)` on y for Retina crispness.

`tool_button`, big swatch, palette/recent swatches: `egui::Button` wrapped in a scope that zeroes `button_padding` + `interact_size`. Keeps exact sizing while egui's AA tessellator renders cleanly (manual-painter version produced Retina jaggies).

### Design tokens

**Every spacing, padding, radius, font size, icon size, swatch size, stroke width, fixed widget size, container width/height must resolve to `src/ui/tokens.rs` — not an inline literal.** Submodules: `font` (SMALL/BODY/HEADING, ≥12pt), `radius` (XS/SM/MD/LG/PILL u8, `PILL = 18`), `space` (scalar f32), `gap` (Vec2 item_spacing), `pad` (Vec2 button_padding), `icon`, `swatch`, `stroke` (HAIR/NORMAL/ACCENT), `size`, `width`, `height`.

All values land on a 4-px grid and are even. Token guard tests in `tokens::tests` enforce. **Never inline a literal radius/padding/gap/font size.** Add a token if none fits. Colors stay in `Theme` (`theme.rs`) — they swap with mode.

### UI widget helpers

Helpers in `src/ui/widgets.rs`:
- Structural: `section`, `prefs_row`, `modal_window`, `swatch_grid`, `vertical_rule`. Floating-surface: `pill_frame`, `floating_area`, `tool_island`, `pill_menu` (in `ui/floating.rs`).
- Buttons: `tool_button`, `icon_button`, `icon_only_button`, `wide_action_button`, `dialog_button` (primary = accent), `chip_button`, `swatch_button`.
- Labels: `stat_row`, `hint_label`, `status_label`, `hex_label`/`hex_string`, `tool_label`, `plane_color_row`.

**Prefer helpers over hand-rolling.** Promote a second call site into `widgets.rs`. Never reach for `egui::Window::new` or hand-painted hlines directly — use `modal_window` and `section`.

### Toast notifications

`crate::ui::toast::Toasts` — capped `VecDeque<Toast>` (max 4 visible). `toasts.success/error/info(msg)`. Success TTL 3.5s, error 6s. `draw_toasts` anchors bottom-center of canvas, pivot `CENTER_BOTTOM`, grows upward. Always renders (even in focus mode).

**Never reintroduce `eprintln!` for user-facing I/O errors** — terminal output is invisible in packaged apps. Internal diagnostics (dropped-voxel counts) can still go to stderr.

### Theme + Preferences

`Theme` (`theme.rs`) — Resource with all egui color slots + `mode: ThemeMode::{Light, Dark}`. `Theme::dark()` (bg `#191A2E`) / `Theme::light()`.

`Preferences` (`theme.rs`) — `theme`, `canvas_bg` (`MatchTheme` resolves via `canvas_match_color` to near-neutral grey, not bluish panel bg, so voxel hues read true — Light `#F2F3F6`, Dark `#1C1C1E`), `show_floor_grid` (master canvas chrome toggle: dot grid + vignette), `show_origin_axes` (RGB triad, auto-fades as voxels appear near origin), `color_space` (`Hex`/`Rgb`/`Hsl`/`Hsb`/`Oklch` — conversions in `src/color_space.rs`, sRGB roundtrip within ±1/255, OKLCH reuses `mesh::srgb_to_linear`/`linear_to_srgb`), `show_status_chip`, `show_tool_labels`, `show_floating_menu_bar` (default `!cfg!(target_os = "macos")`).

Editable color fields backed by `ColorEditBuffer` — string slots repopulated when `CurrentColor` or active space changes, so keystrokes don't roundtrip through `Color8` mid-edit (which would drop hue on greys / quantise OKLCH chroma). Commit on `lost_focus`; invalid silently reverts.

**Every field after `theme` is `#[serde(default)]`** — any new field must have a default provider or older `preferences.ron` becomes unparseable and reverts to `Default`. Removed fields (`show_floor`, `floor_color`, `show_walls`, `wall_color`, `preview_outline`, `show_y_axis`) silently dropped by ron's lax deserializer. Guard tests: `theme::tests::preferences_loads_after_floor_fields_removed`, `..._show_y_axis_field_removed`.

`Preferences` loaded on startup, saved on modal change. Lives at `{config_dir}/roxel/preferences.ron`. `refresh_theme_system` (NonSendMarker for main-thread `WINIT_WINDOWS`) resolves `ThemePref::System` against `winit::Window::theme()`. `apply_canvas_bg_system` diffs before writing.

### Canvas chrome

**No floor plane.** Chrome stack = dot grid + vignette, both gated on `show_floor_grid`.

`floor_dots_system` (`main.rs`) — y=0 intersection dots via `FloorDotsGizmos` group (`line.width = 3.0`, `perspective = false`). Each dot is a tiny `+` cross of `tick = 0.05` segments — thick screen-space-constant width makes the short cross render as a round dot. Spacing always 1 voxel (no LOD bands). Window half-extent `(radius * 1.5).clamp(8, 96)` around camera focus; per-dot alpha fades quadratically (`floor_dot_alpha`). No upper radius cap — extreme zoom kills alpha.

`vignette_system` (`EguiPrimaryContextPass` `after(ui_system)`) — 5-vertex `egui::Mesh` (4 dark corners + transparent center) on `Order::Background`. Pseudo-radial darken. Corner alpha light (28/255 dark, 14/255 light).

`draw_origin_system` — RGB axis triad at (0,0,0) via `OriginAxesGizmos` group (`depth_bias = -1.0` reads cleanly over floor dots). Gated on `show_origin_axes`. `triad_fade(near_count)` — full alpha empty, zero at ~8 voxels in 4-cell cube around origin. Wayfinder for empty start, not always-on chrome.

### Fonts

`install_fonts` (`theme.rs`) embeds Inter Medium + Inter SemiBold via `include_bytes!` (families `"InterMedium"`, `INTER_SEMIBOLD_FAMILY = "InterSemiBold"`).

Monospace **not embedded** — `load_system_monospace` reads `SFNSMono`/`Monaco` (macOS), `consola`/`cour` (Win), DejaVu/Ubuntu/Liberation (Linux). Falls back to egui built-in.

For real bold use `FontFamily::Name(INTER_SEMIBOLD_FAMILY.into())`, not `.strong()` (`.strong()` adds an extra stroke pass, not a family switch).

**Critical scheduling**: `font_setup` runs in `PreUpdate` between `EguiPreUpdateSet::InitContexts` and `EguiPreUpdateSet::BeginPass`. `Context::set_fonts` only takes effect on the next `begin_pass` — if installed inside `EguiPrimaryContextPass` (after begin_pass), first frame panics with `"FontFamily::Name(\"InterSemiBold\") is not bound to any fonts"`.

### App icon

Lives in `assets/icons/`: `roxel.svg`, `roxel-256.png` (embedded), `roxel.icns` (bundling), `roxel.iconset/`.

`set_window_icon` (`icon.rs`) applies the embedded PNG two ways:
1. `winit::Window::set_window_icon` — Win/Linux only; **no-ops on macOS** for unbundled binaries.
2. `NSApplication::setApplicationIconImage` (objc2 + objc2-app-kit) — only way to get a dock icon for `cargo run --release` on macOS.

Accesses `WINIT_WINDOWS` via thread-local `bevy::winit::WINIT_WINDOWS` (not a regular NonSend resource in Bevy 0.18). Main-thread only, enforced by `NonSendMarker`.

Packaged builds: `[package.metadata.bundle]` in `Cargo.toml` points cargo-bundle at `roxel.icns`.

### Camera

`spawn_camera` (`camera.rs`) — orbit at `(1, 1, 1).normalize() * EMPTY_WORLD_RADIUS` (isometric). `EMPTY_WORLD_RADIUS = 32.0` is spawn + empty-world fallback.

`frame_view_system` (Cmd/Ctrl+0) — AABB via `grid.bounding_box()`, **panel-aware** (uses `ViewportRect` updated `after(ui_system)`). Empty scene → focus=origin, radius=`EMPTY_WORLD_RADIUS`. Zoom readout uses `cam.radius` (smoothed, not `target_radius`) rounded to voxel count — target lied during long lerps.

`zoom_click_system` (`KeyZ` + LMB just-pressed) — multiplies `target_radius` by `ZOOM_STEP_IN = 1/√2` (or `ZOOM_STEP_OUT = √2` with Alt), clamped. Click recenters `target_focus` to picked voxel or floor. `tool_input_system` early-returns while Z held.

`FlybyState { active, t }` — auto-orbit "drone" view. `flyby_system` overwrites `target_yaw`/`pitch`/`radius` every frame (PanOrbitCamera has no public `enabled`, so clobbering targets is how cinematic wins over RMB-drag). Painting gated separately. Esc or palette-toggle ends; mouse does NOT cancel (screen recordings).

`zoom_radius_limits` — lower fixed `ZOOM_LOWER_LIMIT = 8.0` (big scenes still allow close inspection); upper `max(fit_radius * 2, 64)`.

### Cursor hints

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

### Gizmo overlay

`gizmo.rs` — second `Camera3d` on `RenderLayers::layer(1)`, `clear_color: None`, draws orientation cube into viewport rect from `update_gizmo_viewport` (`after(ui_system)`). `GizmoRect`/`GizmoDrag`/`GizmoHover` read by `tool_input_system` (suppress clicks), preview systems (hide while dragging), `ui_system` (cursor swap).

### Bevy registration

`EguiPlugin` added with `auto_create_primary_context: false` (via `EguiGlobalSettings`) — required by the gizmo secondary camera. New resources: register with `init_resource` in `main.rs`. `Palettes` is the exception — uses `insert_resource(Palettes::with_user_loaded())` so user palettes load on startup.

## File format

### Snapshot (PNG export)

`snapshot.rs` — one-shot offscreen `Camera3d` at main camera's transform/projection, renders into `Image` at physical window res with `ClearColorConfig::Custom(Color::NONE)`, captures via `Screenshot` pipeline. **Forces `Tonemapping::None`** — tonemap fullscreen pass writes `alpha=1` to every pixel, clobbering transparent clear. Voxel materials are `unlit` so colors are unaffected.

`SnapshotInProgress` set while in flight. `floor_dots_system`, `vignette_system`, `draw_origin_system`, `selection_render_system` early-return on snapshot frame. `start_snapshot_system` ordered `.before(...)` each. `ScreenshotCaptured` observer clears flag.

### `.rox` project

`ron`-serialized `ProjectFile { voxels: Vec<([i32; 3], Color8)> }`. Only occupied cells. No `version`, no `size` — open world has neither. Signed coords roundtrip exactly.

Old (`version`, `size`, `voxels`) layout was dropped. ron silently ignores unknown fields, so older v1 files happen to load if `voxels` matches. **Not a compat code path** — side effect of lax deserialization. Don't add explicit v1 handling.
