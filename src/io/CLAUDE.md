# src/io/

File import/export and persisted resources. Every supported format lives here.

## Async dialogs are mandatory

Sync `rfd::FileDialog` **blocks winit's event loop on macOS** (beachball). All save/open/export goes through `PendingDialog`:

1. Click → `pending.spawn(async { rfd::AsyncFileDialog... })` on `AsyncComputeTaskPool`
2. `poll_dialogs_system` (Update) calls `block_on(future::poll_once(task))` each frame
3. On `Some(DialogResult::*)` → dispatch to `io::project/vox/qb/gox/obj/gltf/svg/ase` (PNG via `snapshot.rs`)

Buttons disabled while `pending.is_active()`. **Never reintroduce sync `rfd::FileDialog::*` in egui draw code.**

## Shared helpers — use them, don't reroll

- `grid::iter_occupied()` — `(IVec3, Color8)` over all chunks. Use in every exporter.
- `grid::bounding_box()` — `Option<(min, max)>` inclusive.
- `mesh::for_each_exposed_face(grid, |cell, face, rgba| ...)` — per-face quad iteration with occlusion culling. Shared by gltf/svg/obj.
- `io::reader::LeReader` — bounds-checked LE binary reader. Shared by qb/gox import.
- `io::test_util::tmp_path(name, ext)` — `#[cfg(test)]` temp-file helper. Don't add `tempfile`.

## Coordinate handling

- **Axis remap**: `.vox` and `.gox` are Z-up → swap `(x, y_roxel, z_roxel) ↔ (x_vox, z_vox, y_vox)` on import + export. `.qb` is Y-up natively. `.gox` BL16 written as raw 16³ RGBA; PNG-encoded BL16 rejected with clear error.
- **AABB-shift on export** for unsigned-coord formats (`.vox`, `.qb`): translate emitted voxels by `-grid.bounding_box().min` so model min lands at (0,0,0). `.vox` refuses export when any axis extent > 256 (u8 cap) — user gets a toast. Mesh formats (`.obj`/`.gltf`/`.svg`) and `.gox` handle negative coords natively.
- Imports `grid.set` each voxel at the source coord. No resize step, no `snap_to_allowed_size`. `apply_import_system` just consumes the `PendingImport` flag. Cmd+0 to re-frame.

## Format notes

- **`io::gltf::export`** — `.glb` (12-byte header + JSON + BIN chunks, 4-byte aligned). Indexed triangles, `COLOR_0` as u8-norm RGBA, Y-up. Primary DCC interchange — Maya/Max/Unity/Unreal/Blender/Godot all import natively.
- **`io::obj::export`** — Wavefront OBJ + accompanying MTL (one material per distinct color).
- **`io::svg::export`** — per-cell face quads (no greedy merge) sorted by view-space z (painter's order). Same-color consecutive quads batched into one `<path>` with multi-subpaths to cut tag overhead. Viewport trimmed to projected bounds.
- **`io::ase`** — Adobe Swatch Exchange. Binary: ASEF signature, version, block count, then blocks (group-start, group-end, color). Color payload: UTF-16BE name + model tag (RGB/GRAY/CMYK/LAB) + f32 values + type byte (global/spot/normal). Import returns `(name, Vec<[u8; 4]>)`.

## Persisted resources

`io::palettes` persists user palettes (built-ins filtered out) to `{config_dir}/roxel/palettes.ron`. Mutations must call `io::palettes::save(...)` — no autosave.

`io::recent` persists recent `.rox` paths (cap `MAX_RECENT = 10`) to `{config_dir}/roxel/recent.ron`. `RecentFiles` resource (`ui/dialogs.rs`) is hydrated on startup; `poll_dialogs_system` pushes after successful open/save. Drives in-app Open Recent submenu (`ui.rs`) and macOS native File menu (`menu.rs`).

## macOS menu bar

`menu.rs` (`#[cfg(target_os = "macos")]`) installs the native `muda` menu. Four chained Update systems:

- `install_menu_system` — one-shot.
- `poll_menu_events_system` — drains `MenuEvent` → `MenuQueue`.
- `apply_menu_actions_system` — translates queue entries into `PendingDialog`/`History`/`NewProject` writes.
- `update_recent_menu_system` — rebuilds Open Recent when `RecentFiles` changes.

`update_menu_enabled_system` greys undo/redo when stacks are empty. Open Recent reuses `MAX_RECENT` pre-allocated `MenuItem`s (muda doesn't support clean runtime creation).

Menu mirrors `ui.rs` File/Edit — wire new dialog actions into both unless they are mac-only.
