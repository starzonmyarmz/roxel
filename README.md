# Roxel

Minimal voxel editor built with [Bevy](https://bevyengine.org/) and [egui](https://github.com/emilk/egui).

## Features

- Paint, erase, fill, and eyedrop on a fixed 3D voxel grid
- Orbit / pan / zoom camera (powered by `bevy_panorbit_camera`)
- Undo / redo history
- Adjustable directional light (azimuth, elevation, intensity)
- Save / load `.roxel` project files (RON)
- Export to MagicaVoxel `.vox` and Wavefront `.obj`

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
| `Cmd/Ctrl + Z` | Undo |
| `Cmd/Ctrl + Shift + Z` | Redo |
| `F` | Frame view on the voxel cluster |

Left mouse drag in the viewport applies the current tool. Hold `Space` + drag left mouse to pan (Figma/Photoshop style). Right mouse drag orbits the camera. Scroll to zoom.

## Project layout

```
src/
  main.rs       app + plugin setup
  ui.rs         egui panels, toolbar, async file dialogs
  tools.rs      brush/erase/paint/eyedropper + shortcuts + undo
  picking.rs    ray → voxel hit testing
  grid.rs       VoxelGrid resource
  mesh.rs       voxel → mesh regeneration
  camera.rs     pan-orbit camera setup
  lighting.rs   directional light controls
  gizmo.rs      axis gizmo overlay
  history.rs    undo/redo stacks
  io/
    project.rs  .roxel save/load (RON)
    vox.rs      .vox export
    obj.rs      .obj export
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
