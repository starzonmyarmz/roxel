# src/

Core data flow, tools, camera, rendering, app systems. egui surface lives in `src/ui/`, file I/O in `src/io/`.

## Data flow

`VoxelGrid` (`grid.rs`) is the single source of truth. Sparse storage: `HashMap<IVec3, Chunk>` keyed by chunk coord. `CHUNK = 32`, `CHUNK_VOL = 32_768`. Chunks allocate on first write, **drop the moment occupancy hits zero**. Only hard rule: `p.y >= 0` (writes below floor silently refused). No upper bound on X/Y/Z — open world sized by user memory. `chunk_coord` uses `div_euclid`, `local_idx` uses `rem_euclid` (negative coords work).

Every `set` flips `dirty`, inserts owning chunk into `dirty_chunks`, plus seam-neighbour chunks for boundary cells (face occlusion changes there). `dirty_chunks` drained by `regenerate_mesh_system`.

**Mutations flow through `History::record`, not `grid.set` directly.** Recording wraps each set in a `CellDelta`, dedupes per-stroke via `HashSet<(i32,i32,i32)>`. `History::begin`/`end` bracket a stroke; LMB-release path in `tool_input_system` calls `end`. `begin` commits a stroke that's still open instead of dropping it; scene-editing actions (`Action::edits_scene`) and the Move arrow-key nudge are disabled while `History::is_open()`, so a key command can't land mid-drag. Undo cap `MAX_UNDO = 200`; new stroke clears redo. Picker reads pre-stroke values via `history.pre_stroke_value(p)` (per-cell shadow) — voxels placed earlier in the same stroke are invisible to the next pick. This kills runaway stacking and replaces the old full-grid-clone snapshot (unbounded in open world).

## Mesher

`regenerate_mesh_system` drains `dirty_chunks` each frame. Chunks with data → spawn/update entity; emptied → despawn. Entities live in `VoxelChunkMeshes`. Mesher is greedy: `greedy_quads_bounded` merges same-color same-direction faces in a `[min, max)` chunk-aligned box but queries the **full grid** for cross-bounds occlusion (quads agree at seams). Base colors run through `srgb_to_linear` before upload — Bevy expects linear vertex colors.

`PreviewHide` (`mesh.rs`): erase preview hides target cell; paint preview swaps it in place via `PreviewHide.recolor`. Change to either re-triggers chunk rebuild even when `grid.dirty` is false. Driven by `brush_preview_system` (`preview.rs`) and `shape_preview_system` (`shape_preview.rs`), both scheduled `before(regenerate_mesh_system)`.

All tool preview outlines go through `PreviewGizmos` group (`preview.rs`): `depth_bias = -1.0`, `line.width = 2.5`, `perspective = false`. **Depth bias is mandatory** — without it, outlines flush against a neighbour's face get z-occluded and silently vanish. Color is `accent_outline_color(&theme)` (system blue). Don't reintroduce a luminance-contrast outline — reads as generic edge, not tool affordance.

Previews hide during orbit (RMB), gizmo drag, or mid-stroke — checked via mouse, `GizmoDrag.active`, `PointerState.stroking`. Don't reintroduce a preview that flashes during orbit.

## Picking

`picking.rs` — DDA voxel raycaster fed by `cursor_ray`. Steps until the ray can no longer reach `bounds` (`step_budget`: L1 distance from the start cell to the far corner of `grid.chunk_bounds()`, unioned with `History::pre_stroke_bounds()` on the stroke path), so it scales with camera distance, an empty world takes zero steps, and `MAX_DDA_STEPS = 65536` bounds a pathological hull. Falls back to y=0 floor plane (unbounded X/Z) on miss so brushing on empty space works.

## Tools and input

`tool_input_system` (`tools.rs`) is the central dispatcher. Early-returns when egui wants the pointer or the gizmo viewport rect contains the cursor.

- **`Tool::Paint`** (`P`, bucket icon) — the single recolor tool. Three gestures, all sampling the color pool per cell into one history stroke each:
  - **Drag / single click** — freehand recolor of each occupied voxel touched (per-voxel stroke machinery, the default fall-through path).
  - **Double-click a voxel** — flood-fills its 6-connected region via `select::fill_region` (mirrors the Select tool's double-click-to-pick-region gesture; uses `PointerState.last_press_cell`/`last_press_secs`/`last_press_color` + `DOUBLE_CLICK_SECS`). The first click already recolored the seed, so the flood matches against `last_press_color` — the seed's color *before* that first click — not the seed's current color (which would spread nowhere). `fill_region(start, match_color, pool)` floods cells equal to `match_color` with `start` always included (caller guarantees `start` occupied); it shares `recolor_cells` with `recolor_selection` and walks `connected_region(start, color)`, the explicit-color flood that `connected_same_color` now delegates to.
  - **Active selection** — a click on a selected voxel (`hits_selection`, shared with Erase) fills the whole selection; clicks elsewhere fall through to the freehand path via `select::recolor_selection`; `F` does the same from the keyboard (`Action::FillSelection`), also listed as **Edit → Fill Selection**.

  The selection-fill and double-click paths are handled ahead of the anchor/stroke machinery in `tool_input_system` and early-return so they never enter drag mode. The old dedicated `Tool::Fill` (bucket, `G`) was folded in here — recoloring now lives on one tool. (Erase still clears selection contents on click-inside; that's a separate path.)
- **`Tool::Eyedropper`** — single-click, auto-restores `tool.previous` on release unless Alt held (sticky via `alt_eyedropper_system`).
- **`Tool::Shape`** (rect/ellipse/line/sphere, `ShapeOptions`) — two-click: anchor on picked face → drag 2D footprint → click commits → drag along face normal to extrude (`shapes::extrude`). State in `ShapeState`. Shift during footprint drag locks aspect ratio via `constrain_shape_corner2` (rect/ellipse/sphere → square; line → nearest 45° in face plane). **Sphere** is the exception to the extrude model: its phase-2 depth is the 3rd ellipsoid semi-axis (not a slice count), so it routes to `shapes::ellipsoid_cells` instead of `extrude`. Shift-locked square footprint + matching extrude → true sphere; otherwise a general ellipsoid.
- **`Tool::Select`** (`select.rs`) — same two-phase face-plane drag as Shape, commits a region into `Selection`. AABB hull (drag) or per-cell mask (double-click → `connected_same_color` flood). `selection_render_system` draws marching ants around AABB or along `silhouette_edges` of mask. All region ops consult mask first, fall back to AABB. Backspace/Delete → `Action::DeleteSelectionContents`; Esc → clear selection (`selection_escape_system` under other tools).
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
- `ellipsoid_cells(c1, c2, axis, thickness, normal_sign)` — solid 3D ellipsoid; in-plane bounds give two semi-axes, signed extrude depth gives the third. `compute_shape_cells` short-circuits to this for `Sphere` instead of `extrude`.

## Shape picker (long-press)

`ui.rs` — `LONG_PRESS_SECS = 0.18` on the rail button opens the picker; release-over-option commits. Quick click selects via `tool_button`'s `clicked()` path. Press state in `egui::Memory` keyed by `shape_resp.id.with("press_hold")` (`f64::NAN` = not pressed). Release frame still paints the picker so `released && r.contains_pointer()` can fire. Picker is bare `egui::Area::fade_in(false)` + manual `Frame::popup` (not `egui::Popup` — its fade-in is hardcoded). Options are hand-painted (`painter().rect` + `Image::paint_at`), not `egui::Button`, so hover fill tracks `r.contains_pointer()` regardless of press source.

## Clipboard

`clipboard.rs` — selection → `Stamp { cells, origin, aabb }`. Cmd+C/X/V are `Action::Copy`/`Cut`/`Paste`. Cut = copy + clear in one stroke. Paste anchor priority: cursor pick (`CursorView::paste_target`, `hit.cell + hit.normal`) → selection AABB min → stamp origin. The cursor branch runs only for `Origin::Canvas` (shortcut or macOS menu bar); from the command palette or pill (`Origin::Ui`) the pointer is over chrome, so it's skipped. Pastes below `y = 0` refused.

## Camera

`spawn_camera` (`camera.rs`) — orbit at `(1, 1, 1).normalize() * EMPTY_WORLD_RADIUS` (isometric). `EMPTY_WORLD_RADIUS = 32.0` is spawn + empty-world fallback. **Forces `Tonemapping::None`** so unlit voxel colors render as the exact sRGB the user picked — Bevy's default `TonyMcMapface` curve desaturates/darkens output and would diverge from the egui palette swatch.

`frame_view_system` (Cmd/Ctrl+0 → `Action::FrameView` → `PendingFrameView`) — AABB via `grid.bounding_box()`, **panel-aware** (uses `ViewportRect` updated `after(ui_system)`). Empty scene → focus=origin, radius=`EMPTY_WORLD_RADIUS`. Zoom readout uses `cam.radius` (smoothed, not `target_radius`) rounded to voxel count — target lied during long lerps.

`zoom_click_system` (`KeyZ` + LMB just-pressed) — multiplies `target_radius` by `ZOOM_STEP_IN = 1/√2` (or `ZOOM_STEP_OUT = √2` with Alt), clamped. Click recenters `target_focus` to picked voxel or floor. `tool_input_system` early-returns while Z held.

`FlybyState { active, t }` — auto-orbit "drone" view. `flyby_system` overwrites `target_yaw`/`pitch`/`radius` every frame (PanOrbitCamera has no public `enabled`, so clobbering targets is how cinematic wins over RMB-drag). Painting gated separately. Esc or palette-toggle ends; mouse does NOT cancel (screen recordings).

`zoom_radius_limits` — lower fixed `ZOOM_LOWER_LIMIT = 8.0` (big scenes still allow close inspection); upper `max(fit_radius * 2, 64)`.

## Gizmo overlay

`gizmo.rs` — second `Camera3d` on `RenderLayers::layer(1)`, `clear_color: None`, draws orientation cube into viewport rect from `update_gizmo_viewport` (`after(ui_system)`). `GizmoRect`/`GizmoDrag`/`GizmoHover` read by `tool_input_system` (suppress clicks), preview systems (hide while dragging), `ui_system` (cursor swap). Drag orbits; a press-release within `CLICK_SLOP` on a hovered face snaps yaw/pitch to that face (`face_angles`, the view-preset angles plus a bottom view), taking the short way round (`nearest_turn`) and keeping focus/zoom.

## Canvas chrome

**No floor plane.** Chrome stack = dot grid + vignette, both gated on `show_floor_grid`. `show_floor_grid` / `show_origin_axes` are toggled from the **View** menu (native `CheckMenuItem`s on macOS — checked state synced from prefs in `update_menu_enabled_system`; floating pill on Win/Linux), not a Preferences row.

`floor_dots_system` (`main.rs`) — y=0 intersection dots via `FloorDotsGizmos` group (`line.width = 3.0`, `perspective = false`). Each dot is a tiny `+` cross of `tick = 0.05` segments — thick screen-space-constant width makes the short cross render as a round dot. Spacing always 1 voxel (no LOD bands). Window half-extent `(radius * 1.5).clamp(8, 96)` around camera focus; per-dot alpha fades quadratically (`floor_dot_alpha`). No upper radius cap — extreme zoom kills alpha.

`vignette_system` (`EguiPrimaryContextPass` `after(ui_system)`) — 5-vertex `egui::Mesh` (4 dark corners + transparent center) on `Order::Background`. Pseudo-radial darken. Corner alpha light (28/255 dark, 14/255 light).

`draw_origin_system` — RGB axis triad at (0,0,0) via `OriginAxesGizmos` group (`depth_bias = -1.0` reads cleanly over floor dots). Gated on `show_origin_axes`. `triad_fade(near_count)` — full alpha empty, zero at ~8 voxels in 4-cell cube around origin. Wayfinder for empty start, not always-on chrome.

## Lighting

`lighting.rs::spawn_lights` — single `DirectionalLight` anchored at origin (open-world grid has no centroid). `illuminance = 1_500.0`, `shadows_enabled = false`, 2-cascade config, `maximum_distance = 400.0`. Spawned once in `setup_scene`. Voxel materials are `unlit`, so color render is independent of light direction — the directional light affects only any non-unlit gizmo / chrome geometry.

## Color space

`color_space.rs` — sRGB u8 storage everywhere. `ColorSpace` enum (`Hex`/`Rgb`/`Hsl`/`Hsb`/`Oklch`) selects how colors read out; the active space is a persisted preference (`Preferences.color_space`) chosen from the **View → Color Format** menu (native `CheckMenuItem`s on macOS — the active option is checked, synced from prefs in `update_menu_enabled_system`; floating menu pill on Win/Linux), not an inspector control. `ColorSpace::format(rgb)` is the single source of truth for non-editable color strings (under-swatch readout, swatch/recent/palette hover tips); the picker edit fields use `ColorEditBuffer::populate` per-channel slots instead. `widgets::hex_string` is hex-only and now serves just the readout next to the Preferences custom-canvas-color swatch. **Every editable picker is space-aware**: the foreground color, the Preferences custom canvas background, and the Export Shot background all go through `ui::color_picker::space_color_picker_rgb` (via `space_color_swatch` for the two standalone swatches), so each honours `Preferences.color_space` — no raw `egui::color_edit_button_srgb`. Conversions: `parse_hex`, `rgb_to_hsl`/`hsl_to_rgb`, `rgb_to_hsb`/`hsb_to_rgb`, `rgb_to_oklch`/`oklch_to_rgb`. OKLCH path reuses `mesh::srgb_to_linear`/`linear_to_srgb` so the voxel-color pipeline and the inspector roundtrip match. sRGB roundtrip is within ±1/255.

`ColorEditBuffer` (same file) — string slots per space. Repopulated when `CurrentColor` or active space changes so keystrokes don't roundtrip through `Color8` mid-edit (drops hue on greys / quantises OKLCH chroma). Commit on `lost_focus`; invalid silently reverts.

## Snapshot (PNG export)

`snapshot.rs` — one-shot offscreen `Camera3d` at main camera's transform/projection, renders into `Image` at physical window res with `ClearColorConfig::Custom(Color::NONE)`, captures via `Screenshot` pipeline. **Forces `Tonemapping::None`** — tonemap fullscreen pass writes `alpha=1` to every pixel, clobbering transparent clear. Voxel materials are `unlit` so colors are unaffected.

`SnapshotInProgress` set while in flight. `floor_dots_system`, `vignette_system`, `draw_origin_system`, `selection_render_system` early-return on snapshot frame; `brush_preview_system` and `shape_preview_system` hide their ghost/outline and clear `PreviewHide` so the chunk meshes rebuild without the preview swap. `start_snapshot_system` ordered `.before(...)` each. `ScreenshotCaptured` observer clears flag.

## Roxel Shot (social export)

`shot.rs` — polished, framed PNG for sharing (the dribbble look): solid palette-derived background, a plinth/base slab, **real** directional lighting with a soft cast shadow, and a contrast-tinted roxel wordmark watermark. Triggered from command palette "Export Shot…", File → Export → "Roxel Shot (PNG)…" (mac native + Win/Linux pill); the async dialog routes `DialogResult::ExportShot` → `ShotRequest`.

Unlike `snapshot.rs` (live unlit scene, current camera, transparent bg), the shot builds a **throwaway scene on `SHOT_LAYER = 2`** (isolated from editor layer 0 + gizmo layer 1): a lit albedo copy of the model (`mesh::build_lit_mesh` — skips the baked `face_shade` so real lights do the shading, else side/bottom faces double-darken), an **unlit** ground plane in the bg color, a faked soft contact shadow, key + fill `DirectionalLight`s, and a dedicated orthographic `Camera3d`. Everything is despawned and its `Mesh`/`StandardMaterial`/`Image` assets removed in the `ScreenshotCaptured` observer (textures tracked in `session.textures`).

**No auto-plinth.** The creator is expected to build their own base into the model (an auto base double-stacked under a diorama that already had one). The ground plane / shadow / framing all sit at the model's bottom (`base_y = model_min.y`).

**Background = hero-color pastel.** `auto_background(hero_color(cells))`. `hero_color` is the distinct voxel color maximizing `count × saturation² × value³` — the most prominent *vivid, bright* color (grass/water), so a multicolor scene gets real hue instead of a muddy average or a brown dirt base (raw frequency alone picks the dirt). Falls back to the average for an all-grey model. `auto_background` then mixes 44% hero + 56% near-white into a light pastel.

**Real cast shadow — ground plane is `lit` and exposure-calibrated.** The shadow must be tied to the scene (anchored at the base, stretching with the light), so it's a **real** Bevy directional shadow, not a faked quad (faked offset blobs read detached; rectangular footprint quads + tight "lift" quads were also tried and rejected). The key `DirectionalLight` has `shadows_enabled` + a 1-cascade config; the camera carries `ShadowFilteringMethod::Gaussian` for soft edges. The ground plane is **lit** (albedo = `auto_background`) so it can *receive* the shadow — but a lit plane × a strong light clips to white under `Tonemapping::None`, so `KEY_LUX`/`FILL_LUX` (+ the global ambient) are **calibrated so a fully-lit up-facing surface renders ≈ its albedo** (the bg color) while shadowed ground darkens naturally to a colored shadow. Don't crank the lux back up — it whites out the ground. `key_dir ≈ (-0.5,-1,0.32)` (upper back-right) shades the camera-facing front-left and drops the shadow front-left. Fill is kept low so it doesn't wash the cast shadow.

**Soft AO contact shadow** — on top of the real cast shadow, one centered `unlit`+`Blend` quad (`soft_blob_image`, tinted by `shadow_color`) pools darkness around the base. Its mask (`contact_alpha`) is **solid across the footprint then feathers _outward_** to the quad edge (rounded-box `sdf_round_box`, `CONTACT_INNER` = footprint fraction), so the dark band hugs the base perimeter and fades into open ground — a centered *radial* falloff was invisible (its core hid under the model and the edge was already near-zero). This is the *only* faked shadow; the directional one is real.

**Voxel saturation** is `build_lit_mesh(grid, SHOT_SATURATION)`; `SHOT_SATURATION = 1.0` keeps the true palette color (the param exists for a future control — do **not** quietly desaturate, the user wants full color).

**Subtle vignette.** `apply_vignette` multiplies each pixel's RGB by `vignette_factor(dist)` (1.0 center, dipping toward corners) in the CPU post pass before the watermark.

**Render race:** newly spawned meshes/lights must be extracted before the screenshot reads back, so `shot_system` is a 3-phase state machine — build scene → `Warmup(2)` frames → spawn `Screenshot` → observer captures. Skipping warmup captured an empty frame.

Post pass (`finish_shot`) decodes the captured image and composites the watermark (`apply_watermark`): the embedded `assets/branding/roxel-wordmark.png` is a **white silhouette whose alpha is the coverage mask**; `pick_watermark_tint` chooses black/white by the bottom-right region's mean luminance, then `composite_tinted` alpha-blends. Pure helpers (`relative_luma`, `pick_watermark_tint`, `auto_background`, `plinth_colors`, `fit_ortho_height`, `composite_tinted`, …) are unit-tested; no Bevy `App` in tests.

**Tweak panel (phase 2).** Export is driven by a modal tweak panel (`ui::modals::draw_shot_panel`, opened from the command palette / File → Export / Win-Linux pill) over the `ShotPanel` resource — **preview left, knob column right** (`width::SHOT_PREVIEW` + `SHOT_CONTROLS`) so it fits the app window. It shows a live, low-res preview (`PREVIEW_LONG_EDGE = 800`, displayed at `width::SHOT_PREVIEW`) and the art-direction knobs in `ShotParams`: aspect (`ResPreset`), background gradient (`GradientMode` solid/vertical/radial) with `gradient_strength` + `gradient_flip` (light↔dark direction), **grain** (`dither` — per-pixel white noise, not ordered dither), auto-or-custom color, vignette, saturation, lift. Every slider **resets to its default on double-click** (detected via `pointer.button_double_clicked` since the slider's drag sense swallows `Response::double_clicked`); a **Reset** button restores all knobs (`ShotParams::default()`). Triggers refuse to open on an empty scene (toast / disabled pill button). Layout stability: chip rows use `labeled_wrap` (whole chips wrap to a new line instead of clipping; `chip_button` forces `TextWrapMode::Extend` so labels like "3:4" never break mid-text), the right column (`width::SHOT_CONTROLS`) sets its own `clip_rect` so section dividers don't bleed over the preview, and the gradient strength/direction rows are **always rendered and enabled** (they're no-ops for Solid since `gradient_color` ignores them) so the centered modal never changes height / jumps and no washed-out disabled chips appear.

The preview render is split across **two cameras / render layers** captured pixel-aligned: camera A (`SHOT_LAYER`) → lit model RGBA with alpha over a transparent clear; camera B (`GROUND_LAYER`) → a white ground → a per-pixel shadow-factor map. The CPU `composite(…, bg_base, hero)` then paints the background (solid/gradient via `gradient_color`, modulated by the normalized shadow factor), alpha-composites the model, then applies a **full-frame white-noise grain post pass** (`apply_grain` / `dither_offset` — over model + bg + shadows, so the grain reads cohesively rather than only on the flat bg), vignette, and watermark. **Cast shadow and vignette darken via a *multiply* by `multiply_tint(hero)`** — a slightly-darkened hero color — so shadowed ground and frame corners pick up the model's hue instead of going neutral grey (lit pixels at `k=1` are untouched). Because the bg + gradient + shadow tint all live in the composite, **post-only knobs (gradient, strength, flip, color, grain, vignette) recomposite the cached captures instantly** while only scene knobs (saturation, lift, aspect) trigger a GPU re-render. `ShotPanel::note_change(scene)` routes the two paths (`scene_differs` decides); scene-slider drags are **time-debounced** via `defer_scene_render(now)` / `scene_due(now, …)` / `flush_scene_render` so the re-render fires after a ~0.1 s pause or on release, not every frame (a `request_repaint_after` keeps the timer ticking during a motionless hold); `shot_system` prioritizes export > render > recomposite. "Export…" spawns the save dialog → `ShotRequest`, which re-renders at full `ResPreset` resolution. The composite, gradient (strength/flip), dither, multiply-tint, and panel-flag helpers are unit-tested.

## `.rox` project format

`roxel-core/src/io/project.rs` — binary: `"ROXL"` + `u16 FORMAT_VERSION`, then tagged sections (`[u8; 4]` tag, `u32` len). `VOXL` holds a zstd frame: sorted color table, then chunks in sorted coord order, each sparse (`u16` local + index) or dense (`E³` indices), whichever is smaller. `PALT` is the active palette at save time (`ui::palette::active_palette`, scratch edits included); on open, `poll_dialogs_system` hands it to `adopt_project_palette`, which keeps/selects a palette showing the same colors or adds a user palette (then `io::palettes::save`). `PREV` is the optional PNG preview. Layout spec is the module doc comment — keep it current.

- **Additive changes get a new section, not a version bump.** Readers skip unknown tags. Bump `FORMAT_VERSION` only for a change old readers would misread; `decode` refuses newer versions with a toast-able error.
- **Saves are deterministic** — sorted chunks, cells and colors — so re-saving an unchanged scene produces identical bytes. Don't iterate `grid.chunks` (a `HashMap`) straight into the output.
- `decode` is bounds-checked (`LeReader`, capped decompressed size, validated indices and coords) and `load` decodes fully before clearing the grid, so a corrupt file never touches the scene.
- Files without the magic go to `decode_legacy_ron` (pre-0.9 pretty-printed RON `voxels` list). The even older (`version`, `size`, `voxels`) layout loads only because ron ignores unknown fields — **not a compat code path**; don't add explicit handling for it.

## New-project flow

`NewProject { apply }` (`grid.rs`) — no grid size. New triggers call `UnsavedGuard::request(GuardedAction::New)`; the guard sets `apply = true` once it's safe; `apply_new_project_system` consumes next frame: `grid.clear()` (drains chunks → mesher despawns), clear history, drain `VoxelChunkMeshes.chunks`, reset camera to origin / `EMPTY_WORLD_RADIUS`, reset `CurrentProjectPath`/`DocStatus`. **Don't poke `VoxelGrid` directly from UI** — go through `NewProject.apply`.

## Document-dirty tracking & save guards

`DocStatus { saved_state_id }` (`ui/dialogs.rs`) tracks unsaved changes. `History::end` stamps each committed stroke with a monotonic id; `History::state_id()` is the top-of-undo id (`0` when empty). `DocStatus::is_modified(&history)` = `state_id() != saved_state_id`, so undoing back to the saved state reads clean. Save/Open call `mark_saved(state_id())`; imports are an ordinary history stroke, so they dirty the document on their own; New resets it.

`window_title_system` reflects file name + `•` in the OS window title (hidden on macOS — see the inspector Status "File" row instead). **Every action that would drop the document goes through `UnsavedGuard`** (`ui/dialogs.rs`): New, Open (dialog), Open Recent, Finder open, and Quit (window close button / Alt+F4 / Close Window via `request_quit_on_close_system` with `WindowPlugin::close_when_requested = false`, and the custom macOS "Quit Roxel" item — never `PredefinedMenuItem::quit`, which terminates without asking). Triggers call `guard.request(GuardedAction::…)`; `resolve_unsaved_guard_system` runs it at once when clean, or sets `confirming` for `modals::draw_unsaved_confirm` (Save / Don't Save / Cancel; Enter = Save, Esc = Cancel). Save parks the action in `after_save` and spawns `spawn_save`; the action runs only if the save leaves the document clean (a cancelled Save As or a failed save drops it). The guard state machine (`step` / `resolve_confirm`) is pure and unit-tested. Not covered: macOS Dock → Quit and logout still terminate without a prompt (winit doesn't expose `applicationShouldTerminate:`).

## Open from Finder (double-click)

`open_file.rs` — receives `.rox` files the OS hands the app at launch. On macOS a double-clicked document is **not** in `argv` for `.app` bundles; LaunchServices sends a `kAEOpenDocuments` Apple Event, routed by the `CFBundleDocumentTypes`/UTI declaration in `assets/Info.plist.ext` (merged into the bundle Info.plist via `osx_info_plist_exts` in `Cargo.toml`). **Both halves are required** — without the plist, Finder reports "can't open this type of file"; without the handler, double-click opens a blank scene. The in-app File → Open dialog bypasses LaunchServices, which is why it always worked.

The macOS handler (`define_class!` NSObject, objc2) registers itself as the `odoc` Apple Event handler from a `NSApplicationWillFinishLaunchingNotification` observer. **Must be `will`, not `did`:** on a cold launch LaunchServices delivers the event during startup and AppKit dispatches it to its default no-op handler *before* `didFinishLaunching` fires — registering at `did` loses the event and the double-click opens a blank scene (warm launches work either way). `will` runs early enough to override AppKit's default before the queued event is dispatched, yet late enough that AppKit hasn't clobbered us (registering from `install`, before the app exists, is too early). It pushes paths onto a static queue; `poll_open_files_system` drains it and requests `GuardedAction::Open(Some(path))`, so a modified document prompts first and the load reuses `poll_dialogs_system` for recents + dirty baseline + toast. `install()` also queues the last non-flag `argv` entry (`path_from_args`; flags skipped for macOS's `-psn_…`), which is how Windows/Linux file associations and `roxel scene.rox` open a file. It is called once from `main()` before `app.run()`. Single-window app → if several files are dropped at once, the last wins.

## Onboarding

`onboarding.rs` — first-launch coachmark tour. `Onboarding { active, step, anchors_ready, pending_persist, autostart_fired }`. `TOUR_STEPS` is a `&[TourStep]` const with 4 entries: `Viewport`, `ToolRail`, `ColorPalette`, `GizmoCube` (`AnchorId`). `OnboardingAnchors` captures widget rects per frame inside `ui_system`; `Viewport` and `GizmoCube` reuse `ViewportRect` / `GizmoRect` to avoid duplicate plumbing.

Cards do NOT highlight anchored widgets or draw arrows — proximity to the anchor is the only spatial cue. Dismissal persists via `preferences.onboarding_seen`. Relaunchable from Help → Show Onboarding Tour… (native menu on macOS, menu pill on Win/Linux). Modal pauses during dialogs.

Systems: `onboarding_autostart_system` (fires once when prefs say unseen) and `onboarding_overlay_system` (draws cards in `EguiPrimaryContextPass`).

## Updater

`updater.rs` — background GitHub Releases check via `ureq`. `REPO = "starzonmyarmz/roxel"`, `RATE_LIMIT_SECS = 24h`, `HTTP_TIMEOUT_SECS = 10`. `UpdateCheck(UpdateState)` resource is the state machine (idle/checking/ready/failed). `start_check(state, manual)` spawns the HTTP task on `AsyncComputeTaskPool`; `poll_update_check_system` drains via `block_on(future::poll_once)`. `startup_check_system` honours `preferences.last_update_check` rate limit; manual check from command palette ignores it.

`parse_tag` accepts `v1.2.3`, `1.2.3` and semver pre-release/build suffixes (`1.0.0-rc.1`, `+meta` dropped) into a `Version` ordered by semver precedence (a pre-release sorts below its release). `is_newer` compares those. `parse_release_json` uses `serde_json` to deserialize `tag_name` + `html_url` (extra GitHub fields ignored). Toast on newer version; after that `Action::CheckForUpdates` reads "Download Roxel vX.Y.Z…" (`ActionState.update_tag`) in every menu and the palette, and opens the release page instead of checking again — the lasting cue on macOS, where there's no pill badge. The native menu syncs `menu_label` each frame for plain items, so state-dependent menu labels work there too. `open_url` shells out to `open`/`xdg-open`/`start`.

## Actions, menus and shortcuts

`actions.rs` is the one table of user commands. `Action` (enum, `Copy`) has `spec()` → `ActionSpec { label, menu_label, category, keywords, shortcut: Option<Chord> }`, plus state-dependent `label(&ActionState)` / `menu_label` / `keywords` / `enabled` / `checked` (`is_check` for menu check items). `ActionState` is a borrowed world snapshot so the table is testable; `ActionStateParams::get()` builds it in systems, `ui_system` builds it with its local `action_state!` macro. `Action::STATIC` lists every non-indexed action; `Action::all(s)` adds `OpenRecent(i)` and `SelectPalette(i)`. Import/export formats live on `ImportFormat`/`ExportFormat` (filter name, extension, default file name, `DialogResult` ctor).

Every surface pushes `(Action, Origin)` onto `ActionQueue`; `apply_actions_system` → `dispatch` is the only place actions run. Surfaces: the command palette, the macOS menu (`menu/native.rs`), the Win/Linux pill (`ui/floating.rs`), the inspector palette `…` menu and "+" swatch (`ui/inspector.rs`), and `shortcut_system`.

- **Menus**: `menu::menu_bar()` is the shared layout (`MenuEntry::{Item(Action), Separator, Submenu, OpenRecent, System}`), branched per platform (macOS Roxel/Window menus and AppKit `SystemItem`s; Win/Linux folds those into File/Help). The native menu maps each `Chord` to a muda accelerator and syncs enabled/checked for every item from `ActionState` each frame; plain-key items (F, Backspace) and the text-editing chords (Cmd+A/Z/X/C/V, Shift+Cmd+Z) are disabled while egui wants the keyboard (`Action::clashes_with_typing`), since AppKit routes key equivalents before egui sees them.
- **Shortcuts**: `shortcut_system` → pure `shortcut_action(mods, just_pressed, state, ShortcutGate)` picks the first enabled `STATIC` action whose `chords()` match (exact modifiers; numpad/Delete aliases; Ctrl+Y for Redo off macOS). `ShortcutGate` blocks everything while typing or a modal is open, except Cmd+K / Cmd+, closing their own modal. Two actions may share a chord only if their `enabled` never overlap (`SelectTool(Shape)` vs `CycleShape` on S) — `no_two_actions_fire_on_the_same_chord` guards it. On macOS a menu key equivalent consumes its key, so the system only sees chords the menu bar doesn't own or has disabled.
- **Display**: `Chord::tokens(platform)` / `display(platform)` — ⇧⌘S on macOS, Ctrl+Shift+S elsewhere. Palette chips, pill shortcut text, tool tooltips (`Action::shortcut_text`) all go through it. Contextual keys (Esc, arrows, Alt eyedropper, Z-click, Space pan, backquote) stay in their own systems.

## Command palette

`ui/command_palette.rs` owns the resource and draw (draw logic in `src/ui/CLAUDE.md`). `CommandPalette { open, search, selected, just_opened }`. `build_catalog(&ActionState)` maps `Action::all` to `CatalogEntry`s; `draw` returns the chosen `Action` for `ui_system` to queue.

`fuzzy_match(haystack, query)` — case-insensitive subsequence with light scoring (start-of-word bonus, contiguous bonus, gap penalty). `matches_entry` searches `label` then `keywords` (small kw penalty so label hits win). `rank(entries, query)` sorts enabled-first then by score desc — disabled entries still appear, just below.

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
