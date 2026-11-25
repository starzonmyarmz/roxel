// Recolors voxels with a vertical gradient: black at the bottom, the
// current color at the top. It covers the selection, or the whole model
// when nothing is selected. Empty cells stay empty.
export default {
  apiVersion: 1,
  name: "Gradient",
  commands: [
    {
      id: "vertical",
      label: "Vertical Gradient",
      run(ctx) {
        const box = ctx.selection ? ctx.selection.bounds : ctx.grid.bounds();
        if (!box) return;
        const bottom = box.min[1];
        const top = box.max[1];
        const recolor = (x, y, z) => {
          const old = ctx.grid.get(x, y, z);
          if (old === null) return;
          const t = top === bottom ? 1 : (y - bottom) / (top - bottom);
          const [r, g, b] = ctx.color.map((c) => Math.round(c * t));
          ctx.grid.set(x, y, z, [r, g, b, old[3]]);
        };
        if (ctx.selection) ctx.selection.forEach(recolor);
        else ctx.grid.forEach(recolor);
      },
    },
  ],
};
