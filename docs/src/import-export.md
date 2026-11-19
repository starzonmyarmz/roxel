# Import & Export

Roxel interoperates with the common voxel, 3D, and image formats. Every save / load / import / export reports its result with an in-app toast notification.

## Projects

Roxel's native format is **`.rox`** — a human-readable RON project file. Save and load from the menu. The **Open Recent** menu lists your last 10 projects.

## Importing models

| Format | Source         |
| ------ | -------------- |
| `.vox` | MagicaVoxel    |
| `.qb`  | Qubicle        |
| `.gox` | Goxel          |

An imported model is added to the current scene at its original coordinates. The whole import is one step you can undo. If a file can't be read, nothing in your scene changes.

## Exporting models

| Format | Target / use                          |
| ------ | ------------------------------------- |
| `.vox` | MagicaVoxel                           |
| `.gox` | Goxel                                 |
| `.obj` | Wavefront OBJ (mesh)                  |
| `.glb` | glTF binary — Unity / Godot ready     |
| `.png` | Transparent raster image             |
| `.svg` | Vector image                          |

## Palettes

Roxel imports and exports **Adobe Swatch Exchange** (`.ase`) palette files. See [Palettes](palettes.md).
