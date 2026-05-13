# Roxel

Minimal voxel editor built with [Bevy](https://bevyengine.org/) and [egui](https://github.com/emilk/egui).

## Features

- Brush, erase, recolor, eyedrop, and shape tools on a variable-size 3D voxel grid (32 / 64 / 96 / 128 per axis)
- Shape tool: rectangle, ellipse, line — 2D on the active build plane or extruded into a 3D box / cylinder / line
- Translucent brush + shape previews with a contrast-aware outline (toggleable in Preferences)
- Shift+click line draw between the last placed voxel and the cursor
- Built-in palettes (Sweetie 16, PICO-8, DawnBringer 16/32, Endesga 32, NA16, Basic) plus user palettes — add a swatch, new/duplicate/rename/delete, drag to reorder, persisted to `palettes.ron`
- Orbit / pan / zoom camera (powered by `bevy_panorbit_camera`) with isometric default angle
- Panel-aware `F` frame-view and live zoom % readout
- Per-chunk greedy mesher — large grids stay snappy because only edited chunks rebuild
- Undo / redo history (capped at 200 strokes)
- New-project dialog with a grid-size picker (resets camera + planes)
- Save / load `.roxel` project files (RON)
- Export to MagicaVoxel `.vox`, Wavefront `.obj`, Autodesk `.fbx` (binary 7.4), transparent `.png`, and `.svg`
- Import / export Adobe Swatch Exchange `.ase` palettes
- Light / Dark / System themes with separate canvas, floor, and wall color preferences (persisted to `~/.config/roxel/preferences.ron`)
- Toggle floor / back+left walls independently
- Custom UI fonts: Nunito (400/500/600/700) for UI text, DM Mono (400) for hex codes and stats
- Native macOS menu bar (File / Edit / View shortcuts wired into the editor)
- App icon for window/dock + bundled `.icns` for `cargo bundle`

## Run

```sh
cargo run --release
```

Dev profile uses `opt-level = 1` for the crate and `opt-level = 3` for deps to keep iteration fast.

## Controls

| Key                      | Action                                                  |
| ------------------------ | ------------------------------------------------------- |
| `B`                      | Brush                                                   |
| `E`                      | Erase                                                   |
| `P`                      | Paint (recolor existing voxel)                          |
| `I`                      | Eyedropper                                              |
| `S`                      | Shape (rect / ellipse / line; 2D or extruded)           |
| `Alt` (hold)             | Temporary eyedropper; releases back to previous tool    |
| `Shift` + click          | Draw a 3D line from the last placed voxel to the cursor |
| `Space` + left-drag      | Pan (Figma/Photoshop style)                             |
| `Z` + left-click         | Zoom in 2× toward target                                |
| `Alt` + `Z` + left-click | Zoom out 2×                                             |
| `Cmd/Ctrl + Z`           | Undo                                                    |
| `Cmd/Ctrl + Shift + Z`   | Redo                                                    |
| `F`                      | Frame view on the voxel cluster (panel-aware)           |

Cursor reflects the active modifier: crosshair (default), grab (Space or gizmo hover), grabbing (RMB orbit or gizmo drag), zoom-in/zoom-out (Z / Alt+Z), pointing hand (Alt sticky-eyedropper). The brush/shape preview hides while orbiting (RMB) or dragging the orientation gizmo so it doesn't distract.

Left mouse drag in the viewport applies the current tool. Right mouse drag orbits the camera. Scroll to zoom.

## Project layout

```
src/
  main.rs           app + plugin setup, scene/floor/wall spawn, new-project apply
  ui.rs             egui panels, toolbar, palette UI, async file dialogs
  menu.rs           macOS native menu bar (File / Edit + accelerators)
  theme.rs          Theme resource, Preferences (theme + canvas/floor/wall + outline), font setup
  icon.rs           window + macOS dock icon
  tools.rs          brush/erase/paint/eyedropper/shape + line draw + shortcuts + undo
  preview.rs        translucent brush-target ghost cuboid + outline
  shape_preview.rs  shape tool ghost mesh + outline
  picking.rs        ray → voxel hit testing (DDA + floor fallback)
  shapes.rs         rect / ellipse / line2d cell generators + extrude
  grid.rs           VoxelGrid resource, chunk dirty flags, New-project state
  mesh.rs           chunked greedy mesher (sRGB-linear vertex colors)
  camera.rs         pan-orbit camera setup, panel-aware frame-view, Z-key zoom
  lighting.rs       fixed scene lighting
  gizmo.rs          orientation gizmo overlay + drag-to-orbit
  history.rs        undo/redo stacks
  snapshot.rs       transparent PNG export pipeline
  io/
    project.rs      .roxel save/load (RON)
    vox.rs          .vox export
    obj.rs          .obj export
    fbx.rs          .fbx export (binary 7.4)
    svg.rs          .svg export of current view
    ase.rs          .ase palette import/export
    palettes.rs     user-palette persistence (palettes.ron)
```

## Packaging a macOS .app

```sh
cargo install cargo-bundle
cargo bundle --release
open target/release/bundle/osx/Roxel.app
```

The bundle picks up `assets/icons/roxel.icns` via `[package.metadata.bundle]` in `Cargo.toml`.

## File format

`.roxel` projects are RON-serialized:

```rust
struct ProjectFile {
    version: u32,
    size: [u32; 3],
    voxels: Vec<([i32; 3], Color8)>,
}
```

## License

MIT — see [LICENSE](LICENSE).
