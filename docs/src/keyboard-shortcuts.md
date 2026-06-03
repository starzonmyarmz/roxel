# Keyboard Shortcuts

## Tools

| Key   | Action                                                         |
| ----- | -------------------------------------------------------------- |
| `B`   | Brush                                                          |
| `E`   | Erase                                                          |
| `P`   | Paint (recolor existing voxel)                                 |
| `I`   | Eyedropper                                                     |
| `S`   | Shape (rect / ellipse / line; 2D or extruded)                  |
| `M`   | Select (drag a 3D AABB on a face plane)                        |
| `V`   | Move (drag a selected voxel; click bare voxel for ad-hoc move) |

## Painting

| Key                    | Action                                                      |
| ---------------------- | ----------------------------------------------------------- |
| `Alt` (hold)           | Temporary eyedropper; releases back to the previous tool    |
| `Shift` + click        | Draw a 3D line from the last placed voxel to the cursor     |

## Selection & Move

| Key                   | Action                                            |
| --------------------- | ------------------------------------------------- |
| `←` / `→` / `↑` / `↓` | Nudge selection 1 voxel on X / Z (Move tool)      |
| `Shift` + `↑` / `↓`   | Nudge selection 1 voxel on Y (Move tool)          |
| `Shift` + drag        | Lock Move drag to the same horizontal plane       |
| `Backspace` / `Delete`| Clear voxels inside the active selection          |

## Camera & view

| Key                      | Action                                       |
| ------------------------ | -------------------------------------------- |
| `Space` + left-drag      | Pan (Figma / Photoshop style)                |
| `Z` + left-click         | Zoom in 2× toward target                     |
| `Alt` + `Z` + left-click | Zoom out 2×                                  |
| `Cmd/Ctrl + =`           | Zoom in 2×                                   |
| `Cmd/Ctrl + -`           | Zoom out 2×                                  |
| `Cmd/Ctrl + 0`           | Frame view on the voxel cluster (panel-aware)|

Right mouse drag orbits the camera. Scroll to zoom.

## History & UI

| Key                    | Action                                     |
| ---------------------- | ------------------------------------------ |
| `Cmd/Ctrl + Z`         | Undo                                       |
| `Cmd/Ctrl + Shift + Z` | Redo                                       |
| `` ` ``                | Focus mode — toggle inspector + chrome     |
