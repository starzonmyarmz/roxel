// Mirrors the selection, or the whole model when nothing is selected, in
// place: left to right, or top to bottom. Both commands have a button on
// the tool rail.

function mirror(ctx, axis) {
  const box = ctx.selection ? ctx.selection.bounds : ctx.grid.bounds();
  if (!box) return;
  const cells = [];
  const take = (x, y, z) => {
    const color = ctx.grid.get(x, y, z);
    if (color !== null) cells.push([[x, y, z], color]);
  };
  if (ctx.selection) ctx.selection.forEach(take);
  else ctx.grid.forEach(take);
  for (const [p] of cells) ctx.grid.clear(...p);
  for (const [p, color] of cells) {
    const q = [...p];
    q[axis] = box.min[axis] + box.max[axis] - p[axis];
    ctx.grid.set(...q, color);
  }
}

export default {
  apiVersion: 1,
  name: "Mirror",
  commands: [
    { id: "x", label: "Mirror Left to Right", run: (ctx) => mirror(ctx, 0) },
    { id: "y", label: "Mirror Top to Bottom", run: (ctx) => mirror(ctx, 1) },
  ],
  toolbar: [
    { command: "x", icon: "flip-horizontal-2" },
    { command: "y", icon: "flip-vertical-2" },
  ],
};
