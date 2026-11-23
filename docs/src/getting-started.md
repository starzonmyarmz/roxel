# Getting Started

This walkthrough gets you from a blank grid to a saved model.

## The tour

The first time you open Roxel, a short tour points out the canvas, the tools, the color palette and the orientation cube. Click through it or close it; it won't come back on its own. To see it again, choose **Help → Show Onboarding Tour…**.

## The canvas

Roxel opens on an open-world 3D voxel grid. You can paint anywhere on the horizontal (X / Z) plane and as high as memory allows. The floor at `y = 0` is the only hard boundary — you can't paint below it.

An RGB axis triad marks the world origin so you know which way is which on an empty canvas. It fades out as you build around it, and **View → Origin Axes** turns it off.

## Moving the camera

<a class="roxel-aside" href="images/screenshot.png" data-dark-href="images/screenshot-dark.png"><img src="images/gallery/crop-cube.jpg" alt="The orientation cube" width="118"></a>

- **Right-mouse drag** — orbit
- **Scroll** — zoom
- **`Space` + left-drag** — pan (Figma / Photoshop style)
- **`Cmd/Ctrl + 0`** — frame the view on your voxels (panel-aware)

The default angle is isometric. The orientation cube in the top-right corner orbits when you drag it; click one of its faces to look straight at that side.

## Placing your first voxels

1. The **Brush** tool (`B`) is active by default. Left-drag in the viewport to place voxels.
2. Pick a color from the palette in the left inspector.
3. Switch to **Erase** (`E`) to remove voxels, or **Paint** (`P`) to recolor an existing voxel without moving it.
4. Use the **Eyedropper** (`I`) — or just hold `Alt` — to sample a color off an existing voxel.

> **Tip:** `Shift` + click draws a straight 3D line from the last voxel you placed to the cursor.

## Undo / redo

- **`Cmd/Ctrl + Z`** — undo
- **`Cmd/Ctrl + Shift + Z`** — redo

Roxel keeps the last 200 steps, so experiment freely.

## Finding a command

Press **`Cmd/Ctrl + K`** to open the command palette. Type part of a command's name, like "sphere" or "export", and press `Enter` to run it. Every menu command is there, with its shortcut.

## Saving your work

Roxel's native project format is `.rox`, a compact binary file. Save with `Cmd/Ctrl + S` and open with `Cmd/Ctrl + O`. The **Open Recent** menu keeps your last 10 projects one click away.

To share or use your model elsewhere, see [Import & Export](import-export.md).

## Next steps

- Learn every tool in [Tools](tools.md).
- Memorize the [Keyboard Shortcuts](keyboard-shortcuts.md).
- Build and manage color sets in [Palettes](palettes.md).
