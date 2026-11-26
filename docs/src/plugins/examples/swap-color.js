// Adds a Swap Color section to the inspector. It counts the voxels in the
// current color, and its button swaps them for the color picked in the
// panel. Both work on the selection, or the whole model when nothing is
// selected.

function eachVoxel(ctx, fn) {
  ctx.grid.forEach((x, y, z, color) => {
    if (!ctx.selection || ctx.selection.has(x, y, z)) fn(x, y, z, color);
  });
}

const sameColor = (a, b) => a[0] === b[0] && a[1] === b[1] && a[2] === b[2];

export default {
  apiVersion: 1,
  name: "Swap Color",
  commands: [
    {
      id: "swap",
      label: "Swap Current Color",
      params: [{ id: "to", label: "Swap to", type: "color", default: [240, 200, 80] }],
      run(ctx) {
        eachVoxel(ctx, (x, y, z, color) => {
          if (sameColor(color, ctx.color)) ctx.grid.set(x, y, z, ctx.params.to);
        });
      },
    },
  ],
  panel: {
    title: "Swap Color",
    rows(ctx) {
      let matching = 0;
      const colors = new Set();
      eachVoxel(ctx, (x, y, z, color) => {
        colors.add(color.join());
        if (sameColor(color, ctx.color)) matching++;
      });
      return [
        { type: "text", label: ctx.selection ? "Counting the selection." : "Counting the model." },
        { type: "text", label: "Colors", value: colors.size },
        { type: "text", label: "Current color", value: matching },
        { type: "param", command: "swap", param: "to" },
        { type: "button", command: "swap", label: "Swap" },
      ];
    },
  },
};
