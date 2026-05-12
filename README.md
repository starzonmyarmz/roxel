# Roxel

Minimal voxel editor built with [Bevy](https://bevyengine.org/) and [egui](https://github.com/emilk/egui).

## Features

- Brush, erase, recolor, and eyedrop on a fixed 3D voxel grid
- Translucent brush preview at the placement target
- Shift+click line draw between the last placed voxel and the cursor
- Built-in palettes (Sweetie 16, PICO-8, DawnBringer 16/32, Endesga 32, NA16, Basic)
- Orbit / pan / zoom camera (powered by `bevy_panorbit_camera`)
- Undo / redo history
- Save / load `.roxel` project files (RON)
- Export to MagicaVoxel `.vox`, Wavefront `.obj`, Autodesk `.fbx` (binary 7.4), transparent `.png`, and `.svg`
- Import / export Adobe Swatch Exchange `.ase` palettes
- Light / Dark / System themes (persisted to `~/.config/roxel/preferences.ron`)
- Custom UI fonts: Nunito (400/500/600/700) for UI text, DM Mono (400) for hex codes and stats
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
| `Alt` (hold)             | Temporary eyedropper; releases back to previous tool    |
| `Shift` + click          | Draw a 3D line from the last placed voxel to the cursor |
| `Space` + left-drag      | Pan (Figma/Photoshop style)                             |
| `Z` + left-click         | Zoom in 2× toward target                                |
| `Alt` + `Z` + left-click | Zoom out 2×                                             |
| `Cmd/Ctrl + Z`           | Undo                                                    |
| `Cmd/Ctrl + Shift + Z`   | Redo                                                    |
| `F`                      | Frame view on the voxel cluster                         |

Cursor reflects the active modifier: crosshair (default), grab (Space), zoom-in/zoom-out (Z / Alt+Z), move (RMB orbit), pointing hand (Alt sticky-eyedropper).

Left mouse drag in the viewport applies the current tool. Right mouse drag orbits the camera. Scroll to zoom.

## Project layout

```
src/
  main.rs       app + plugin setup
  ui.rs         egui panels, toolbar, palette UI, async file dialogs
  theme.rs      Theme resource, Preferences (Light/Dark/System), font setup
  icon.rs       window + macOS dock icon
  tools.rs      brush/erase/paint/eyedropper + line draw + shortcuts + undo
  preview.rs    translucent brush-target ghost cuboid
  picking.rs    ray → voxel hit testing
  grid.rs       VoxelGrid resource
  mesh.rs       voxel → mesh regeneration (sRGB-linear vertex colors)
  camera.rs     pan-orbit camera setup, frame-view, Z-key zoom
  lighting.rs   fixed scene lighting
  gizmo.rs      axis gizmo overlay
  history.rs    undo/redo stacks
  snapshot.rs   transparent PNG export pipeline
  io/
    project.rs  .roxel save/load (RON)
    vox.rs      .vox export
    obj.rs      .obj export
    fbx.rs      .fbx export (binary 7.4)
    svg.rs      .svg export of current view
    ase.rs      .ase palette import/export
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
