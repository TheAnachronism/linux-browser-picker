#!/usr/bin/env bash
# Repeatable KDE Plasma release scenario for Browser Picker.
# Run from an actual Plasma graphical session after `nix build .#browser-picker`.
# Uses isolated XDG state and never writes the operator's MIME defaults.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP_ID="io.github.TheAnachronism.BrowserPicker.desktop"
A11Y_INSPECT="${A11Y_INSPECT:-$ROOT/nix/a11y_inspect.py}"

desktop="${XDG_CURRENT_DESKTOP:-}"
session_type="${XDG_SESSION_TYPE:-}"
case ":${desktop}:" in
  *:[Kk][Dd][Ee]:*|*:[Pp][Ll][Aa][Ss][Mm][Aa]:*) ;;
  *)
    echo "This scenario requires an actual KDE Plasma session, not ${desktop:-an unknown desktop}." >&2
    exit 1
    ;;
esac
case ":${desktop}:" in
  *:[Gg][Nn][Oo][Mm][Ee]:*|*:[Nn][Ii][Rr][Ii]:*)
    echo "Refuse GNOME, niri, and mixed-session substitutes: XDG_CURRENT_DESKTOP=$desktop" >&2
    exit 1
    ;;
esac
if [ "$session_type" = "tty" ]; then
  echo "Refuse a tty session; Plasma must own the graphical session." >&2
  exit 1
fi
if ! pgrep -u "$(id -u)" -x plasmashell >/dev/null 2>&1; then
  echo "plasmashell is not running for this user; start a KDE Plasma session." >&2
  exit 1
fi
if [ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
  echo "Plasma must provide DISPLAY or WAYLAND_DISPLAY." >&2
  exit 1
fi
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
  echo "Plasma must provide a user session D-Bus." >&2
  exit 1
fi

if [ -z "${BROWSER_PICKER:-}" ]; then
  if [ -x "$ROOT/result/bin/browser-picker" ]; then
    BROWSER_PICKER="$ROOT/result"
  else
    echo "Set BROWSER_PICKER to the packaged output of nix build .#browser-picker" >&2
    exit 1
  fi
fi
if [ -x "$BROWSER_PICKER/bin/browser-picker" ]; then
  PICKER="$BROWSER_PICKER/bin/browser-picker"
elif [ -x "$BROWSER_PICKER" ]; then
  PICKER="$BROWSER_PICKER"
  BROWSER_PICKER="$(cd "$(dirname "$PICKER")/.." && pwd)"
else
  echo "Browser Picker executable was not found in $BROWSER_PICKER" >&2
  exit 1
fi

owned="$(
  gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner \
    io.github.TheAnachronism.BrowserPicker
)"
case "$owned" in
  *true*)
    echo "Browser Picker already owns the session bus name. Close it before this scenario." >&2
    exit 1
    ;;
esac

operator_home="${HOME}"
operator_config="${XDG_CONFIG_HOME:-$operator_home/.config}"
snapshot_dir="$(mktemp -d "${TMPDIR:-/tmp}/browser-picker-plasma-snapshot.XXXXXX")"
snapshot_operator() {
  local path="$1"
  local name="$2"
  if [ -e "$path" ]; then
    cp -a "$path" "$snapshot_dir/$name"
  fi
}
snapshot_operator "$operator_config/mimeapps.list" mimeapps.list
snapshot_operator "$operator_config/kde-mimeapps.list" kde-mimeapps.list
snapshot_operator "$operator_config/plasma-mimeapps.list" plasma-mimeapps.list
snapshot_operator "$operator_home/.config/mimeapps.list" home-mimeapps.list

workdir="$(mktemp -d "${TMPDIR:-/tmp}/browser-picker-plasma.XXXXXX")"
cleanup() {
  if [ -n "${picker_pid:-}" ]; then
    kill "$picker_pid" 2>/dev/null || true
    wait "$picker_pid" 2>/dev/null || true
  fi
  if [ -n "${config_pid:-}" ]; then
    kill "$config_pid" 2>/dev/null || true
    wait "$config_pid" 2>/dev/null || true
  fi
  unchanged=0
  for name in mimeapps.list kde-mimeapps.list plasma-mimeapps.list home-mimeapps.list; do
    case "$name" in
      home-mimeapps.list) current="$operator_home/.config/mimeapps.list" ;;
      *) current="$operator_config/${name}" ;;
    esac
    if [ -e "$snapshot_dir/$name" ]; then
      if ! cmp -s "$snapshot_dir/$name" "$current"; then
        echo "operator $current changed during the Plasma scenario" >&2
        unchanged=1
      fi
    elif [ -e "$current" ]; then
      echo "operator $current was created during the Plasma scenario" >&2
      unchanged=1
    fi
  done
  rm -rf "$snapshot_dir" "$workdir"
  if [ "$unchanged" -ne 0 ]; then
    exit 1
  fi
}
trap cleanup EXIT

export HOME="$workdir/home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_DATA_HOME="$HOME/.local/share"
export XDG_STATE_HOME="$HOME/.local/state"
export XDG_CACHE_HOME="$HOME/.cache"
export XDG_RUNTIME_DIR="$workdir/runtime"
export XDG_DATA_DIRS="$BROWSER_PICKER/share${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
export LANG="${LANG:-C.UTF-8}"
export LC_ALL="${LC_ALL:-C.UTF-8}"
export GTK_A11Y=atspi
export ADW_DISABLE_PORTAL=1
mkdir -p "$HOME" "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME/applications" \
  "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$XDG_CONFIG_HOME/browser-picker"
chmod 700 "$XDG_RUNTIME_DIR"
cp "$BROWSER_PICKER/share/applications/$DESKTOP_ID" "$XDG_DATA_HOME/applications/$DESKTOP_ID"

cat > "$workdir/controlled-browser" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" >> "${BROWSER_PICKER_TEST_OUTPUT:-$TMPDIR/received-argv}"
EOF
chmod 700 "$workdir/controlled-browser"

cat > "$XDG_DATA_HOME/applications/other-browser.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Other Browser
Exec=true %u
MimeType=x-scheme-handler/http;x-scheme-handler/https;text/html;application/xhtml+xml;
Terminal=false
EOF
cat > "$XDG_DATA_HOME/applications/aaa-controlled.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=AAA Controlled Browser
Exec=$workdir/controlled-browser %u
MimeType=x-scheme-handler/http;x-scheme-handler/https;
Terminal=false
EOF
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$XDG_DATA_HOME/applications"
else
  cat > "$XDG_DATA_HOME/applications/mimeinfo.cache" <<'EOF'
[MIME Cache]
x-scheme-handler/http=aaa-controlled.desktop;other-browser.desktop;
x-scheme-handler/https=aaa-controlled.desktop;other-browser.desktop;
text/html=other-browser.desktop;
application/xhtml+xml=other-browser.desktop;
EOF
fi

cat > "$XDG_CONFIG_HOME/mimeapps.list" <<EOF
[Default Applications]
x-scheme-handler/http=$DESKTOP_ID
x-scheme-handler/https=other-browser.desktop
text/html=other-browser.desktop
application/xhtml+xml=$DESKTOP_ID
EOF
cat > "$XDG_CONFIG_HOME/kde-mimeapps.list" <<EOF
[Default Applications]
x-scheme-handler/http=other-browser.desktop
EOF

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
    local directory
    IFS=':'
    set -- "$XDG_DATA_HOME/applications" ${XDG_DATA_DIRS:-}
    unset IFS
    for directory in "$@"; do
      directory="${directory%/applications}/applications"
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

report="$("$PICKER" associations)"
echo "$report"
echo "$report" | grep -Fx "HTTP: Other Browser"
assert_report_matches_gio "$report"

cat > "$XDG_CONFIG_HOME/browser-picker/config.toml" <<EOF
version = 1

[[destinations]]
id = "alpha"
label = "Alpha Browser"

[destinations.application]
type = "manual"
label = "Alpha Browser Application"
executable = "$workdir/controlled-browser"
args = ["--alpha", "{target}"]
private_args = ["--private-alpha", "{target}"]

[[destinations]]
id = "controlled"
label = "Work Browser"

[destinations.application]
type = "manual"
label = "Controlled Browser Application"
executable = "$workdir/controlled-browser"
args = ["--normal", "{target}"]
private_args = ["--private", "{target}"]

[fallback]
action = "show-picker"
EOF

inspect() {
  python3 "$A11Y_INSPECT" "$@"
}

press_keys() {
  python3 - "$@" <<'PY'
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi

Atspi.init()
press = getattr(Atspi.KeySynthType, "PRESS", None) or Atspi.KeySynthType.STRING
release = getattr(Atspi.KeySynthType, "RELEASE", None) or Atspi.KeySynthType.STRING
symbols = sys.argv[1:]
for symbol in symbols:
    Atspi.generate_keyboard_event(0, symbol, press)
    time.sleep(0.02)
for symbol in reversed(symbols):
    Atspi.generate_keyboard_event(0, symbol, release)
    time.sleep(0.02)
time.sleep(0.2)
PY
}

"$PICKER" config &
config_pid=$!
inspect wait "Desktop defaults" --timeout 30
inspect wait "Configuration" --timeout 5
tree="$(inspect dump)"
printf '%s\n' "$tree" | grep -F "System Settings"
printf '%s\n' "$tree" | grep -F "Applications"
printf '%s\n' "$tree" | grep -F "Default Applications"
printf '%s\n' "$tree" | grep -F "never changes system defaults"
printf '%s\n' "$tree" | grep -Fi "set as default" && {
  echo "KDE instructions offered a takeover action" >&2
  exit 1
} || true
printf '%s\n' "$tree" | grep -F "HTTP: Other Browser"
printf '%s\n' "$tree" | grep -F "HTTPS: Other Browser"
printf '%s\n' "$tree" | grep -F "HTML: Other Browser"
printf '%s\n' "$tree" | grep -F "XHTML: Browser Picker"
kill "$config_pid"
wait "$config_pid" || true
config_pid=

export BROWSER_PICKER_TEST_OUTPUT="$workdir/received-argv"
rm -f "$BROWSER_PICKER_TEST_OUTPUT"
"$PICKER" "https://plasma-first.example/" &
picker_pid=$!
inspect wait "Filter Browser Destinations" --timeout 30
inspect wait "Picker" --timeout 5
inspect wait "1 Pending Request"
inspect node "Filter Browser Destinations" --focused || inspect wait "Filter Browser Destinations"
inspect node "Alpha Browser" --role "list item" --enabled --shortcut "<Alt>1"
inspect node "Work Browser" --role "list item" --enabled --shortcut "<Alt>2"
inspect node "Private Launch Mode" --role "check box" --shortcut "<Control><Shift>p"

press_keys Alt_L 1
inspect node "Alpha Browser" --role "list item" --selected
press_keys Alt_L 2
inspect node "Work Browser" --role "list item" --selected
press_keys Control_L Shift_L p
inspect checked "Private Launch Mode"

gdbus call --session \
  --dest io.github.TheAnachronism.BrowserPicker \
  --object-path /io/github/TheAnachronism/BrowserPicker \
  --method org.freedesktop.Application.Open \
  "['https://plasma-second.example/']" \
  "{}" >/dev/null
inspect wait "2 Pending Requests" --timeout 20

press_keys Return
for attempt in $(seq 1 50); do
  if [ -f "$BROWSER_PICKER_TEST_OUTPUT" ]; then
    break
  fi
  sleep 0.1
done
grep -F "https://plasma-first.example/" "$BROWSER_PICKER_TEST_OUTPUT"
inspect wait "1 Pending Request" --timeout 20
inspect wait "Picker"

kill "$picker_pid" 2>/dev/null || true
wait "$picker_pid" 2>/dev/null || true
picker_pid=

echo "KDE Plasma session scenario completed without changing operator defaults"
