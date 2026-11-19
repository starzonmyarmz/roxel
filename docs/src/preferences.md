# Preferences & Themes

Roxel's UI is canvas-first: a single left inspector, a floating tool island, and — on Windows / Linux — a top-center menu pill with the same File, Edit, View and Help menus as the macOS menu bar. Everything is hideable so the model takes the whole screen.

## Themes

Choose **Light**, **Dark**, or **System** (follows your OS setting).

## UI scale

**Preferences → Appearance → UI scale** sizes the interface from 80% to 150%: panels, menus, dialogs and text. The 3D canvas isn't scaled.

## Reduce motion

**Preferences → Appearance → Motion → Reduce motion** turns off interface animations: the camera jumps straight to a view preset, Frame View or zoom step instead of easing there, and palette swatches snap into place when you reorder them. The flyby camera still moves when you start it.

## Canvas appearance

Independent of the UI theme, you can set:

- **Canvas color** — the viewport background
- **Floor color** — the ground plane at `y = 0`
- **Floor grid lines** — optional Minecraft-style grid on the floor

A permanent RGB **axis triad** marks the world origin.

## Updates

Roxel checks for a new version when it starts, at most once every 24 hours. When one is out, a notice appears and **Check for Updates…** changes to **Download Roxel vX.Y.Z…**, which opens the release page in your browser. Roxel never downloads or installs anything itself.

The check is one HTTPS request to GitHub's public API (`api.github.com/repos/starzonmyarmz/roxel/releases/latest`). It sends:

- your IP address, as every web request does
- a `User-Agent` header with the Roxel version, like `roxel/X.Y.Z`

It sends no account, device ID, usage data or anything about your projects. GitHub's [privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement) covers what GitHub logs.

To turn it off, uncheck **Preferences → Updates → Check for updates on launch**. You can still check by hand from **Roxel → Check for Updates…** on macOS, **Help → Check for Updates…** on Windows/Linux, or the command palette.

## Focus mode

Press `` ` `` (backtick) to toggle focus mode, which hides the inspector and all floating chrome for a distraction-free canvas. Press again to bring the UI back. It is also under **View → Focus Mode** and in the command palette (`Cmd/Ctrl + K`, "focus mode").
