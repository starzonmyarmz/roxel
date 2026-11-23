#!/bin/zsh
# Export a curated Roxel Shot of the open scene.
#   shot.sh <out-dir> <name> [knob=value ...]
# knobs: aspect=4:3|1:1|16:9|3:4  style=solid|vertical|radial
#        dir=ltd|dtl  strength|grain|saturation|vignette|lift=<0..1>
# Saves <out-dir>/<name>.png and the panel preview as <name>.panel.png.
# Control positions are for a 1280x832 window; run takeover.sh first.
set -e
here=${0:A:h}
bin=${ROXCTL:-$here/../.bin/roxctl}
out=$1; name=$2; shift 2
read id wx wy ww wh <<< "$($bin win)"
c() { $bin click $((wx + $1)) $((wy + $2)); sleep 0.6; }
slider() { c $(( 780 + ${2} * 160 )) $1; }

$bin key 53; sleep 0.5                        # Esc: close any open modal
$bin key 40 cmd; sleep 0.6; $bin type "Export Shot"; sleep 0.4; $bin key 36; sleep 3
c 870 699                                     # Reset knobs to defaults
for kv in "$@"; do
  k=${kv%%=*}; v=${kv#*=}
  case $k in
    aspect) case $v in 4:3) c 801 214;; 1:1) c 850 214;; 16:9) c 901 214;; 3:4) c 956 214;; esac ;;
    style) case $v in solid) c 806 316;; vertical) c 874 316;; radial) c 947 316;; esac ;;
    dir) case $v in ltd) c 828 383;; dtl) c 931 383;; esac ;;
    strength) slider 349 $v ;;
    grain) slider 451 $v ;;
    saturation) slider 552 $v ;;
    vignette) slider 586 $v ;;
    lift) slider 620 $v ;;
  esac
done
$bin move $((wx + 120)) $((wy + 800)); sleep 3   # let the preview re-render
screencapture -l$id -o -x $out/$name.panel.png
c 955 699                                     # Export…
for _ in {1..20}; do $bin dialog && break; sleep 0.5; done
$bin dialog || { print -u2 "save dialog never opened"; exit 1; }
sleep 0.8
# The save panel ignores roxctl's synthetic key events; System Events
# keystrokes reach it. Re-check focus first, like roxctl does.
[[ $(osascript -e 'tell application "System Events" to get name of first process whose frontmost is true') == Roxel ]] \
  || { print -u2 "Roxel lost focus; not typing"; exit 3; }
osascript \
  -e 'tell application "System Events"' \
  -e 'keystroke "a" using command down' \
  -e "keystroke \"$name.png\"" \
  -e 'keystroke "g" using {command down, shift down}' \
  -e 'delay 1' \
  -e "keystroke \"$out\"" \
  -e 'delay 0.5' \
  -e 'key code 36' \
  -e 'delay 1' \
  -e 'key code 36' \
  -e 'end tell'
for _ in {1..30}; do [[ -f $out/$name.png || -f $out/$name.png.png ]] && break; sleep 0.5; done
sleep 1
[[ -f $out/$name.png.png ]] && mv $out/$name.png.png $out/$name.png
[[ -f $out/$name.png ]] || { print -u2 "export failed: $out/$name.png missing"; exit 1; }
