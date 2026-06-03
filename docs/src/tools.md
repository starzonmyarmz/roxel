# Tools

Roxel has seven tools, reachable from the floating tool island or by keyboard. Left-mouse drag in the viewport applies the current tool.

## Brush — `B`

Places voxels in the active color. Drag to paint a stroke. Paints onto the face of existing voxels or onto the build plane in empty space.

## Erase — `E`

Removes voxels under the cursor. Drag to erase a stroke.

## Paint (recolor) — `P`

Recolors an existing voxel in place without adding or removing geometry.

## Eyedropper — `I`

Samples the color of the voxel under the cursor and makes it active. Holding **`Alt`** anytime acts as a temporary eyedropper — release to snap back to your previous tool.

## Shape — `S`

Draws rectangles, ellipses, and lines.

- **2D** on the active build plane, or **extruded** into a 3D box, cylinder, or line.
- **Long-press** the shape rail button to open the primitive picker.
- **Hold `Shift`** while dragging the footprint to lock aspect — square, circle, or 45° line.

## Select — `M`

Drag a 3D axis-aligned bounding box (AABB) on a face plane to define a region. Once selected, you can:

- Bulk **delete** the voxels inside (`Backspace` / `Delete`).
- Bulk **recolor** them.
- Hand the selection to the Move tool.

## Move — `V`

Moves voxels along the picked face plane, or nudge with the arrow keys.

- With a selection active, Move operates on the whole selection.
- Click a bare voxel with **no** selection to move just that single voxel.
- Arrow keys nudge one voxel on X / Z; `Shift` + `↑`/`↓` nudges on Y.
- Hold `Shift` while dragging to lock the move to the same horizontal plane.

See [Keyboard Shortcuts](keyboard-shortcuts.md) for the full key reference.
