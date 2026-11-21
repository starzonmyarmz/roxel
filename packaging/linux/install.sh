#!/bin/sh
# Installs Roxel for the current user. Run from the unpacked Roxel folder.
# Pass --uninstall to remove it again.
set -eu

here="$(cd "$(dirname "$0")" && pwd)"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
bin="$HOME/.local/bin"

refresh() {
  command -v update-mime-database >/dev/null 2>&1 && update-mime-database "$data/mime" || true
  command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" || true
}

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$bin/Roxel" "$data/applications/roxel.desktop" \
    "$data/icons/hicolor/256x256/apps/roxel.png" "$data/mime/packages/roxel.xml"
  refresh
  echo "Roxel removed."
  exit 0
fi

install -Dm755 "$here/Roxel" "$bin/Roxel"
install -Dm644 "$here/roxel.png" "$data/icons/hicolor/256x256/apps/roxel.png"
install -Dm644 "$here/roxel.xml" "$data/mime/packages/roxel.xml"
install -d "$data/applications"
sed "s|^Exec=Roxel|Exec=$bin/Roxel|" "$here/roxel.desktop" > "$data/applications/roxel.desktop"
chmod 644 "$data/applications/roxel.desktop"
refresh

echo "Roxel installed to $bin/Roxel. Find it in your app launcher."
