#!/bin/zsh
# Shoot several angles of one scene, each in light and dark.
#   model.sh <scene.rox> <out-dir> <name>:<yaw-px>:<pitch-px>[:<frame args>[:<fx,fy,zooms>]] ...
# The optional 5th field makes it a detail shot (see angle.sh DETAIL).
# yaw-px is a right-drag from the isometric preset: -320 px turns one
# `rox render --view` step (view k = -320*k). pitch-px > 0 looks down more.
# Runs the 10 s takeover countdown (and launches the scene) first.
set -e
here=${0:A:h}
scene=$1; out=$2; shift 2
mkdir -p $out
$here/takeover.sh --launch $scene
bin=${ROXCTL:-$here/../.bin/roxctl}
if grep -q "show_origin_axes: true" "$HOME/Library/Application Support/roxel/preferences.ron"; then
  $bin key 40 cmd; sleep 0.6; $bin type "Hide origin axes"; sleep 0.4; $bin key 36; sleep 1
fi
stem=${scene:t:r}
for spec in "$@"; do
  parts=("${(@s.:.)spec}")
  DETAIL=${parts[5]} $here/angle.sh $out/$stem-${parts[1]} ${parts[2]} ${parts[3]} ${=parts[4]}
done
