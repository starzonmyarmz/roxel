// Plants an oak: a trunk with a round crown of leaves. It stands on the
// floor under the middle of the selection, or at the origin.
export default {
  apiVersion: 1,
  name: "Tree",
  commands: [
    {
      id: "oak",
      label: "Oak Tree",
      run(ctx) {
        const trunk = [96, 62, 40];
        const leaves = [124, 190, 60];
        const height = 6;
        const radius = 3;

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
