# Getting Started

This walkthrough gets you from a blank grid to a saved model.

## The canvas

Roxel opens on an open-world 3D voxel grid. You can paint anywhere on the horizontal (X / Z) plane and as high as memory allows. The floor at `y = 0` is the only hard boundary — you can't paint below it.

A permanent RGB axis triad marks the world origin so you always know which way is which.

## Moving the camera

- **Right-mouse drag** — orbit
- **Scroll** — zoom
- **`Space` + left-drag** — pan (Figma / Photoshop style)
- **`Cmd/Ctrl + 0`** — frame the view on your voxels (panel-aware)

The default angle is isometric.

## Placing your first voxels

1. The **Brush** tool (`B`) is active by default. Left-drag in the viewport to place voxels.
2. Pick a color from the palette in the left inspector.
3. Switch to **Erase** (`E`) to remove voxels, or **Paint** (`P`) to recolor an existing voxel without moving it.
4. Use the **Eyedropper** (`I`) — or just hold `Alt` — to sample a color off an existing voxel.

> **Tip:** `Shift` + click draws a straight 3D line from the last voxel you placed to the cursor.

## Undo / redo

- **`Cmd/Ctrl + Z`** — undo
- **`Cmd/Ctrl + Shift + Z`** — redo

Roxel keeps a deep undo history, so experiment freely.

## Saving your work

Roxel's native project format is `.rox` (a human-readable RON file). Save and load from the menu. The **Open Recent** menu keeps your last 10 projects one click away.

To share or use your model elsewhere, see [Import & Export](import-export.md).

## Next steps

- Learn every tool in [Tools](tools.md).
- Memorize the [Keyboard Shortcuts](keyboard-shortcuts.md).
- Build and manage color sets in [Palettes](palettes.md).
