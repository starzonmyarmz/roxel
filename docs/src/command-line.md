# Command Line

`rox` is a command-line companion to Roxel. It reads Roxel projects and other voxel files without opening the app, so you can inspect, preview, convert or build models from scripts, build tools or a terminal.

## Installing

On macOS or Linux (x86_64), install it with [Homebrew](https://brew.sh):

```sh
brew install starzonmyarmz/tap/rox
```

`brew upgrade` picks up new releases.

Each [GitHub Release](https://github.com/starzonmyarmz/roxel/releases) includes `rox` as its own download, separate from the app:

| Platform | Download |
| -------- | -------- |
| macOS (Apple silicon and Intel) | `rox-macos.zip` |
| Windows | `rox-windows.zip` |
| Linux (x86_64) | `rox-linux.tar.gz` |

Unpack it and move `rox` (`rox.exe` on Windows) into a folder on your `PATH`, such as `~/.local/bin` or `/usr/local/bin`. Run `rox --help` for a summary of every command.

On macOS, `rox` is self-signed and not notarized, like the app, so macOS blocks a copy downloaded with a browser. Clear the quarantine flag once:

```sh
xattr -d com.apple.quarantine /usr/local/bin/rox
```

With [Rust](https://www.rust-lang.org/tools/install) installed, you can build it from source instead:

```sh
cargo install --git https://github.com/starzonmyarmz/roxel roxel-cli
```

## Supported files

Every command reads `.rox`, `.vox`, `.qb` and `.gox`, chosen by the file's extension. `rox convert`, `rox apply` and `rox run` can also write `.glb`, `.obj` and `.svg`.

## `rox info`

```sh
rox info castle.rox
```

Prints the model's voxel count, bounds and colors as JSON:

```json
{
  "voxels": 1520,
  "bounds": { "min": [0, 0, 0], "max": [15, 9, 15], "size": [16, 10, 16] },
  "colors": [
    { "color": "#7ec63e", "count": 256 },
    { "color": "#603e28", "count": 40 }
  ]
}
```

- **`bounds`** — the smallest box holding every voxel. `min` and `max` are inclusive voxel coordinates and `size` is the box's extent on each axis. It's `null` for an empty model.
- **`colors`** — every color used, most-used first. Colors are `#rrggbb`, or `#rrggbbaa` when not fully opaque.

## `rox render`

```sh
rox render castle.rox -o castle.png
```

Renders the model as an isometric PNG, lit the same way as an SVG export. Every voxel draws solid: transparency in a voxel's color is ignored. The image is cropped to the model with a margin one voxel wide.

| Option | Meaning | Default |
| ------ | ------- | ------- |
| `-o`, `--output <path>` | The PNG to write (required) | |
| `--view <0-3>` | Camera angle. Each step turns 90° around the model, so the four views show every side. | `0` |
| `--scale <N>` | Pixels per voxel, from 1 to 256 | `8` |
| `--bg <color>` | Background color, `#rrggbb` or `#rrggbbaa` | transparent |

Images over 16384 pixels on a side, or over 64 megapixels in all, are refused; lower `--scale` for very large models.

```sh
rox render castle.rox -o castle-back.png --view 2 --scale 4 --bg "#202830"
```

Quote a `--bg` color in your shell, since `#` starts a comment in some shells.

## `rox convert`

```sh
rox convert castle.vox castle.glb
```

Saves the model in another format, chosen by the output file's extension.

| Output | Format |
| ------ | ------ |
| `.rox` | Roxel project. It keeps the palette saved in a `.rox` input, or the colors a `.vox` input uses, named after the file. |
| `.vox` | MagicaVoxel. Up to 256 voxels on each axis. |
| `.gox` | Goxel |
| `.glb` | Binary glTF mesh |
| `.obj` | Wavefront OBJ mesh with vertex colors |
| `.svg` | Vector image from the same camera angle as `rox render`'s default view |

A `.rox` written by `rox convert` has no preview image. Open and save it in Roxel to add one.

## `rox apply`

```sh
rox apply --new scene.ron -o scene.rox
rox apply castle.rox tower.ron
python gen.py | rox apply --new - -o scene.rox
```

Builds a model from a list of ops written in [RON](https://github.com/ron-rs/ron). Ops run on an existing model, or on an empty scene with `--new`, and the result is saved to `-o`. Without `-o` the input file is overwritten. Pass `-` instead of an ops file to read the ops from standard input, so a script can generate them.

The output can be any format `rox convert` writes. If an op fails, nothing is written.

```ron
(
  colors: { "soil": "#f6a82e", "grass": "#7ec63e", "leaves": "#7cbe3c", "trunk": "#603e28" },
  ops: [
    Box(min: (0, 0, 0), max: (47, 10, 47), color: "soil"),
    Box(min: (0, 11, 0), max: (47, 11, 47), color: "grass"),
    Cylinder(min: (22, 12, 22), max: (24, 20, 24), axis: Y, color: "trunk"),
    Ellipsoid(min: (18, 18, 18), max: (28, 26, 28), color: "leaves"),
    Line(from: (0, 12, 0), to: (10, 16, 4), color: "#ffffff"),
    Clear(min: (20, 11, 26), max: (30, 11, 34)),
    Recolor(from: "grass", to: "#5a9e30", within: (min: (0, 0, 0), max: (10, 20, 10))),
    Voxels(color: "leaves", cells: [(1, 12, 1), (5, 12, 9)]),
  ],
)
```

Ops run in order, and later ones overwrite earlier ones. Points are `(x, y, z)` voxel coordinates, with `y` pointing up. Anything placed below `y = 0` is dropped, the same as in the app.

| Op | Effect |
| -- | ------ |
| `Box(min, max, color)` | Fills every voxel from `min` to `max` |
| `Cylinder(min, max, axis, color)` | Fills a cylinder that fits the box from `min` to `max`, running along `axis`: `X`, `Y` or `Z`. `axis` defaults to `Y`. |
| `Ellipsoid(min, max, color)` | Fills an ellipsoid, or a sphere when every side is the same length, that fits the box from `min` to `max` |
| `Line(from, to, color)` | Draws a line of voxels from `from` to `to`, both included |
| `Clear(min, max)` | Removes every voxel from `min` to `max` |
| `Recolor(from, to, within)` | Changes every voxel of color `from` to color `to`. `within` is optional and limits it to a box, written `(min: …, max: …)`. |
| `Voxels(color, cells)` | Places a voxel at each point in `cells` |

Boxes include both corners, so `Box(min: (0, 0, 0), max: (1, 1, 1), …)` fills 8 voxels. A shape can cover at most 16,777,216 voxels, the size of a 256 × 256 × 256 box.

A color is a name from `colors` or an inline `#rrggbb` or `#rrggbbaa`. `colors` is optional. Its named colors are saved as the `.rox` file's palette, in the order listed, after any colors already in the input's palette.

## `rox run`

> **Experimental.** Plugins and the plugin API may still change.

```sh
rox run tree.js -o tree.rox
rox run gradient.js castle.rox --color "#ffcc00"
rox run trees.js forest.vox -o forest.glb --command pine
```

Runs a command from a [plugin](plugin-api.md), a JavaScript file, on an existing model or on an empty scene, and saves the result to `-o`. Without `-o` the input file is overwritten. The output can be any format `rox convert` writes. If the plugin fails or hits a limit, nothing is written and `rox` prints the file, the line and the error.

| Option | Meaning | Default |
| ------ | ------- | ------- |
| `-o`, `--output <path>` | The file to write. Required without an input file. | the input file |
| `--command <id>` | The command to run. Required when the plugin has more than one. | the plugin's only command |
| `--color <color>` | The current color the command sees as `ctx.color`, `#rrggbb` or `#rrggbbaa` | `#ffffff` |

The command sees the input's palette as `ctx.palette`: a `.rox` file's saved palette, or the colors a `.vox` file uses. Nothing is selected, so `ctx.selection` is `null`. The run is limited the same way as in the app; see [Limits and errors](plugin-api.md#limits-and-errors).

Saving works as it does for `rox convert`: only a `.rox` output keeps the palette, and it has no preview image, so rewriting a `.rox` drops its preview until you open and save it in Roxel.

The [example plugins](plugin-api.md#examples) are a good place to start.

## Exit status

`rox` exits with status `0` on success. On failure it prints a message starting with `rox:` to standard error and exits with status `1`.
