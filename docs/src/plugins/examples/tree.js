// Plants an oak: a trunk with a round crown of leaves. It stands on the
// floor under the middle of the selection, or at the origin. Its params
// set the trunk height, the crown size and both colors.
export default {
  apiVersion: 1,
  name: "Tree",
  commands: [
    {
      id: "oak",
      label: "Oak Tree",
      params: [
        { id: "height", label: "Height", type: "int", min: 1, max: 32, default: 6 },
        { id: "radius", label: "Crown", type: "int", min: 1, max: 12, default: 3 },
        { id: "trunk", label: "Trunk", type: "color", default: [96, 62, 40] },
        { id: "leaves", label: "Leaves", type: "color", default: [124, 190, 60] },
      ],
      run(ctx) {
        const { height, radius, trunk, leaves } = ctx.params;

        let [bx, by, bz] = [0, 0, 0];
        if (ctx.selection) {
          const { min, max } = ctx.selection.bounds;
          bx = Math.floor((min[0] + max[0]) / 2);
          by = min[1];
          bz = Math.floor((min[2] + max[2]) / 2);
        }

        for (let y = 0; y < height; y++) ctx.grid.set(bx, by + y, bz, trunk);

        const cy = by + height + 1;
        for (let x = -radius; x <= radius; x++) {
          for (let y = -radius; y <= radius; y++) {
            for (let z = -radius; z <= radius; z++) {
              if (x * x + y * y + z * z > radius * radius + 1) continue;
              if (x === 0 && z === 0 && cy + y < by + height) continue;
              ctx.grid.set(bx + x, cy + y, bz + z, leaves);
            }
          }
        }
      },
    },
  ],
};
