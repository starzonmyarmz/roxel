# Plugin API

> **Experimental.** The plugin API may still change before it settles. When it does, the change is listed in the changelog.

A plugin is a JavaScript file that adds commands to Roxel: generators that build something, filters that change what's there, and recolors. This page is the reference for writing one; to install and run plugins, see [Plugins](plugins.md). [`roxel.d.ts`](roxel.d.ts) describes the same API as TypeScript types, and your editor can use it for autocomplete.

## The manifest

A plugin is an ES module. Its default export, the manifest, names the plugin and lists its commands:

```js
export default {
  apiVersion: 1,
  name: "Pillars",
  commands: [
    {
      id: "pillar",
      label: "Pillar",
      run(ctx) {
        for (let y = 0; y < 8; y++) ctx.grid.set(0, y, 0, ctx.color);
      },
    },
  ],
};
```

- **`apiVersion`** — the API version the plugin is written for. It's `1`.
- **`name`** — the plugin's name, shown in the [Plugins menu](plugins.md#running-a-command).
- **`commands`** — what the plugin can do. Each command has an `id` that's unique within the plugin, a `label` shown in menus and the command palette, and a `run` function.

Roxel checks the manifest when it loads the plugin, and refuses one with a missing field, a duplicate `id` or a `run` that isn't a function.

For autocomplete, put `roxel.d.ts` next to your plugin and add a JSDoc annotation:

```js
/** @type {import("./roxel").Plugin} */
export default { apiVersion: 1, name: "Pillars", commands: [] };
```

## `apiVersion`

Every plugin declares `apiVersion: 1`. A plugin written for a version this copy of Roxel doesn't support is refused when it loads, so it fails with a clear message rather than partway through a run. A plugin that works with `apiVersion: 1` keeps working in later versions of Roxel that support `apiVersion: 1`.

## `ctx`

`run` gets one argument, `ctx`, which holds everything a command can see:

| Field | What it is |
| ----- | ---------- |
| `ctx.grid` | The model: read cells with `get`, `bounds` and `forEach`, change them with `set` and `clear`. |
| `ctx.selection` | The selection, or `null` when nothing is selected. |
| `ctx.color` | The current color. |
| `ctx.palette` | The active palette's colors, in order. |
| `ctx.params` | Parameter values for this run, by name. |

```js
export default {
  apiVersion: 1,
  name: "Palette Stripe",
  commands: [
    {
      id: "stripe",
      label: "Palette Stripe",
      run(ctx) {
        ctx.palette.forEach((color, x) => ctx.grid.set(x, 0, 0, color));
      },
    },
  ],
};
```

## Coordinates and the floor

Cells are addressed by integer `x`, `y` and `z`. Y is up. Nothing exists below the floor, so `y` is never negative: a write below `y = 0` is ignored, and reading there returns `null`. X and Z can be negative.

Coordinates must be whole numbers. Passing `1.5` throws a `TypeError`, so round first:

```js
const y = Math.round(Math.sin(x / 4) * 3) + 3;
ctx.grid.set(x, y, 0, ctx.color);
```

## Colors

A color is an array `[r, g, b, a]` of integers from 0 to 255. Roxel always returns all four channels. When you write a color, you can leave out alpha and it's 255:

```js
ctx.grid.set(0, 0, 0, [255, 128, 0]);     // orange
const [r, g, b] = ctx.grid.get(0, 0, 0);  // 255, 128, 0
```

Anything else, such as `"#ff8000"` or a channel above 255, throws a `TypeError`.

## Reading the grid

- **`ctx.grid.get(x, y, z)`** — the color at a cell, or `null` when it's empty.
- **`ctx.grid.bounds()`** — `{ min: [x, y, z], max: [x, y, z] }`, the smallest box holding every voxel. Both corners are inside it. It's `null` for an empty model.
- **`ctx.grid.forEach((x, y, z, color) => …)`** — calls the function for every filled cell, in no particular order.

Reads always see the model as it was when the run started, even after `set` or `clear`. That lets a filter read the old state while it writes the new one:

```js
export default {
  apiVersion: 1,
  name: "Grow",
  commands: [
    {
      id: "grow-up",
      label: "Grow Up",
      run(ctx) {
        ctx.grid.forEach((x, y, z, color) => {
          if (ctx.grid.get(x, y + 1, z) === null) ctx.grid.set(x, y + 1, z, color);
        });
      },
    },
  ],
};
```

## Changing the grid

- **`ctx.grid.set(x, y, z, color)`** — fills a cell.
- **`ctx.grid.clear(x, y, z)`** — empties a cell.

Changes apply when `run` returns, all at once, as **one undo step**: a single Undo reverts the whole run. If a cell is written more than once, the last write wins. A run that changes nothing adds nothing to the undo history.

If `run` throws, nothing changes.

`run` must be synchronous. An `async` command is refused.

## The selection

When something is selected, `ctx.selection` describes it:

- **`ctx.selection.bounds`** — the selection box, `{ min, max }`, both corners inside.
- **`ctx.selection.has(x, y, z)`** — whether a cell is selected.
- **`ctx.selection.forEach((x, y, z) => …)`** — calls the function for every selected cell, filled or empty, in no particular order.

When you pick individual voxels, only those are selected. Otherwise the whole box is. A gradient over the selection:

```js
export default {
  apiVersion: 1,
  name: "Gradient",
  commands: [
    {
      id: "vertical",
      label: "Vertical Gradient",
      run(ctx) {
        if (!ctx.selection) throw new Error("Select something first");
        const { min, max } = ctx.selection.bounds;
        const top = ctx.color;
        const bottom = [0, 0, 0, 255];
        ctx.selection.forEach((x, y, z) => {
          if (ctx.grid.get(x, y, z) === null) return;
          const t = max[1] === min[1] ? 1 : (y - min[1]) / (max[1] - min[1]);
          const mix = (i) => Math.round(bottom[i] + (top[i] - bottom[i]) * t);
          ctx.grid.set(x, y, z, [mix(0), mix(1), mix(2)]);
        });
      },
    },
  ],
};
```

## Params

`ctx.params` holds this run's parameter values by name: numbers, booleans, strings, and colors as `[r, g, b, a]`. Commands can't declare parameters yet, so for now it's an empty object.

## Limits and errors

Every run has limits, so a plugin can't freeze Roxel:

| Limit | |
| ----- | - |
| Time | 5 seconds |
| Memory | 256 MB for the script |
| Changed voxels | 4,194,304 per run |

A run that hits a limit stops, and nothing changes. Catching the exception doesn't get around a limit.

When a plugin fails to load or a run throws, Roxel shows the file name, the line and the error:

```text
trees.js:12: TypeError: grid.set: color must be [r, g, b] or [r, g, b, a] of integers 0-255
```

## Examples

Download these to try them, or use them as a starting point:

- [`tree.js`](plugins/examples/tree.js) — a generator that plants an oak under the selection, or at the origin.
- [`gradient.js`](plugins/examples/gradient.js) — a recolor that shades the selection, or the whole model, from black at the bottom to the current color at the top.

## What plugins can't do

A plugin only sees what's in `ctx`. It can't read or write files, use the network, start programs, change preferences or `import` other modules. It can't change the model except through `ctx.grid`, so every change it makes can be undone.
