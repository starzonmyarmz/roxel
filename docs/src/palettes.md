# Palettes

A palette is the set of colors you paint with. Roxel ships with classic pixel-art palettes and lets you build your own.

## Built-in palettes

- Sweetie 16
- PICO-8
- DawnBringer 16 / 32
- Endesga 32
- NA16
- Basic

## User palettes

You can manage your own palettes from the inspector's **…** menu or the command palette (`Cmd/Ctrl + K`):

- **Add** a color to the current palette
- Create a **new** palette
- **Duplicate** an existing one as a starting point (for a built-in palette this reads **Save as new palette**)
- **Rename** and **delete** (Roxel asks you to confirm before deleting a palette)
- **Drag to reorder** swatches

User palettes persist between sessions.

## Palettes in projects

Saving a `.rox` project also saves the palette you're using, including any unsaved edits to a built-in. Opening the project switches back to that palette. If none of your palettes has those colors, Roxel adds it as a new user palette.

## Importing & exporting palettes

Roxel reads and writes the **Adobe Swatch Exchange** (`.ase`) format, so you can move palettes between Roxel and other design tools. Importing a MagicaVoxel `.vox` model also adds the colors it uses as a new user palette. See [Import & Export](import-export.md).
