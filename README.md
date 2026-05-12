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
- Export to MagicaVoxel `.vox` and Wavefront `.obj`
- Import / export Adobe Swatch Exchange `.ase` palettes

## Run

```sh
cargo run --release
```

Dev profile uses `opt-level = 1` for the crate and `opt-level = 3` for deps to keep iteration fast.

## Controls

| Key | Action |
|-----|--------|
| `B` | Brush |
| `E` | Erase |
| `P` | Paint (recolor existing voxel) |
| `I` | Eyedropper |
| `Alt` (hold) | Temporary eyedropper; releases back to previous tool |
| `Shift` + click | Draw a 3D line from the last placed voxel to the cursor |
| `Space` + left-drag | Pan (Figma/Photoshop style) |
| `Cmd/Ctrl + Z` | Undo |
| `Cmd/Ctrl + Shift + Z` | Redo |
| `F` | Frame view on the voxel cluster |

Left mouse drag in the viewport applies the current tool. Right mouse drag orbits the camera. Scroll to zoom.

## Project layout

```
src/
  main.rs       app + plugin setup
  ui.rs         egui panels, toolbar, palette UI, async file dialogs
  tools.rs      brush/erase/paint/eyedropper + line draw + shortcuts + undo
  preview.rs    translucent brush-target ghost cuboid
  picking.rs    ray → voxel hit testing
  grid.rs       VoxelGrid resource
  mesh.rs       voxel → mesh regeneration (sRGB-linear vertex colors)
  camera.rs     pan-orbit camera setup
  lighting.rs   fixed scene lighting
  gizmo.rs      axis gizmo overlay
  history.rs    undo/redo stacks
  io/
    project.rs  .roxel save/load (RON)
    vox.rs      .vox export
    obj.rs      .obj export
    ase.rs      .ase palette import/export
```

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

Unlicensed / personal project.
