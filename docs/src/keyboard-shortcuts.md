# Keyboard Shortcuts

On macOS use <kbd>Cmd</kbd>; on Windows and Linux use <kbd>Ctrl</kbd>. Shortcuts don't fire while you're typing in a text field or while a dialog is open; <kbd>Cmd/Ctrl</kbd> + <kbd>K</kbd> and <kbd>Cmd/Ctrl</kbd> + <kbd>,</kbd> still close the command palette and Preferences.

Open **Help → Keyboard Shortcuts** to see these in the app, with your platform's modifier keys.

## File

| Key                    | Action                 |
| ---------------------- | ---------------------- |
| <kbd>Cmd/Ctrl</kbd> + <kbd>N</kbd>         | New project            |
| <kbd>Cmd/Ctrl</kbd> + <kbd>O</kbd>         | Open…                  |
| <kbd>Cmd/Ctrl</kbd> + <kbd>S</kbd>         | Save                   |
| <kbd>Cmd/Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>S</kbd> | Save As…               |
| <kbd>Cmd</kbd> + <kbd>Q</kbd>              | Quit Roxel (macOS)     |

## General

| Key            | Action                                                    |
| -------------- | --------------------------------------------------------- |
| <kbd>Cmd/Ctrl</kbd> + <kbd>K</kbd> | Command palette: search and run any command by name       |
| <kbd>Cmd/Ctrl</kbd> + <kbd>,</kbd> | Preferences                                               |
| <kbd>Esc</kbd>          | Close the open dialog or command palette; cancel a drag; clear the selection; stop the flyby camera |
| <kbd>Enter</kbd>        | Run the highlighted command; confirm a dialog's default button |

## Tools

| Key   | Action                                                         |
| ----- | -------------------------------------------------------------- |
| <kbd>B</kbd>   | Brush                                                          |
| <kbd>E</kbd>   | Erase                                                          |
| <kbd>P</kbd>   | Paint (recolor existing voxels)                                |
| <kbd>I</kbd>   | Eyedropper                                                     |
| <kbd>S</kbd>   | Shape; press again to cycle rectangle → ellipse → line → sphere |
| <kbd>M</kbd>   | Select (drag a 3D box on a face plane)                         |
| <kbd>V</kbd>   | Move (drag a selected voxel; click a bare voxel to move just that one) |

## Painting

| Key                    | Action                                                      |
| ---------------------- | ----------------------------------------------------------- |
| <kbd>Alt</kbd> (hold)           | Temporary eyedropper; releases back to the previous tool    |
| <kbd>Shift</kbd> + click        | Draw a 3D line from the last placed voxel to the cursor     |
| Double-click (Paint)   | Recolor every connected voxel of the same color             |

## Selection, clipboard & Move

| Key                   | Action                                            |
| --------------------- | ------------------------------------------------- |
| <kbd>Cmd/Ctrl</kbd> + <kbd>A</kbd>        | Select all voxels                                 |
| <kbd>Cmd/Ctrl</kbd> + <kbd>D</kbd>        | Clear the selection                               |
| <kbd>Cmd/Ctrl</kbd> + <kbd>C</kbd>        | Copy the selection                                |
| <kbd>Cmd/Ctrl</kbd> + <kbd>X</kbd>        | Cut the selection                                 |
| <kbd>Cmd/Ctrl</kbd> + <kbd>V</kbd>        | Paste at the cursor (or onto the selection)       |
| <kbd>Backspace</kbd> / <kbd>Delete</kbd>| Clear voxels inside the active selection          |
| <kbd>F</kbd>                   | Fill the selection with the current color         |
| <kbd>←</kbd> / <kbd>→</kbd> / <kbd>↑</kbd> / <kbd>↓</kbd> | Nudge selection 1 voxel on X / Z (Move tool)      |
| <kbd>Shift</kbd> + <kbd>↑</kbd> / <kbd>↓</kbd>   | Nudge selection 1 voxel on Y (Move tool)          |
| <kbd>Shift</kbd> + drag        | Lock Move drag to the same horizontal plane       |

## Camera & view

| Key                      | Action                                       |
| ------------------------ | -------------------------------------------- |
| <kbd>Space</kbd> + left-drag      | Pan (Figma / Photoshop style)                |
| <kbd>Z</kbd> + left-click         | Zoom in 2× toward target                     |
| <kbd>Alt</kbd> + <kbd>Z</kbd> + left-click | Zoom out 2×                                  |
| <kbd>Cmd/Ctrl</kbd> + <kbd>=</kbd>           | Zoom in 2×                                   |
| <kbd>Cmd/Ctrl</kbd> + <kbd>-</kbd>           | Zoom out 2×                                  |
| <kbd>Cmd/Ctrl</kbd> + <kbd>0</kbd>           | Frame view on the voxel cluster (panel-aware)|
| <kbd>Cmd/Ctrl</kbd> + <kbd>1</kbd>           | Front view (<kbd>Shift</kbd> for Back)                |
| <kbd>Cmd/Ctrl</kbd> + <kbd>3</kbd>           | Right view (<kbd>Shift</kbd> for Left)                |
| <kbd>Cmd/Ctrl</kbd> + <kbd>5</kbd>           | Isometric view                               |
| <kbd>Cmd/Ctrl</kbd> + <kbd>7</kbd>           | Top view                                     |

Right mouse drag orbits the camera. Scroll to zoom. The orientation cube in the top-right corner orbits when you drag it; click one of its faces to look straight at that side.

## History & UI

| Key                    | Action                                     |
| ---------------------- | ------------------------------------------ |
| <kbd>Cmd/Ctrl</kbd> + <kbd>Z</kbd>         | Undo                                       |
| <kbd>Cmd/Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Z</kbd> | Redo                                       |
| <kbd>Ctrl</kbd> + <kbd>Y</kbd>             | Redo (Windows and Linux)                   |
| <kbd>&#96;</kbd>                | Focus mode — toggle inspector + chrome     |
