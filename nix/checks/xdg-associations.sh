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

gio_default_id() {
  local output
  output="$(gio mime "$1" 2>/dev/null || true)"
  printf '%s\n' "$output" | sed -n 's/^Default application for.*: //p' | head -n1
}

handler_label() {
  local desktop_id="$1"
  if [ -z "$desktop_id" ]; then
    printf '%s\n' "not set"
  elif [ "$desktop_id" = "$DESKTOP_ID" ]; then
    printf '%s\n' "Browser Picker"
  else
    local desktop_file=""
    for directory in "$XDG_DATA_HOME/applications" "$XDG_DATA_DIRS/applications"; do
      if [ -f "$directory/$desktop_id" ]; then
        desktop_file="$directory/$desktop_id"
        break
      fi
    done
    if [ -n "$desktop_file" ]; then
      sed -n 's/^Name=//p' "$desktop_file" | head -n1
    else
      printf '%s\n' "$desktop_id"
    fi
  fi
}

assert_report_matches_gio() {
  local report="$1"
  local http_label https_label html_label xhtml_label
  http_label="$(handler_label "$(gio_default_id x-scheme-handler/http)")"
  https_label="$(handler_label "$(gio_default_id x-scheme-handler/https)")"
  html_label="$(handler_label "$(gio_default_id text/html)")"
  xhtml_label="$(handler_label "$(gio_default_id application/xhtml+xml)")"
  printf '%s\n' "$report" | grep -Fx "HTTP: $http_label"
  printf '%s\n' "$report" | grep -Fx "HTTPS: $https_label"
  printf '%s\n' "$report" | grep -Fx "HTML: $html_label"
  printf '%s\n' "$report" | grep -Fx "XHTML: $xhtml_label"
}

report="$("$BROWSER_PICKER/bin/browser-picker" associations)"
echo "$report"
assert_report_matches_gio "$report"

# A silent user file must still see the system XDG mimeapps location.
mv "$XDG_CONFIG_HOME/mimeapps.list" "$XDG_CONFIG_HOME/mimeapps.list.user"
https_system="$(gio mime x-scheme-handler/https)"
echo "$https_system"
echo "$https_system" | grep -F "$DESKTOP_ID"
system_report="$("$BROWSER_PICKER/bin/browser-picker" associations)"
echo "$system_report"
assert_report_matches_gio "$system_report"
mv "$XDG_CONFIG_HOME/mimeapps.list.user" "$XDG_CONFIG_HOME/mimeapps.list"

# Ordered defaults skip an unavailable first desktop entry.
cat > "$XDG_CONFIG_HOME/mimeapps.list" <<EOF
[Default Applications]
x-scheme-handler/http=missing.desktop;$DESKTOP_ID
x-scheme-handler/https=other-browser.desktop
text/html=other-browser.desktop
application/xhtml+xml=$DESKTOP_ID
EOF
ordered="$(gio mime x-scheme-handler/http)"
echo "$ordered"
[ "$(gio_default_id x-scheme-handler/http)" = "$DESKTOP_ID" ]
assert_report_matches_gio "$("$BROWSER_PICKER/bin/browser-picker" associations)"

# Desktop-specific mimeapps override the generic user file for that desktop only.
cat > "$XDG_CONFIG_HOME/mimeapps.list" <<EOF
[Default Applications]
x-scheme-handler/http=$DESKTOP_ID
x-scheme-handler/https=$DESKTOP_ID
text/html=$DESKTOP_ID
application/xhtml+xml=$DESKTOP_ID
EOF
cat > "$XDG_CONFIG_HOME/gnome-mimeapps.list" <<EOF
[Default Applications]
x-scheme-handler/http=other-browser.desktop
EOF
export XDG_CURRENT_DESKTOP=GNOME
gnome_http="$(gio mime x-scheme-handler/http)"
echo "$gnome_http"
echo "$gnome_http" | grep -F "other-browser.desktop"
assert_report_matches_gio "$("$BROWSER_PICKER/bin/browser-picker" associations)"

export XDG_CURRENT_DESKTOP=KDE
kde_http="$(gio mime x-scheme-handler/http)"
echo "$kde_http"
echo "$kde_http" | grep -F "$DESKTOP_ID"
assert_report_matches_gio "$("$BROWSER_PICKER/bin/browser-picker" associations)"

touch "$out"
