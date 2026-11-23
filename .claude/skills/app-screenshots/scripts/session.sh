#!/bin/zsh
# Back up Roxel's preferences before a screenshot session and put them back
# after, since the session changes theme, origin axes and last_dir.
#   session.sh begin <backup-dir>
#   session.sh end <backup-dir>      quits Roxel first (it writes prefs on change)
prefs="$HOME/Library/Application Support/roxel/preferences.ron"
dir=$2
case $1 in
  begin)
    mkdir -p $dir
    [[ -f $dir/preferences.ron ]] || cp "$prefs" $dir/preferences.ron
    print "Backed up preferences to $dir/preferences.ron" ;;
  end)
    pkill -x Roxel && sleep 1
    cp $dir/preferences.ron "$prefs" && rm $dir/preferences.ron
    print "Restored preferences" ;;
  *) print -u2 "usage: session.sh begin|end <backup-dir>"; exit 2 ;;
esac
