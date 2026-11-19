# Import & Export

Roxel interoperates with the common voxel, 3D, and image formats. Every save / load / import / export reports its result with an in-app toast notification.

## Projects

Roxel's native format is **`.rox`**, a compact, compressed project file. Projects saved by older versions (which wrote human-readable RON) still open; they're converted the next time you save. A project remembers the palette you were using; see [Palettes](palettes.md#palettes-in-projects). Save and load from the menu. The **Open Recent** menu lists your last 10 projects. You can also open a project by passing its path on the command line, e.g. `roxel scene.rox`.

If your project has unsaved changes, Roxel asks before anything replaces it — starting a new project, opening another one (from the menu, Open Recent or by double-clicking a file), or quitting. Choose **Save** to save first, **Don't Save** to discard your changes, or **Cancel** to go back. Pressing Enter picks Save.

## Importing models

| Format | Source         |
| ------ | -------------- |
| `.vox` | MagicaVoxel    |
| `.qb`  | Qubicle        |
| `.gox` | Goxel          |

An imported model is added to the current scene at its original coordinates. A MagicaVoxel scene with several models comes in whole, each model placed and rotated as in MagicaVoxel; hidden models and layers are skipped. The whole import is one step you can undo. If a file can't be read, nothing in your scene changes.

## Exporting models

| Format | Target / use                          |
| ------ | ------------------------------------- |
| `.vox` | MagicaVoxel                           |
| `.gox` | Goxel                                 |
| `.obj` | Wavefront OBJ (mesh)                  |
| `.glb` | glTF binary — Unity / Godot ready     |
| `.png` | Transparent raster image             |
| `.svg` | Vector image                          |

A `.vox` file holds at most 255 colors. If your scene uses more, the most-used colors are kept exactly, the rest are matched to the nearest kept color, and the export notification says how many.

## Palettes

Roxel imports and exports **Adobe Swatch Exchange** (`.ase`) palette files. See [Palettes](palettes.md).
