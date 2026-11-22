# Command Line

`rox` is a command-line companion to Roxel. It reads Roxel projects and other voxel files without opening the app, so you can inspect or preview models from scripts, build tools or a terminal.

## Installing

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

Every command reads `.rox`, `.vox`, `.qb` and `.gox`, chosen by the file's extension. `rox convert` can also write `.glb`, `.obj` and `.svg`.

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

## Exit status

`rox` exits with status `0` on success. On failure it prints a message starting with `rox:` to standard error and exits with status `1`.
