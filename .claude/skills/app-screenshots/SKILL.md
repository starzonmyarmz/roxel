---
name: app-screenshots
description: Take screenshots of the Roxel app with a model loaded (README/docs hero images, itch.io page, marketing) and export branded Roxel Shot PNGs. Use when asked to take, retake or refresh screenshots of the app or to make Roxel Shot images. Drives the user's mouse and keyboard on macOS, so it always warns first.
---

# Roxel app screenshots

Drives a release build of Roxel with synthetic input, composes each shot
like a designer would, and captures the window. macOS only. Scripts live in
`scripts/` next to this file.

## Rules

- **Warn before touching the screen.** Every takeover starts with
  `scripts/takeover.sh` (10 s countdown, a beep each second, a notification),
  which also confirms Roxel is frontmost. Run it again after any pause where
  the user may have used the machine. `roxctl` refuses to send input unless
  Roxel is frontmost, so a click into another app aborts the run instead of
  typing into it.
- **Never edit the scene.** Camera moves run under the Select tool (`M`), so
  a drag that loses its modifier only makes a selection. Shoot copies of the
  scenes in a scratch folder and never save. If a scene changes, quit
  without saving and regenerate it.
- **Leave preferences as found.** `session.sh begin` backs up
  `preferences.ron`; `session.sh end` quits Roxel and restores it. The
  session flips the theme, hides the origin axes and changes `last_dir`.
- Window is **1280×832** points (2560×1664 captures).
- Default to a **clean idle canvas**: Brush selected, no selection, cursor
  parked over the inspector, origin axes hidden. Mid-edit moments, the
  command palette, modals or focus mode only when the user asks.

## Design brief

- **3 angles per model**, each in **light and dark** with identical framing,
  so the user can choose. Make them vary: a hero 3/4 view, a low dramatic
  angle, a higher overview or the opposite side. Not three nudges of the
  same view.
- **Show the front.** Find each model's front before choosing angles:
  `rox render <scene> -o v.png --view 0..3` renders the four sides (view 0
  is the editor's default isometric angle). Faces, doors and paths should
  face the camera. A bunny seen from behind is a wasted shot.
- **Optically center.** `frame.py` centers on a blend of the bounding box
  and the model's visual-mass centroid (`--mass`), so a tall thin part
  (a lighthouse tower) doesn't leave the heavy part looking off-center.
  Look at every result; if it still feels lopsided, reshoot with
  `--nudge-x` / `--nudge-y` (points) or adjust `--mass`. Don't let the
  model run into the tool rail or the window edges.
- **Detail shots** for complex models: zoom in on a focal area that shows
  craft (a lighthouse lamp, a door, a waterfall, castle towers) with a 5th
  spec field `fx,fy,steps` (see Shooting). Skip them on simple models
  like the island or bunny; zooming into big plain voxels shows only flat
  slabs.
- Review contact sheets of everything before handing over; reshoot what
  doesn't look good.

## Setup

```sh
S=.claude/skills/app-screenshots/scripts
mkdir -p .claude/skills/app-screenshots/.bin
swiftc -O -o .claude/skills/app-screenshots/.bin/roxctl $S/roxctl.swift
cargo build --release --workspace
```

Sample scenes: `samples/*.vox` (simple: island, bunny; huge: castle) and
the generators in `samples/generators/` (diorama, lighthouse, mushroom):

```sh
python3 samples/generators/lighthouse.py | target/release/rox apply --new - -o $SCRATCH/lighthouse.rox
target/release/rox convert samples/bunny.vox $SCRATCH/bunny.rox
```

Ghostty (or whatever runs the session) needs **Screen Recording** and
**Accessibility** permission. Screen Recording only applies after the
terminal is fully quit and reopened.

## Shooting

```sh
$S/session.sh begin $SCRATCH/prefs
$S/model.sh $SCRATCH/bunny.rox $SCRATCH/out a:-430:-10 b:-570:-50 c:-480:-90
$S/session.sh end $SCRATCH/prefs
```

`model.sh` runs the countdown, relaunches Roxel on the scene at 1280×832
and hides the origin axes. Then, for each
`name:yaw:pitch[:frame args[:fx,fy,steps]]`, it runs `angle.sh`:

1. Start from the isometric preset (Cmd+5) and right-drag by the yaw and
   pitch pixels.
2. Let `compose.sh` zoom and pan until `frame.py` is satisfied.
3. Capture `<scene>-<name>-light.png` and `-dark.png` with identical
   framing; the theme is switched through the command palette.

With the detail field, `angle.sh` zooms `steps` wheel steps (19 ≈ 2×) on
the point at fraction (fx, fy) of the model's bounding box and centers it.
For example, `d:0:0::0.5,0.13,19` zooms on the lighthouse lamp.

Angles that worked for the samples:

| Model | Angles |
| ----- | ------ |
| bunny | `a:-430:-10 b:-570:-50 c:-400:-80` |
| island | `a:-640:0 b:-540:-60 c:-780:70` |
| mushroom-house | `a:40:-20 b:-200:-60 c:200:70 d:40:-20::0.52,0.55,19` |
| lighthouse | `a:0:0 b:-260:-60 c:320:80 d:0:0::0.5,0.13,19` |
| diorama | `a:0:0 b:-300:-50 c:120:90 d:0:0::0.75,0.76,19` |
| castle | `a:0:0 b:-200:-70 c:420:60 d:0:0::0.54,0.3,19` |

Pitch around −80 or below, looking straight at a face, flattens the model
into an elevation. Keep some yaw off-axis.

The detail zoom (pivot found from two bounding boxes, then wheel zoom) is
approximate and untested on these scenes. Its one trial overshot (about
3× for 22 steps) and landed off the focal point, so check every detail
shot and adjust `fx,fy,steps`.

Camera calibration at 1280×832:

- **Yaw:** −320 px is one quarter turn, so view k ≈ −320·k px.
- **Pitch:** positive pitch px raises the camera (looks down more);
  negative lowers it.
- **Pan:** follows the drag about 1:1. It is Space + left-drag, and the
  drag events must carry motion deltas, which Bevy reads instead of
  positions. `roxctl` sets them.
- **Zoom:** one 20 px wheel step changes the model's size by about 3.7%.
  The wheel zooms about the camera focus.
- **Z+click zoom:** don't use it. Under synthetic input it was erratic,
  6× on one run and about 0.6× on the next.
- **Easing:** the camera eases after every move, so `compose.sh` captures
  only once two probes in a row agree.

## Roxel Shot exports

Roxel Shot (`Export Shot` in the command palette) always renders from one
isometric corner, ignoring the editor camera. To show a model's front,
turn a copy: `rox convert` to `.vox`, `scripts/turn_vox.py in.vox out.vox
<quarter-turns>`, `rox convert` back to `.rox`. One turn equals one
`rox render --view` step.

Curate the look per model instead of taking defaults, then export:

```sh
$S/takeover.sh --launch $SCRATCH/turned/bunny.rox
$S/shot.sh $SCRATCH/shots bunny aspect=3:4 style=radial strength=0.6 grain=0.15 vignette=0.3
```

`shot.sh` resets the panel, clicks the knobs (sliders jump to a clicked
point) and saves a `<name>.panel.png` preview. It then clicks **Export…**
and fills the save panel.

The save panel has two quirks:

- **Slow to open:** it takes a few seconds, and keystrokes sent before it
  appears land on the canvas as tool shortcuts. Cmd+A selects every voxel.
  `roxctl dialog` waits for it.
- **Ignores synthetic keys:** it ignores `roxctl` key events, so
  `shot.sh` types there with System Events after re-checking focus.

The panel can append a second `.png`; `shot.sh` renames it.

Looks that suit the samples:

| Model | Look |
| ----- | ---- |
| tall subjects | 3:4, radial |
| compact dioramas | 1:1, radial |
| wide scenes | 16:9, vertical |

Grain at 0.1–0.2 and vignette at 0.25–0.4 add finish without getting
muddy.

## Repo images

| File | Used by |
| ---- | ------- |
| `.github/assets/screenshot.jpg` | README hero |
| `docs/src/images/screenshot.jpg` / `screenshot-dark.jpg` | docs introduction (CSS swaps in dark) |
| `.github/assets/roxel_cover.png` | itch.io cover, 630×500 |
| `.github/assets/roxel_ss_light.jpg` / `roxel_ss_dark.jpg` | spare window shots |

Write JPGs with `magick in.png -resize 1600x -quality 82 -strip out.jpg`.
itch.io images are uploaded by hand; put a copy of everything in a
Desktop folder for the user.
