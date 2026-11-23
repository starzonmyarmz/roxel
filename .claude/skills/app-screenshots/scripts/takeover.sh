#!/bin/zsh
# Warn before driving the user's screen: 10 s countdown with a beep each
# second, then (optionally) relaunch Roxel on a scene at 1280x832, and
# confirm Roxel has focus before anything else runs.
# usage: takeover.sh [--launch <scene.rox>] [seconds]
app=${ROXEL_APP:-${0:A:h}/../../../../target/release/Roxel}
scene=""
if [[ $1 == --launch ]]; then scene=$2; shift 2; fi
n=${1:-10}
osascript -e "display notification \"Taking control of the screen in $n seconds\" with title \"Roxel screenshots\"" 2>/dev/null
for ((i = n; i > 0; i--)); do
  print "Taking control of the screen in $i…"
  afplay /System/Library/Sounds/Tink.aiff &
  sleep 1
done
if [[ -n $scene ]]; then
  pkill -x Roxel && sleep 1
  ($app $scene >/dev/null 2>&1 &)
  for _ in {1..20}; do pgrep -x Roxel >/dev/null && break; sleep 0.5; done
  sleep 4
  osascript -e 'tell application "System Events" to tell (first process whose name is "Roxel") to set size of window 1 to {1280, 832}'
fi
osascript -e 'tell application "System Events" to set frontmost of (first process whose name is "Roxel") to true' || exit 1
sleep 0.5
front=$(osascript -e 'tell application "System Events" to get name of first process whose frontmost is true')
if [[ $front != Roxel ]]; then
  print -u2 "Roxel is not frontmost ($front has focus); not starting."
  exit 1
fi
afplay /System/Library/Sounds/Glass.aiff
print "Roxel has focus; starting."
