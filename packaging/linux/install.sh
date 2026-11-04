#!/bin/sh
# Kel on this computer, for you alone (no sudo): the program in ~/.local/bin,
# Kel in your apps menu. Run it from the folder you unpacked:
#
#   ./install.sh               install, or update to this version
#   ./install.sh --uninstall   take it away again (your config stays)
set -e

here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
apps="$data/applications"
icons="$data/icons/hicolor/256x256/apps"

if [ "${1-}" = "--uninstall" ]; then
  rm -f "$bin/kel" "$apps/kel.desktop" "$icons/kel.png"
  echo "Kel is gone. Your config and projects stay: ~/.config/kel"
  exit 0
fi

mkdir -p "$bin" "$apps" "$icons"
# A copy, then a rename: a Kel that runs now keeps running, the next is new.
cp "$here/kel" "$bin/kel.new"
chmod +x "$bin/kel.new"
mv -f "$bin/kel.new" "$bin/kel"
cp "$here/kel.png" "$icons/kel.png"
# The menu starts Kel by its full path: ~/.local/bin isn't on every menu's PATH.
sed "s|^Exec=kel|Exec=$bin/kel|" "$here/kel.desktop" > "$apps/kel.desktop"
update-desktop-database "$apps" 2>/dev/null || true

echo "Kel is in $bin/kel and in your apps menu."
case ":$PATH:" in
  *":$bin:"*) echo "Type kel to start it." ;;
  *) echo "$bin isn't on your PATH: add it to type kel in a terminal." ;;
esac
