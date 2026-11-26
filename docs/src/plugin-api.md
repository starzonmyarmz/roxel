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
- **`commands`** — what the plugin can do. Each command has an `id` that's unique within the plugin, a `label` shown in menus and the command palette, and a `run` function. It can also list [`params`](#params), values to ask for before it runs.
- **`toolbar`** — optional. Buttons on the tool rail that run commands. See [Toolbar](#toolbar).

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
| `ctx.params` | The values of the command's [params](#params), by `id`. |

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

A command can ask for values before it runs. List them in its `params`, and Roxel shows a form with one field per param when you choose the command. **Run** runs it with those values, and **Cancel** doesn't run it. The form starts with the values you used last time for that command, or the defaults the first time.

```js
export default {
  apiVersion: 1,
  name: "Tower",
  commands: [
    {
      id: "tower",
      label: "Tower",
      params: [
        { id: "height", label: "Height", type: "int", min: 1, max: 32, default: 8 },
        { id: "hollow", label: "Hollow", type: "bool" },
        { id: "wall", label: "Wall", type: "color", default: [180, 170, 150] },
        { id: "shape", label: "Shape", type: "choice", options: ["Square", "Round"] },
      ],
      run(ctx) {
        const { height, hollow, wall, shape } = ctx.params;
        for (let y = 0; y < height; y++) {
          for (let x = -2; x <= 2; x++) {
            for (let z = -2; z <= 2; z++) {
              const round = shape === "Round";
              const d = x * x + z * z;
              if (round && d > 5) continue;
              const edge = round ? d >= 3 : Math.max(Math.abs(x), Math.abs(z)) === 2;
              if (edge || !hollow) ctx.grid.set(x, y, z, wall);
            }
          }
        }
      },
    },
  ],
};
```

Each param has an `id`, unique within the command, a `label` shown next to its field, and a `type`:

| `type` | Field | Value in `ctx.params` | Options |
| ------ | ----- | --------------------- | ------- |
| `"number"` | A number field, or a slider when it has both `min` and `max` | A number | `min`, `max`, `default` (0) |
| `"int"` | The same, in whole numbers | A whole number | `min`, `max`, `default` (0) |
| `"bool"` | A checkbox | `true` or `false` | `default` (`false`) |
| `"color"` | A color swatch | `[r, g, b, a]` | `default` (white) |
| `"choice"` | A dropdown | One of the `options` strings | `options` (required), `default` (the first option) |

`default` is optional. When a number's default is left out and 0 is out of range, it's the nearest of `min` and `max`.

`ctx.params` always has a value for every param, and only for those. Values outside `min` and `max` are clamped, and an `int` is rounded. A command without `params` runs as soon as you choose it, and its `ctx.params` is an empty object.

Roxel checks `params` when it loads the plugin, and refuses one with a missing `id`, `label` or `type`, a duplicate `id`, a `min` above its `max`, or a `default` that's out of range or not one of the `options`.

[`rox run`](command-line.md#rox-run) runs a command with its defaults, or with values you pass with `--param`.

## Toolbar

A plugin can put buttons on the tool rail, under the built-in tools. Each button runs one of the plugin's commands, the same as choosing it from the Plugins menu, so a command with params still opens its form first. List them in `toolbar`:

```js
export default {
  apiVersion: 1,
  name: "Stamps",
  commands: [
    {
      id: "crate",
      label: "Crate",
      run(ctx) {
        for (let x = 0; x < 2; x++)
          for (let y = 0; y < 2; y++)
            for (let z = 0; z < 2; z++) ctx.grid.set(x, y, z, [150, 100, 50]);
      },
    },
  ],
  toolbar: [{ command: "crate", icon: "box", tooltip: "Stamp a crate" }],
};
```

Each button has:

- **`command`** — the `id` of one of the plugin's commands. A command can have one button.
- **`icon`** — the name of a bundled icon, or the path of your own `.svg` file. See [Icons](#icons).
- **`tooltip`** — optional. Shown when you hover over the button. It's the command's `label` when left out.

Roxel checks `toolbar` when it loads the plugin, and refuses one whose `command` isn't one of the plugin's commands, or that has two buttons for the same command.

The rail has room for a few plugin buttons: at most five, and fewer when the window is short. When there are more, the last place holds a **⋯** button that lists the rest by their tooltips. Buttons appear in plugin order (sorted by file name), then in the order of each plugin's `toolbar`.

### Icons

Buttons show an icon only, tinted to match the theme like the built-in tools. The bundled icons are from [Lucide](https://lucide.dev/icons/):

`blend`, `box`, `boxes`, `brick-wall`, `castle`, `cloud`, `cone`, `cylinder`, `dices`, `droplet`, `fence`, `flame`, `flip-horizontal-2`, `flip-vertical-2`, `flower`, `gem`, `grid-3x3`, `hammer`, `heart`, `house`, `layers`, `leaf`, `moon`, `mountain`, `mountain-snow`, `paintbrush`, `palette`, `puzzle`, `pyramid`, `rotate-cw`, `scaling`, `shovel`, `shuffle`, `snowflake`, `sparkles`, `sprout`, `star`, `sun`, `tent`, `torus`, `tree-deciduous`, `tree-pine`, `trees`, `wand-sparkles`, `waves`, `zap`.

To use your own, ship an `.svg` file with the plugin and give its path relative to the plugin's module, such as `"icon.svg"` or `"icons/crate.svg"`. The path can't leave the plugin's folder. Make it a 24 × 24 icon drawn in white or `currentColor`, like [Lucide's](https://lucide.dev/icons/): Roxel tints it, so other colors come out darker, not in color. A plugin with its own icons is easiest to share as a [folder plugin](plugins.md#the-plugins-folder), with the icons next to its `index.js`.

Roxel reads the file when it loads the plugin, so reload after you change it. If it can't show an icon, because the name isn't bundled, or the file is missing, isn't a valid SVG or is over 64 KB, the button shows the `puzzle` icon and Roxel says why.

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

- [`tree.js`](plugins/examples/tree.js) — a generator that plants an oak under the selection, or at the origin, with params for its height, crown size and colors.
- [`gradient.js`](plugins/examples/gradient.js) — a recolor that shades the selection, or the whole model, from black at the bottom to the current color at the top.

## What plugins can't do

A plugin only sees what's in `ctx`. Roxel reads its toolbar icon files to show them, but the plugin itself can't read or write files, use the network, start programs, change preferences or `import` other modules. It can't change the model except through `ctx.grid`, so every change it makes can be undone.
