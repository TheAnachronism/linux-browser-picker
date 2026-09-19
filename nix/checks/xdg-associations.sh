#!/usr/bin/env bash
set -eu

export HOME="$TMPDIR/home"
export LANG=C.UTF-8
export LC_ALL=C.UTF-8
export XDG_DATA_HOME="$HOME/.local/share"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_DIRS="$BROWSER_PICKER/share"
export XDG_CONFIG_DIRS="$TMPDIR/xdg-config-dirs"
export XDG_RUNTIME_DIR="$TMPDIR/runtime"
mkdir -p "$HOME" "$XDG_DATA_HOME/applications" "$XDG_CONFIG_HOME" "$XDG_CONFIG_DIRS" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
cp "$BROWSER_PICKER/share/applications/$DESKTOP_ID" "$XDG_DATA_HOME/applications/$DESKTOP_ID"

test -x "$BROWSER_PICKER/bin/browser-picker"
test -f "$BROWSER_PICKER/share/applications/$DESKTOP_ID"
test -f "$BROWSER_PICKER/share/icons/hicolor/scalable/apps/io.github.TheAnachronism.BrowserPicker.svg"
test -f "$BROWSER_PICKER/share/metainfo/io.github.TheAnachronism.BrowserPicker.metainfo.xml"
test -d "$BROWSER_PICKER/share/locale/en/LC_MESSAGES"

python3 "$CHECK_DESKTOP_ENTRY" "$BROWSER_PICKER/share/applications/$DESKTOP_ID"

cat > "$XDG_DATA_HOME/applications/other-browser.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Other Browser
Exec=true %u
MimeType=x-scheme-handler/https;text/html;application/xhtml+xml;
EOF
cat > "$XDG_CONFIG_DIRS/mimeapps.list" <<EOF
[Default Applications]
x-scheme-handler/http=other-browser.desktop
x-scheme-handler/https=$DESKTOP_ID
text/html=$DESKTOP_ID
application/xhtml+xml=$DESKTOP_ID
EOF
cat > "$XDG_CONFIG_HOME/mimeapps.list" <<EOF
[Default Applications]
x-scheme-handler/http=$DESKTOP_ID
x-scheme-handler/https=other-browser.desktop
text/html=other-browser.desktop

[Removed Associations]
application/xhtml+xml=$DESKTOP_ID;other-browser.desktop;
EOF
update-desktop-database "$XDG_DATA_HOME/applications"

http="$(gio mime x-scheme-handler/http)"
https="$(gio mime x-scheme-handler/https)"
html="$(gio mime text/html)"
xhtml="$(gio mime application/xhtml+xml || true)"
echo "$http"
echo "$https"
echo "$html"
echo "$xhtml"
echo "$http" | grep -F "$DESKTOP_ID"
echo "$https" | grep -F "other-browser.desktop"
echo "$html" | grep -F "other-browser.desktop"
# A silent user file must still see the system XDG mimeapps location.
mv "$XDG_CONFIG_HOME/mimeapps.list" "$XDG_CONFIG_HOME/mimeapps.list.user"
https_system="$(gio mime x-scheme-handler/https)"
echo "$https_system"
echo "$https_system" | grep -F "$DESKTOP_ID"
mv "$XDG_CONFIG_HOME/mimeapps.list.user" "$XDG_CONFIG_HOME/mimeapps.list"
touch "$out"
