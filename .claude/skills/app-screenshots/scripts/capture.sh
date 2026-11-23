#!/bin/zsh
# Usage: capture.sh <scene.rox> <out dir> <name>:<yaw>:<pitch>[:<more flags>] ...
# Writes <out>/<scene>-<name>-light.png and -dark.png for each spec, then a
# contact sheet, <out>/<scene>-sheet.png. The optional 4th field holds more
# capture flags, space-separated: "d:45:35:--focus 0.5,0.87,0.5 --zoom 3".
set -euo pipefail
root=${0:A:h:h:h:h:h}
bin=${ROXEL:-$root/target/release/Roxel}
scene=${1:A}
out=${2:A}
shift 2
mkdir -p $out
base=${scene:t:r}
shots=()
for spec in "$@"; do
  IFS=: read -r name yaw pitch extra <<< "$spec"
  for theme in light dark; do
    file=$out/$base-$name-$theme.png
    ROXEL_CAPTURE=1 RUST_LOG=error $bin $scene --capture $file \
      --yaw $yaw --pitch $pitch --theme $theme ${=extra} > /dev/null
    shots+=$file
  done
done
magick montage $shots -tile 2x -geometry 640x+8+8 -background '#888' $out/$base-sheet.png
print $out/$base-sheet.png
