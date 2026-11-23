# roxel-core/src/io/

File import/export and persisted resources. Every supported format lives here.

## Async dialogs are mandatory

Sync `rfd::FileDialog` **blocks winit's event loop on macOS** (beachball). All save/open/export goes through `PendingDialog`:

1. Click → `pending.spawn(async { rfd::AsyncFileDialog... })` on `AsyncComputeTaskPool`
2. `poll_dialogs_system` (Update) calls `block_on(future::poll_once(task))` each frame
3. On `Some(DialogResult::*)` → dispatch to `io::project/vox/qb/gox/obj/gltf/svg/ase` (PNG via `snapshot.rs`)

Buttons disabled while `pending.is_active()`. **Never reintroduce sync `rfd::FileDialog::*` in egui draw code.**

`poll_dialogs_system` also maintains the document-dirty baseline (`DocStatus`, see `src/CLAUDE.md`): Save/Open call `doc.mark_saved(history.state_id())`. `.vox`/`.qb`/`.gox` imports go through `dialogs::finish_import`: the file is parsed into a scratch grid (a failed parse never touches the scene), then merged into the scene as one `History` stroke — undoable, and it dirties the document like any edit. The current project path is kept, since the import becomes part of the open document. The "Open project…" dialog (`dialogs::spawn_open`), Open Recent and Finder opens all go through `UnsavedGuard` so an unsaved document confirms first.

## Shared helpers — use them, don't reroll

- `grid::iter_occupied()` — `(IVec3, Color8)` over all chunks. Use in every exporter.
- `grid::bounding_box()` — `Option<(min, max)>` inclusive.
- `mesh_util::greedy_quads(grid, None, None)` — the renderer's greedy mesher (same-color faces merged, chunk by chunk). glTF and OBJ export through it. SVG walks unit faces itself: its painter's sort needs per-cell quads.
- `projection::{collect_cell_quads, project_quads, shade_color}` — camera-facing unit faces, projected to pixels and flat-shaded. SVG export and the CLI's `rox render` share them.
- `io::atomic::write` / `write_with` — temp file + fsync + rename. **Every file the app writes goes through it** (projects, exporters, palettes, recents, preferences, PNGs); never `fs::write`/`File::create` a user-visible file directly.
- `io::reader::LeReader` / `BeReader` — the one bounds-checked binary reader (`Reader<const BE: bool>`). LE for qb/gox import and `.rox` load, BE for `.ase`.
- `io::test_util::tmp_path(name, ext)` — `#[cfg(test)]` temp-file helper. Don't add `tempfile`.

## Coordinate handling

- **Axis remap**: `.vox` and `.gox` are Z-up → swap `(x, y_roxel, z_roxel) ↔ (x_vox, z_vox, y_vox)` on import + export. `.qb` is Y-up natively. `.gox` BL16 written as raw 16³ RGBA; PNG-encoded BL16 rejected with clear error.
- **AABB-shift on `.vox` export** (unsigned coords): translate emitted voxels by `-grid.bounding_box().min` so model min lands at (0,0,0). `.vox` refuses export when any axis extent > 256 (u8 cap) — user gets a toast. Mesh formats (`.obj`/`.gltf`/`.svg`) and `.gox` handle negative coords natively.
- Format importers `grid.set` each voxel at the source coord into the scratch grid they're handed (never the live scene). No resize step, no `snap_to_allowed_size`. `apply_import_system` just consumes the `PendingImport` flag. Cmd+0 to re-frame.

**`io::vox::import` walks the scene graph** (`place_models` / `collect_instances`): every visible `nSHP` instance is placed through its accumulated `nTRN` frame-0 transform (`Xform`: signed-permutation `_r` + `_t`, rotating about the model center in doubled coordinates), hidden nodes/layers skipped, depth-capped against cycles. `_r` is parsed by `parse_rotation`, not dot_vox's `Frame::orientation` (that one panics on an invalid byte). The whole import is shifted so the lowest model-box corner lands at the origin, which keeps a lone untransformed model at its raw voxel coords. Files without a scene graph place each model untransformed.

**`io::vox::import` also returns a palette** — the distinct colors actually placed (ascending palette-index order, deduped; the full 256-entry MV ramp is *not* returned, unused entries are noise). `poll_dialogs_system`'s `ImportVox` arm pushes them as a new user `Palette` (named after the file stem via `palette::unique_palette_name`), selects it, and calls `io::palettes::save`. Mirrors the `.ase` import path. `.qb`/`.gox` imports remain grid-only.

## Format notes

- **`io::gltf::export`** — `.glb` (12-byte header + JSON + BIN chunks, 4-byte aligned). Greedy quads as indexed triangles, `COLOR_0` as u8-norm RGBA, Y-up. Primary DCC interchange — Maya/Max/Unity/Unreal/Blender/Godot all import natively.
- **`io::obj::export`** — Wavefront OBJ + accompanying MTL (one material per distinct color).
- **`io::svg::export`** — per-cell face quads (no greedy merge) sorted by view-space z (painter's order). Same-color consecutive quads batched into one `<path>` with multi-subpaths to cut tag overhead. Viewport trimmed to projected bounds.
- **`io::ase`** — Adobe Swatch Exchange. Binary: ASEF signature, version, block count, then blocks (group-start, group-end, color). Color payload: UTF-16BE name + model tag (RGB/GRAY/CMYK/LAB) + f32 values + type byte (global/spot/normal). Import returns `(name, Vec<[u8; 4]>)`.

## Persisted resources

`io::palettes` persists user palettes (built-ins filtered out) to `{config_dir}/roxel/palettes.ron`. Mutations must call `io::palettes::save(...)` — no autosave. **A file that won't parse is never overwritten:** `load` renames it to `palettes.ron.corrupt-<unix secs>` (`LoadIssue::MovedAside`); if it can't be read or renamed, `save` is turned off for the session (`LoadIssue::SaveDisabled`). `main` toasts `LoadIssue::message()` at startup.

`io::set_config_read_only()` turns off `palettes::save`, `recent::save` and the app's `save_preferences` for the rest of the process (scripted capture mode uses it). Every new config-file writer must check `io::config_writable()`.

`io::recent` persists recent `.rox` paths (cap `MAX_RECENT = 10`) to `{config_dir}/roxel/recent.ron`. `RecentFiles` resource (`ui/dialogs.rs`) is hydrated on startup; `poll_dialogs_system` pushes after successful open/save. Drives in-app Open Recent submenu (`ui.rs`) and macOS native File menu (`menu.rs`).
