---
name: app-screenshots
description: Take screenshots of the Roxel app with a model loaded (README/docs hero images, itch.io page, marketing) and export branded Roxel Shot PNGs. Use when asked to take, retake or refresh screenshots of the app or to make Roxel Shot images.
---

# Roxel app screenshots

Roxel has a hidden scripted capture mode: with `ROXEL_CAPTURE` set, it
loads a scene, frames it, writes one PNG and quits. Nothing drives the
mouse or keyboard. A window pops up on top for a few seconds without
taking focus, and a shot takes about a second. Capture runs on default
preferences plus the flags and never writes the user's preferences,
recents or palettes. Scripts live in `scripts/` next to this file.

## Setup

```sh
S=.claude/skills/app-screenshots/scripts
cargo build --release --workspace
```

Sample scenes: `samples/*.vox` (simple: island, bunny; huge: castle) and
the generators in `samples/generators/` (diorama, lighthouse, mushroom):

```sh
python3 samples/generators/lighthouse.py | target/release/rox apply --new - -o $SCRATCH/lighthouse.rox
target/release/rox convert samples/bunny.vox $SCRATCH/bunny.rox
```

## Design brief

- **3 angles per model**, each in **light and dark** with identical framing,
  so the user can choose. Make them vary: a hero 3/4 view, a low dramatic
  angle, a higher overview or the opposite side. Not three nudges of the
  same view.
- **Show the front.** Find each model's front before choosing angles:
  `rox render <scene> -o v.png --view 0..3` renders the four sides (view 0
  is the editor's isometric angle, yaw 45). Faces, doors and paths should
  face the camera. A bunny seen from behind is a wasted shot.
- **Optically center.** Framing blends the bounding-box center with the
  model's visual-mass centroid (`--mass`, default 0.15), centers it
  between the inspector and the tool rail and fills 72% of the height
  (`--fill`). Look at every result; if one still feels lopsided, reshoot
  with `--nudge x,y` (points) or another `--mass`.
- **Detail shots** for complex models: zoom in on a focal area that shows
  craft (a lighthouse lamp, a door, a waterfall, castle towers). Skip them
  on simple models like the island or bunny; zooming into big plain voxels
  shows only flat slabs.
- Default to a **clean idle canvas**: Brush selected, no selection, origin
  axes hidden. Capture mode gives you exactly that.
- Review the contact sheets before handing over; reshoot what doesn't look
  good.

## Shooting

```sh
$S/capture.sh $SCRATCH/lighthouse.rox $SCRATCH/out a:45:35 b:118:22 c:315:53 \
  "d:45:35:--focus 0.5,0.87,0.5 --zoom 3"
```

Each `name:yaw:pitch[:more flags]` writes `<scene>-<name>-light.png` and
`-dark.png` (2560×1664, a 1280×832 window at 2×), then
`<scene>-sheet.png`. Read the sheet.

One capture by hand:

```sh
ROXEL_CAPTURE=1 target/release/Roxel scene.rox --capture out.png --yaw 45 --pitch 30 --theme dark
```

| Flag | Meaning |
| ---- | ------- |
| `--yaw <deg>` | Orbit angle; 45 is the isometric preset, +90 turns a quarter to the right |
| `--pitch <deg>` | Elevation; 35 is isometric, higher looks down more |
| `--theme light\|dark` | Window captures only, default light |
| `--size WxH` | Window points, default `1280x832` |
| `--fill`, `--mass`, `--nudge x,y` | Framing, see the brief |
| `--focus fx,fy,fz` | Detail shot on a fraction of the bounding box (y up from the floor) |
| `--focus-voxel x,y,z` | Detail shot on a voxel |
| `--zoom <n>` | Detail zoom, default 2 (radius ÷ n) |

Pitch near 80 or more, or straight at a face with no yaw offset, flattens
the model into an elevation. Keep some yaw off-axis.

Angles that worked for the samples:

| Model | Angles |
| ----- | ------ |
| bunny | `a:166:33 b:205:24 c:158:18` |
| island | `a:225:35 b:197:22 c:264:50` |
| mushroom-house | `a:34:31 b:101:22 c:349:50 "d:34:31:--focus 0.5,0.45,0.5 --zoom 2"` |
| lighthouse | `a:45:35 b:118:22 c:315:53 "d:45:35:--focus 0.5,0.87,0.5 --zoom 3"` |
| diorama | `a:45:35 b:129:24 c:11:55 "d:45:35:--focus 1,0.45,0.6 --zoom 2"` |
| castle | `a:45:35 b:101:20 c:287:48 "d:45:35:--focus 0.5,0.75,0.5 --zoom 2"` |

The capture is the window's content only: no macOS title bar buttons and
square corners.

## Roxel Shot exports

```sh
ROXEL_CAPTURE=1 target/release/Roxel scene.rox --export-shot bunny.png \
  --yaw 225 --aspect 3:4 --style radial --strength 0.6 --grain 4 --vignette 0.3
```

`--yaw` turns the shot camera (and its lights) around the model; 45 is the
panel's default corner. The other flags set the panel's knobs: `--aspect
4:3|1:1|16:9|3:4`, `--style solid|vertical|radial`, `--strength 0–1`,
`--flip`, `--grain 0–24`, `--vignette 0–1`, `--saturation 0–1.5`,
`--lift 0–8`, `--bg <hex>` (default: auto from the model).

Curate the look per model instead of taking defaults:

| Model | Look |
| ----- | ---- |
| tall subjects | 3:4, radial |
| compact dioramas | 1:1, radial |
| wide scenes | 16:9, vertical |

Grain at 2–5 and vignette at 0.25–0.4 add finish without getting muddy.

## Repo images

| File | Used by |
| ---- | ------- |
| `.github/assets/screenshot.png` | README hero |
| `docs/src/images/screenshot.png` / `screenshot-dark.png` | docs introduction (CSS swaps in dark) |
| `docs/src/images/gallery/*` | docs thumbnails that open in a lightbox: intro gallery, Roxel Shots, panel crops |
| `.github/assets/roxel_cover.png` | itch.io cover, 630×500 |
| `.github/assets/roxel_ss_light.jpg` / `roxel_ss_dark.jpg` | spare window shots |

Hero shots get a drop shadow on a transparent background, so they sit on
any page background:

```sh
magick in.png -resize 1600x \( +clone -background black -shadow 35x24+0+16 \) \
  +swap -background none -layers merge +repage out.png
```

Rebuild the docs gallery with
`$S/docs-gallery.sh <picks dir> <roxel-shots dir>`. It expects the picked
captures by name (`lighthouse-d-light.png`, …) and crops the panels from
`lighthouse-a-{light,dark}.png`. Change the crop offsets if the UI layout
moves.

Write the spare JPGs with `magick in.png -resize 1600x -quality 82 -strip out.jpg`.
itch.io images are uploaded by hand; put a copy of everything in a
Desktop folder for the user.
