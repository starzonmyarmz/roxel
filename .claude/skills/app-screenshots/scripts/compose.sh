#!/bin/zsh
# Zoom, then pan, the Roxel camera until the model sits on the composition
# target (frame.py), waiting for the camera's easing to settle each time.
# usage: compose.sh <probe.png> [frame.py args...]
set -e
here=${0:A:h}
bin=${ROXCTL:-$here/../.bin/roxctl}
probe=$1; shift

read id wx wy ww wh <<< "$($bin win)"
cx=$((wx + 700)); cy=$((wy + wh / 2))

# capture until two probes in a row measure the same (camera has settled)
settle() {
  local prev="" cur
  for _ in 1 2 3 4 5 6; do
    $bin move $((wx + 120)) $((wy + wh - 60)); sleep 0.8
    screencapture -l$id -o -x $probe
    cur=$(python3 $here/frame.py $probe --win-w $ww --win-h $wh "$@")
    [[ $cur == $prev ]] && break
    prev=$cur
  done
  print -r -- $cur
}

for pass in 1 2 3 4; do
  read dx dy steps _ <<< "$(settle "$@")"
  if (( steps != 0 )); then
    if (( steps > 0 )); then $bin scroll $cx $cy 20 $steps; else $bin scroll $cx $cy -20 $((-steps)); fi
    read dx dy steps _ <<< "$(settle "$@")"
  fi
  (( dx * dx + dy * dy < 25 && steps == 0 )) && break
  (( dx * dx + dy * dy >= 25 )) && $bin pan $cx $cy $dx $dy
done
settle "$@" > /dev/null
