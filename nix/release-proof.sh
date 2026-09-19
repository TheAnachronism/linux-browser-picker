#!/usr/bin/env bash
set -eu

python3 - "$PO_FILE" "$CHECKLIST" <<'PY'
from pathlib import Path
import re
import sys

po = Path(sys.argv[1]).read_text()
if re.search(r"\b(TODO|FIXME|TBD|WIP)\b", po):
    raise SystemExit("translation catalog contains placeholder copy")
for needle in ("lorem ipsum", "coming soon", "unfinished"):
    if needle in po.lower():
        raise SystemExit(f"translation catalog contains placeholder copy: {needle}")
if po.count("msgid ") < 20:
    raise SystemExit("English catalog looks incomplete")
if re.search(r'^msgstr ""\n\n', po, flags=re.M):
    raise SystemExit("English catalog has an empty translation")

checklist = Path(sys.argv[2]).read_text()
for heading in (
    "Automated coverage",
    "Manual accessibility and desktop checklist",
    "Workstation Zen applications",
    "Rofi picker",
    "MIME defaults",
    "Parent specification coverage",
):
    if heading not in checklist:
        raise SystemExit(f"release checklist missing {heading!r}")
print("English catalog and release checklist are complete")
PY

mkdir -p "$HOME" "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME/applications" \
  "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$XDG_CONFIG_HOME/browser-picker"
chmod 700 "$XDG_RUNTIME_DIR"

cat > "$TMPDIR/controlled-browser" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" >> "${BROWSER_PICKER_TEST_OUTPUT:-$TMPDIR/received-argv}"
EOF
chmod 700 "$TMPDIR/controlled-browser"

cat > "$XDG_DATA_HOME/applications/aaa-controlled.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=AAA Controlled Browser
Exec=$TMPDIR/controlled-browser %u
MimeType=x-scheme-handler/http;x-scheme-handler/https;
Terminal=false
EOF
cat > "$XDG_DATA_HOME/applications/mimeinfo.cache" <<'EOF'
[MIME Cache]
x-scheme-handler/http=aaa-controlled.desktop;
x-scheme-handler/https=aaa-controlled.desktop;
EOF

cat > "$XDG_CONFIG_HOME/browser-picker/config.toml" <<EOF
# Browser Picker configuration
version = 1

[[destinations]]
id = "alpha"
label = "Alpha Browser"

[destinations.application]
type = "manual"
label = "Alpha Browser Application"
executable = "$TMPDIR/controlled-browser"
args = ["--alpha", "{target}"]
private_args = ["--private-alpha", "{target}"]

[[destinations]]
id = "controlled"
label = "Work Browser"
profile_label = "Work Profile"

[destinations.application]
type = "manual"
label = "Controlled Browser Application"
executable = "$TMPDIR/controlled-browser"
args = ["--normal", "{target}"]
private_args = ["--private", "{target}"]

[[destinations]]
id = "unavailable"
label = "Unavailable Browser"

[destinations.application]
type = "manual"
label = "Missing Browser Application"
executable = "/definitely/missing/browser"
args = ["{target}"]

[[rules]]
id = "suggested"
name = "Suggested"
enabled = true

[rules.action]
type = "preselect"
destination = "controlled"
mode = "private"

[[rules.groups]]

[[rules.groups.conditions]]
type = "host"
value = "suggested.example"

[[rules]]
id = "later"
name = "Later Rule"
enabled = true

[rules.action]
type = "open"
destination = "alpha"
mode = "normal"

[[rules.groups]]

[[rules.groups.conditions]]
type = "host"
value = "later.example"

[fallback]
action = "show-picker"
EOF

inspect() {
  python3 "$A11Y_INSPECT" "$@"
}

scan_privacy() {
  python3 - "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$HOME" "$@" <<'PY'
from pathlib import Path
import sys

roots = [Path(part) for part in sys.argv[1:]]
forbidden = (
    "token=",
    "user:secret",
    "kept-private",
    "do-not-print",
    "Pending Request",
    "https://user:",
    "https://xn--",
    "suggested.example/secret",
)
allowed_names = {"received-argv"}
leaks = []

def files_of(root: Path):
    if root.is_file():
        yield root
        return
    if not root.exists():
        return
    for path in root.rglob("*"):
        if path.is_file():
            yield path

for root in roots:
    for path in files_of(root):
        if path.name in allowed_names or path.name.endswith("-argv"):
            continue
        text = path.read_text(errors="replace")
        blob = f"{path}\n{text}"
        for needle in forbidden:
            if needle in blob:
                leaks.append(f"{path}: {needle}")
if leaks:
    raise SystemExit("logs or persistent state leaked private Open Target data:\n" + "\n".join(leaks))
state = Path(sys.argv[2]) / "browser-picker"
if state.exists():
    for path in state.rglob("*"):
        if not path.is_file():
            continue
        text = path.read_text(errors="replace")
        if "https://" in text or "file://" in text:
            raise SystemExit(f"{path} persisted an Open Target")
print("logs and persistent XDG state contain no Open Target secrets")
PY
}

assert_order() {
  python3 - "$XDG_CONFIG_HOME/browser-picker/config.toml" "$1" "$2" "$3" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text()
want_ids = sys.argv[2].split(",")
want_rules = sys.argv[3].split(",")
label = sys.argv[4]
ids = []
for block in text.split("[[destinations]]")[1:]:
    for line in block.splitlines():
        if line.startswith("id = "):
            ids.append(line.split("=", 1)[1].strip().strip('"'))
            break
rules = []
for block in text.split("[[rules]]")[1:]:
    for line in block.splitlines():
        if line.startswith("id = "):
            rules.append(line.split("=", 1)[1].strip().strip('"'))
            break
if ids[: len(want_ids)] != want_ids:
    raise SystemExit(f"destination order not {label}: {ids} (want {want_ids})")
if rules[: len(want_rules)] != want_rules:
    raise SystemExit(f"routing rule order not {label}: {rules} (want {want_rules})")
print(f"{label} ordering persisted", ids, rules)
PY
}

wait_window() {
  for attempt in $(seq 1 100); do
    set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
    if [ "$#" -gt 0 ]; then
      printf "%s\n" "$1"
      return
    fi
    sleep 0.1
  done
  echo "Browser Picker window did not appear" >&2
  exit 1
}

export ADW_DISABLE_PORTAL=1
export GDK_BACKEND=x11
export GSK_RENDERER=cairo
export GSETTINGS_BACKEND=memory
export NO_AT_BRIDGE=0
"$AT_SPI_LAUNCHER" --launch-immediately &
"$AT_SPI_REGISTRY" &
sleep 0.4

secret="https://user:secret@suggested.example/secret?token=kept-private"
PICKER_LOG="$TMPDIR/picker-session.log"
CONFIG_LOG="$TMPDIR/config-session.log"
: > "$PICKER_LOG"
: > "$CONFIG_LOG"
"$BROWSER_PICKER" "$secret" >>"$PICKER_LOG" 2>&1 &
picker=$!
window=$(wait_window)
xdotool windowfocus --sync "$window"
xdotool windowsize --sync "$window" 1600 1000
sleep 0.4
inspect wait "Filter Browser Destinations" --timeout 20
inspect wait "Picker" --timeout 5
inspect assert \
  "Picker" \
  "Filter Browser Destinations" \
  "Browser Destinations" \
  "Private Launch Mode" \
  "Work Browser" \
  "Unavailable Browser" \
  "Alpha Browser" \
  "Pending Request count" \
  "Open Target title" \
  "Private Launch Mode privacy boundary" \
  "Browser Application is not installed" \
  "state:selected" \
  "<Alt>2" \
  "<Control><Shift>p"
inspect node "Work Browser" --role "list item" --enabled --selected --shortcut "<Alt>2"
inspect node "Alpha Browser" --role "list item" --enabled --shortcut "<Alt>1"
inspect node "Unavailable Browser" --role "list item" --disabled --shortcut "<Alt>3"
inspect node "Repair" --role "button" --enabled
xdotool windowfocus --sync "$window"
xdotool key --clearmodifiers ctrl+w
wait "$picker"
scan_privacy "$PICKER_LOG"

"$BROWSER_PICKER" config >>"$CONFIG_LOG" 2>&1 &
setup=$!
window=$(wait_window)
xdotool windowfocus --sync "$window"
xdotool windowsize --sync "$window" 1600 1000
sleep 0.4
inspect wait "Configuration" --timeout 20
inspect assert \
  "Configuration" \
  "Browser Candidates" \
  "Move destination up" \
  "Move destination down" \
  "Ordered Routing Rules" \
  "Move Routing Rule up" \
  "Move Routing Rule down" \
  "Matching URL to test" \
  "Test Rules" \
  "Routing Rule explanation" \
  "Fallback Action" \
  "Desktop defaults" \
  "<Alt><Shift>Down" \
  "<Alt>Up"
xdotool windowfocus --sync "$window"
xdotool key --clearmodifiers alt+shift+Down
xdotool key --clearmodifiers alt+Down
xdotool key --clearmodifiers alt+s
for attempt in $(seq 1 50); do
  assert_order controlled,alpha later,suggested keyboard && break
  sleep 0.1
done
assert_order controlled,alpha later,suggested keyboard
inspect assert \
  "Move destination up" \
  "Move destination down" \
  "Move Routing Rule up" \
  "Move Routing Rule down" \
  "<Alt><Shift>Down" \
  "<Alt>Up"

xdotool windowfocus --sync "$window"
inspect focus "Browser Destination display label"
xdotool key --clearmodifiers ctrl+a
xdotool type "Renamed Destination"
xdotool key --clearmodifiers ctrl+w
inspect wait "Unsaved changes" --timeout 10
inspect assert "Unsaved changes" "Save the configuration draft before closing?"
xdotool key --clearmodifiers Escape
inspect wait "Configuration" --timeout 5
xdotool key --clearmodifiers ctrl+w
inspect wait "Unsaved changes" --timeout 10
xdotool key --clearmodifiers alt+d
wait "$setup" || true
scan_privacy "$CONFIG_LOG"
