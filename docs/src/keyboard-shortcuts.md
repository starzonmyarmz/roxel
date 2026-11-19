# Keyboard Shortcuts

On macOS use `Cmd`; on Windows and Linux use `Ctrl`. Shortcuts don't fire while you're typing in a text field or while a dialog is open; `Cmd/Ctrl + K` and `Cmd/Ctrl + ,` still close the command palette and Preferences.

Open **Help → Keyboard Shortcuts** to see these in the app, with your platform's modifier keys.

## File

| Key                    | Action                 |
| ---------------------- | ---------------------- |
| `Cmd/Ctrl + N`         | New project            |
| `Cmd/Ctrl + O`         | Open…                  |
| `Cmd/Ctrl + S`         | Save                   |
| `Cmd/Ctrl + Shift + S` | Save As…               |
| `Cmd + Q`              | Quit Roxel (macOS)     |

## General

| Key            | Action                                                    |
| -------------- | --------------------------------------------------------- |
| `Cmd/Ctrl + K` | Command palette: search and run any command by name       |
| `Cmd/Ctrl + ,` | Preferences                                               |
| `Esc`          | Close the open dialog or command palette; cancel a drag; clear the selection; stop the flyby camera |
| `Enter`        | Run the highlighted command; confirm a dialog's default button |

## Tools

| Key   | Action                                                         |
| ----- | -------------------------------------------------------------- |
| `B`   | Brush                                                          |
| `E`   | Erase                                                          |
| `P`   | Paint (recolor existing voxels)                                |
| `I`   | Eyedropper                                                     |
| `S`   | Shape; press again to cycle rectangle → ellipse → line → sphere |
| `M`   | Select (drag a 3D box on a face plane)                         |
| `V`   | Move (drag a selected voxel; click a bare voxel to move just that one) |

## Painting

| Key                    | Action                                                      |
| ---------------------- | ----------------------------------------------------------- |
| `Alt` (hold)           | Temporary eyedropper; releases back to the previous tool    |
| `Shift` + click        | Draw a 3D line from the last placed voxel to the cursor     |
| Double-click (Paint)   | Recolor every connected voxel of the same color             |

## Selection, clipboard & Move

| Key                   | Action                                            |
| --------------------- | ------------------------------------------------- |
| `Cmd/Ctrl + A`        | Select all voxels                                 |
| `Cmd/Ctrl + D`        | Clear the selection                               |
| `Cmd/Ctrl + C`        | Copy the selection                                |
| `Cmd/Ctrl + X`        | Cut the selection                                 |
| `Cmd/Ctrl + V`        | Paste at the cursor (or onto the selection)       |
| `Backspace` / `Delete`| Clear voxels inside the active selection          |
| `F`                   | Fill the selection with the current color         |
| `←` / `→` / `↑` / `↓` | Nudge selection 1 voxel on X / Z (Move tool)      |
| `Shift` + `↑` / `↓`   | Nudge selection 1 voxel on Y (Move tool)          |
| `Shift` + drag        | Lock Move drag to the same horizontal plane       |

## Camera & view

| Key                      | Action                                       |
| ------------------------ | -------------------------------------------- |
| `Space` + left-drag      | Pan (Figma / Photoshop style)                |
| `Z` + left-click         | Zoom in 2× toward target                     |
| `Alt` + `Z` + left-click | Zoom out 2×                                  |
| `Cmd/Ctrl + =`           | Zoom in 2×                                   |
| `Cmd/Ctrl + -`           | Zoom out 2×                                  |
| `Cmd/Ctrl + 0`           | Frame view on the voxel cluster (panel-aware)|
| `Cmd/Ctrl + 1`           | Front view (`Shift` for Back)                |
| `Cmd/Ctrl + 3`           | Right view (`Shift` for Left)                |
| `Cmd/Ctrl + 5`           | Isometric view                               |
| `Cmd/Ctrl + 7`           | Top view                                     |

Right mouse drag orbits the camera. Scroll to zoom. The orientation cube in the top-right corner orbits when you drag it; click one of its faces to look straight at that side.

## History & UI

| Key                    | Action                                     |
| ---------------------- | ------------------------------------------ |
| `Cmd/Ctrl + Z`         | Undo                                       |
| `Cmd/Ctrl + Shift + Z` | Redo                                       |
| `Ctrl + Y`             | Redo (Windows and Linux)                   |
| `` ` ``                | Focus mode — toggle inspector + chrome     |
