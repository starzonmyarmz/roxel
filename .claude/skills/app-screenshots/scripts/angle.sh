#!/bin/zsh
# Shoot one camera angle of the open scene in light and dark themes.
#   angle.sh <out-base> <yaw-px> <pitch-px> [frame.py args...]
# Starts from the isometric preset (Cmd+5), orbits by a right-drag of
# yaw-px/pitch-px, composes (frame.py), then captures <out-base>-light.png
# and <out-base>-dark.png. Leaves the theme on Light.
#
# DETAIL="fx,fy,steps" makes a detail shot: zoom in `steps` wheel steps
# (19 ~ 2x) on the point at fraction (fx, fy) of the composed model's
# bounding box and center it. (Z+click zoom was erratic under synthetic
# input: 6x one run, ~0.6x the next. Don't use it.)
# Run takeover.sh first; roxctl refuses input unless Roxel is frontmost.
set -e
here=${0:A:h}
bin=${ROXCTL:-$here/../.bin/roxctl}
base=$1; yaw=$2; pitch=$3; shift 3
read id wx wy ww wh <<< "$($bin win)"
cx=$((wx + 700)); cy=$((wy + 416))

theme() {
  $bin key 40 cmd; sleep 0.6          # Cmd+K
  $bin type "Theme: $1"; sleep 0.4
  $bin key 36; sleep 1.2              # Return
}

$bin key 46                           # M: Select, so a stray drag can't edit
$bin key 23 cmd; sleep 1              # Cmd+5 isometric
$bin key 29 cmd; sleep 1.2            # Cmd+0 frame
$bin scroll $cx $cy -20 10; sleep 1
if (( yaw != 0 || pitch != 0 )); then $bin orbit $cx $cy $yaw $pitch; sleep 1.5; fi
ROXCTL=$bin $here/compose.sh $base.probe.png "$@"

if [[ -n $DETAIL ]]; then
  # Wheel zoom pivots on the camera focus, whose screen position pan
  # doesn't change. Find it from two bboxes a few steps apart, pan the
  # focal point onto it, zoom in, then pan the focal point to center.
  parts=(${(s:,:)DETAIL})
  read x0 y0 w0 h0 <<< "$(python3 $here/frame.py $base.probe.png --print-bbox)"
  $bin scroll $cx $cy 20 4; sleep 2
  $bin move $((wx + 120)) $((wy + wh - 60)); sleep 0.8
  screencapture -l$id -o -x $base.probe.png
  read x1 y1 w1 h1 <<< "$(python3 $here/frame.py $base.probe.png --print-bbox)"
  read pvx pvy fx fy <<< "$(python3 -c "
s = $w1 / $w0
print(round(($x1 - s * $x0) / (1 - s)), round(($y1 - s * $y0) / (1 - s)),
      round($x1 + $w1 * ${parts[1]}), round($y1 + $h1 * ${parts[2]}))")"
  $bin pan $cx $cy $((pvx - fx)) $((pvy - fy)); sleep 2
  $bin scroll $cx $cy 20 ${parts[3]}; sleep 2.5
  $bin pan $cx $cy $((732 - pvx)) $((416 - pvy)); sleep 2
fi
rm -f $base.probe.png

$bin key 53; $bin key 11; sleep 0.5   # Esc, B: Brush (default rail state)
$bin move $((wx + 120)) $((wy + wh - 60)); sleep 1.5
screencapture -l$id -o -x $base-light.png
theme Dark
$bin move $((wx + 120)) $((wy + wh - 60)); sleep 1.2
screencapture -l$id -o -x $base-dark.png
theme Light
