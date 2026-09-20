#!/usr/bin/env bash
set -eu

python3 "$I18N_COVERAGE" "$SRC_DIR" "$PO_FILE"
python3 - "$CHECKLIST" <<'PY'
from pathlib import Path
import sys

checklist = Path(sys.argv[1]).read_text()
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
print("release checklist is complete")
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

write_default_config() {
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

[[rules]]
id = "automatic"
name = "Automatic"
enabled = true

[rules.action]
type = "open"
destination = "controlled"
mode = "normal"

[[rules.groups]]

[[rules.groups.conditions]]
type = "host"
value = "automatic.example"

[fallback]
action = "show-picker"
EOF
}

write_default_config

inspect() {
  python3 "$A11Y_INSPECT" "$@"
}

scenario_tag() {
  python3 -c 'import uuid; print(uuid.uuid4().hex[:12])'
}

scan_privacy() {
  python3 - "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$HOME" "$@" <<'PY'
from pathlib import Path
import sys

roots = [Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4])]
parts = sys.argv[5:]
if "--" not in parts:
    raise SystemExit("scan_privacy requires -- before sentinels")
split = parts.index("--")
extra_files = [Path(part) for part in parts[:split]]
needles = parts[split + 1 :]
if not needles:
    raise SystemExit("scan_privacy requires sentinels")
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

for root in [*roots, *extra_files]:
    for path in files_of(root):
        if path.name in allowed_names or path.name.endswith("-argv"):
            continue
        text = path.read_text(errors="replace")
        blob = f"{path}\n{text}"
        for needle in needles:
            if needle and needle in blob:
                leaks.append(f"{path}: {needle}")
if leaks:
    raise SystemExit("logs or persistent state leaked private Open Target data:\n" + "\n".join(leaks))
print("generated sentinels are absent from logs and XDG state")
PY
}

assert_config() {
  python3 - "$XDG_CONFIG_HOME/browser-picker/config.toml" "$@" <<'PY'
from pathlib import Path
import sys
import tomllib

data = tomllib.load(Path(sys.argv[1]).open('rb'))
want = dict(item.split("=", 1) for item in sys.argv[2:])
destinations = {item["id"]: item for item in data.get("destinations", [])}
rules = {item["id"]: item for item in data.get("rules", [])}
ids = [item["id"] for item in data.get("destinations", [])]
rule_ids = [item["id"] for item in data.get("rules", [])]
if "destinations" in want and ids != want["destinations"].split(","):
    raise SystemExit(f"destination order {ids} != {want['destinations'].split(',')}")
if "rules" in want and rule_ids != want["rules"].split(","):
    raise SystemExit(f"rule order {rule_ids} != {want['rules'].split(',')}")
if "version" in want and str(data.get("version")) != want["version"]:
    raise SystemExit(f"version {data.get('version')!r} != {want['version']!r}")
if "fallback.action" in want and data.get("fallback", {}).get("action") != want["fallback.action"]:
    raise SystemExit(f"fallback action {data.get('fallback')} != {want['fallback.action']}")
if "fallback.destination" in want:
    actual = data.get("fallback", {}).get("destination")
    expected = want["fallback.destination"]
    if expected == "" and actual not in (None, ""):
        raise SystemExit(f"fallback destination {actual!r} should be absent")
    if expected != "" and actual != expected:
        raise SystemExit(f"fallback destination {actual!r} != {expected!r}")
for key, value in want.items():
    if key.startswith("dest.") and key.endswith(".type"):
        dest_id = key[len("dest.") : -len(".type")]
        actual = destinations[dest_id]["application"]["type"]
        if actual != value:
            raise SystemExit(f"{dest_id} type {actual!r} != {value!r}")
    if key.startswith("dest.") and key.endswith(".label"):
        dest_id = key[len("dest.") : -len(".label")]
        actual = destinations[dest_id]["label"]
        if actual != value:
            raise SystemExit(f"{dest_id} label {actual!r} != {value!r}")
    if key.startswith("rule.") and key.endswith(".destination"):
        rule_id = key[len("rule.") : -len(".destination")]
        if rule_id not in rules:
            raise SystemExit(f"missing rule {rule_id!r} among {sorted(rules)}")
        actual = rules[rule_id]["action"]["destination"]
        if actual != value:
            raise SystemExit(f"{rule_id} destination {actual!r} != {value!r}")
    if key.startswith("rule.") and key.endswith(".type"):
        rule_id = key[len("rule.") : -len(".type")]
        actual = rules[rule_id]["action"]["type"]
        if actual != value:
            raise SystemExit(f"{rule_id} type {actual!r} != {value!r}")
    if key.startswith("rule.") and key.endswith(".host"):
        rule_id = key[len("rule.") : -len(".host")]
        hosts = [
            condition["value"]
            for group in rules[rule_id].get("groups", [])
            for condition in group.get("conditions", [])
            if condition.get("type") == "host"
        ]
        if value not in hosts:
            raise SystemExit(f"{rule_id} hosts {hosts} missing {value!r}")
print("saved TOML parsed structurally", want)
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

wait_receipt() {
  expected=$1
  output=${BROWSER_PICKER_TEST_OUTPUT:-$TMPDIR/received-argv}
  for attempt in $(seq 1 100); do
    test "$(cat "$output" 2>/dev/null || true)" = "$expected" && return
    sleep 0.1
  done
  echo "controlled destination receipt did not match" >&2
  cat "$output" >&2 || true
  exit 1
}

focus_latest_picker_window() {
  set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
  if [ "$#" -eq 0 ]; then
    echo "Browser Picker window did not appear" >&2
    exit 1
  fi
  xdotool windowfocus --sync "${!#}" || true
  xdotool windowsize --sync "${!#}" 1600 1000 || true
}

close_windows() {
  for round in $(seq 1 8); do
    set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
    if [ "$#" -eq 0 ]; then
      return
    fi
    for candidate in "$@"; do
      xdotool windowfocus --sync "$candidate" || true
      xdotool key --clearmodifiers ctrl+w
    done
    sleep 0.15
  done
}

export ADW_DISABLE_PORTAL=1
export GDK_BACKEND=x11
export GSK_RENDERER=cairo
export GSETTINGS_BACKEND=memory
export NO_AT_BRIDGE=0
"$AT_SPI_LAUNCHER" --launch-immediately &
"$AT_SPI_REGISTRY" &
sleep 0.4

run_picker_reveal_filter_repair() {
  tag=$(scenario_tag)
  cred="cred-${tag}"
  query="query-${tag}"
  search="search-${tag}"
  selection="select-${tag}"
  file="$TMPDIR/path-${tag}.html"
  url="https://user:${cred}@suggested.example/${tag}?token=${query}"
  printf x > "$file"
  log="$TMPDIR/picker-reveal-${tag}.log"
  : > "$log"
  export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/picker-reveal-${tag}-argv"
  rm -f "$BROWSER_PICKER_TEST_OUTPUT"
  "$BROWSER_PICKER" "$url" >>"$log" 2>&1 &
  picker=$!
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  xdotool windowsize --sync "$window" 1600 1000
  sleep 0.4
  inspect wait "Filter Browser Destinations" --timeout 20
  inspect wait "Picker" --timeout 5
  inspect wait "Pending Request count"
  inspect node "Work Browser" --role "list item" --enabled --selected --focused --shortcut "<Alt>2"
  inspect node "Alpha Browser" --role "list item" --enabled --shortcut "<Alt>1"
  inspect node "Unavailable Browser" --role "list item" --disabled --shortcut "<Alt>3"
  inspect node "Repair" --role "button" --enabled
  inspect node "Private Launch Mode" --role "check box" --enabled --shortcut "<Control><Shift>p"
  inspect node "Reveal full URL details" --role "check box" --enabled
  inspect node "$url" --role "label" --description "Full Open Target details" --hidden
  inspect activate "Reveal full URL details"
  inspect checked "Reveal full URL details"
  inspect node "$url" --role "label" --description "Full Open Target details" --showing --enabled
  inspect node "Edit Routing Rules for This URL" --role "button" --enabled --shortcut "<Alt>e"

  xdotool windowfocus --sync "$window"
  inspect focus "Filter Browser Destinations"
  xdotool type "$search"
  inspect wait "No Browser Destinations match this filter."
  inspect node "Clear filter" --role "button" --enabled
  inspect node "Configure Browser Picker" --role "button" --enabled
  inspect activate "Clear filter"
  inspect wait "Work Browser"
  inspect node "Work Browser" --role "list item" --enabled --showing
  inspect focus "Filter Browser Destinations"
  xdotool type "$selection"
  inspect wait "No Browser Destinations match this filter."
  inspect activate "Clear filter"
  inspect wait "Work Browser"
  inspect wait "Repair"

  inspect activate "Repair"
  inspect wait "Configuration" --timeout 20
  inspect wait "Configuration"
  inspect node "Add Manual Browser Application" --role "button" --enabled
  inspect node "Refresh Browser Profiles" --role "button" --enabled
  xdotool key --clearmodifiers ctrl+w
  inspect wait "Picker" --timeout 10
  xdotool windowfocus --sync "$window"
  xdotool key --clearmodifiers ctrl+w
  wait "$picker"
  scan_privacy "$log" -- "$url" "$cred" "$query" "$search" "$selection" "$file"

  : > "$log"
  "$BROWSER_PICKER" "$file" >>"$log" 2>&1 &
  picker=$!
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  inspect wait "Reveal absolute path"
  inspect node "Reveal absolute path" --role "check box" --enabled
  inspect node "$file" --role "label" --description "Full Open Target details" --hidden
  inspect activate "Reveal absolute path"
  inspect checked "Reveal absolute path"
  inspect node "$file" --role "label" --description "Full Open Target details" --showing --enabled
  xdotool key --clearmodifiers ctrl+w
  wait "$picker"
  scan_privacy "$log" -- "$url" "$cred" "$query" "$search" "$selection" "$file"
  unset BROWSER_PICKER_TEST_OUTPUT
}

run_configuration_reorder_and_unsaved() {
  tag=$(scenario_tag)
  cred="cred-${tag}"
  query="query-${tag}"
  search="search-${tag}"
  selection="Renamed ${tag}"
  file="$TMPDIR/config-${tag}.html"
  url="https://user:${cred}@config-${tag}.example/path?token=${query}"
  printf x > "$file"
  log="$TMPDIR/config-reorder-${tag}.log"
  : > "$log"
  "$BROWSER_PICKER" config >>"$log" 2>&1 &
  setup=$!
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  xdotool windowsize --sync "$window" 1600 1000
  sleep 0.4
  inspect wait "Configuration" --timeout 20
  inspect node "Move destination up" --role "button" --enabled --shortcut "<Alt><Shift>Up"
  inspect node "Move destination down" --role "button" --enabled --shortcut "<Alt><Shift>Down"
  inspect node "Move Routing Rule up" --role "button" --enabled --shortcut "<Alt>Up"
  inspect node "Move Routing Rule down" --role "button" --enabled --shortcut "<Alt>Down"
  xdotool windowfocus --sync "$window"
  xdotool key --clearmodifiers alt+shift+Down
  xdotool key --clearmodifiers alt+Down
  xdotool key --clearmodifiers alt+s
  for attempt in $(seq 1 50); do
    assert_config \
      destinations=controlled,alpha,unavailable \
      rules=later,suggested,automatic \
      fallback.action=show-picker \
      version=1 && break
    sleep 0.1
  done
  assert_config \
    destinations=controlled,alpha,unavailable \
    rules=later,suggested,automatic \
    fallback.action=show-picker \
    version=1
  inspect node "Move destination up" --role "button" --enabled --shortcut "<Alt><Shift>Up"
  inspect node "Move destination down" --role "button" --enabled --shortcut "<Alt><Shift>Down"
  inspect node "Move Routing Rule up" --role "button" --enabled --shortcut "<Alt>Up"
  inspect node "Move Routing Rule down" --role "button" --enabled --shortcut "<Alt>Down"

  xdotool windowfocus --sync "$window"
  inspect focus "Browser Destination display label"
  xdotool key --clearmodifiers ctrl+a
  xdotool type "$selection"
  xdotool key --clearmodifiers ctrl+w
  inspect wait "Unsaved changes" --timeout 10
  inspect wait "Unsaved changes"
  inspect wait "Save the configuration draft before closing?"
  xdotool key --clearmodifiers Escape
  inspect wait "Configuration" --timeout 5
  inspect wait "Configuration" --timeout 5
  xdotool key --clearmodifiers ctrl+w
  inspect wait "Unsaved changes" --timeout 10
  xdotool key --clearmodifiers alt+d
  wait "$setup" || true
  assert_config dest.controlled.label="Work Browser"
  scan_privacy "$log" -- "$url" "$cred" "$query" "$search" "$selection" "$file"
}

run_contextual_rule_and_saved_routing() {
  tag=$(scenario_tag)
  cred="cred-${tag}"
  query="query-${tag}"
  search="search-${tag}"
  selection="select-${tag}"
  file="$TMPDIR/context-${tag}.html"
  host="context-${tag}.example"
  url="https://user:${cred}@${host}/path?token=${query}"
  printf x > "$file"
  log="$TMPDIR/context-${tag}.log"
  : > "$log"
  write_default_config
  export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/context-${tag}-argv"
  rm -f "$BROWSER_PICKER_TEST_OUTPUT"
  "$BROWSER_PICKER" "$url" >>"$log" 2>&1 &
  picker=$!
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  inspect wait "Picker"
  inspect node "Edit Routing Rules for This URL" --role "button" --enabled --shortcut "<Alt>e"
  inspect activate "Edit Routing Rules for This URL"
  inspect wait "Configuration" --timeout 20
  inspect wait "Matching URL: "
  focus_latest_picker_window
  inspect node "Add Routing Rule" --role "button" --enabled
  inspect node "Save and Apply" --role "button" --enabled --shortcut "<Alt>s"
  inspect activate "Add Routing Rule"
  inspect wait "Route $host"
  inspect node "Route $host" --role "list item" --showing
  set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
  xdotool windowfocus --sync "${!#}" || true
  sleep 0.3
  xdotool key --clearmodifiers Tab Tab Tab
  sleep 0.2
  xdotool key --clearmodifiers ctrl+a
  xdotool type "controlled"
  inspect combo "Preselect in Picker" "Open automatically" --last
  inspect node "Open automatically" --role "combo box"

  automatic="https://automatic.example/draft-${tag}"
  "$BROWSER_PICKER" "$automatic"
  wait_receipt "--normal
$automatic"
  test ! -f "$BROWSER_PICKER_TEST_OUTPUT" || true
  draft="https://${host}/still-draft"
  "$BROWSER_PICKER" "$draft"
  sleep 0.4
  if grep -Fq "$draft" "$BROWSER_PICKER_TEST_OUTPUT"; then
    echo "unsaved draft routed a live arrival" >&2
    cat "$BROWSER_PICKER_TEST_OUTPUT" >&2
    exit 1
  fi

  set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
  xdotool windowfocus --sync "${!#}" || true
  inspect activate "Save and Apply"
  for attempt in $(seq 1 50); do
    if assert_config "rule.rule-4.host=$host" "rule.rule-4.destination=controlled" "rule.rule-4.type=open" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
  if ! assert_config "rule.rule-4.host=$host" "rule.rule-4.destination=controlled" "rule.rule-4.type=open"; then
    echo "saved configuration after contextual Save and Apply:" >&2
    cat "$XDG_CONFIG_HOME/browser-picker/config.toml" >&2 || true
    exit 1
  fi
  test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic"
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  inspect wait "$host"
  xdotool key --clearmodifiers ctrl+w
  wait "$picker" || true

  next="https://${host}/after-save?token=${query}"
  "$BROWSER_PICKER" "$next"
  wait_receipt "--normal
$automatic
--normal
$next"
  scan_privacy "$log" -- "$url" "$cred" "$query" "$search" "$selection" "$file" "user:${cred}"
  unset BROWSER_PICKER_TEST_OUTPUT
}

run_manual_and_profile_setup() {
  tag=$(scenario_tag)
  cred="cred-${tag}"
  query="query-${tag}"
  search="search-${tag}"
  selection="Graphical ${tag}"
  file="$TMPDIR/manual-${tag}.html"
  url="https://user:${cred}@manual-${tag}.example/path?token=${query}"
  printf x > "$file"
  log="$TMPDIR/manual-${tag}.log"
  : > "$log"
  export XDG_CONFIG_HOME="$TMPDIR/manual-config-${tag}"
  mkdir -p "$XDG_CONFIG_HOME"
  export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/manual-${tag}-argv"
  rm -f "$BROWSER_PICKER_TEST_OUTPUT"
  cat > "$TMPDIR/graphical-browser-${tag}" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" > "${BROWSER_PICKER_TEST_OUTPUT:-$TMPDIR/received-argv}"
EOF
  chmod 700 "$TMPDIR/graphical-browser-${tag}"
  mkdir -p "$HOME/.mozilla/firefox/abcd1234.default" "$TMPDIR/firefox-bin"
  cat > "$TMPDIR/firefox-bin/firefox" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" >> "${BROWSER_PICKER_TEST_OUTPUT:-$TMPDIR/received-argv}"
EOF
  chmod 700 "$TMPDIR/firefox-bin/firefox"
  cat > "$HOME/.mozilla/firefox/profiles.ini" <<'EOF'
[General]
StartWithLastProfile=1

[Profile0]
Name=default-release
IsRelative=1
Path=abcd1234.default
Default=1
EOF
  cat > "$XDG_DATA_HOME/applications/firefox.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Firefox
Exec=$TMPDIR/firefox-bin/firefox %u
MimeType=x-scheme-handler/http;x-scheme-handler/https;
Terminal=false
EOF
  cat > "$XDG_DATA_HOME/applications/mimeinfo.cache" <<'EOF'
[MIME Cache]
x-scheme-handler/http=aaa-controlled.desktop;firefox.desktop;
x-scheme-handler/https=aaa-controlled.desktop;firefox.desktop;
EOF

  "$BROWSER_PICKER" config >>"$log" 2>&1 &
  setup=$!
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  inspect wait "Configuration" --timeout 20
  xdotool windowsize --sync "$window" 1600 1000 || true
  inspect wait "Enable Browser Candidate Firefox — default-release" --timeout 20
  inspect node "Enable Browser Candidate Firefox — default-release" --role "check box" --enabled
  inspect wait "Family assumptions for this Browser Profile"
  inspect activate "Enable Browser Candidate Firefox — default-release"
  inspect checked "Enable Browser Candidate Firefox — default-release"
  inspect node "Add Manual Browser Application" --role "button" --enabled
  inspect activate "Add Manual Browser Application"
  inspect wait "Manual executable"
  inspect set-text -- "Browser Destination ID" "graphical-manual"
  inspect set-text -- "Browser Destination display label" "$selection"
  inspect set-text -- "Browser Application label" "Graphical Browser Application"
  inspect set-text -- "Manual executable" "$TMPDIR/graphical-browser-${tag}"
  inspect set-text -- "Normal literal arguments" $'--graphical\n{target}'
  inspect tab-to "Show Picker"
  inspect combo "Show Picker" "Open $selection"
  inspect node "Save configuration" --role "button" --enabled --shortcut "<Alt>s"
  inspect activate "Save configuration"
  for attempt in $(seq 1 50); do
    test -f "$XDG_CONFIG_HOME/browser-picker/config.toml" && break
    sleep 0.1
  done
  assert_config \
    dest.graphical-manual.type=manual \
    dest.graphical-manual.label="$selection" \
    fallback.action=open \
    fallback.destination=graphical-manual \
    dest.firefox-default-release.type=firefox-profile
  xdotool windowfocus --sync "$window"
  xdotool key --clearmodifiers ctrl+w
  wait "$setup" || true
  "$BROWSER_PICKER" "$url"
  wait_receipt "--graphical
$url"
  scan_privacy "$log" -- "$cred" "$query" "$search" "$file" "user:${cred}"
  export XDG_CONFIG_HOME="$TMPDIR/xdg-config"
  unset BROWSER_PICKER_TEST_OUTPUT
}

run_invalid_draft_and_external_conflict() {
  tag=$(scenario_tag)
  cred="cred-${tag}"
  query="query-${tag}"
  search="search-${tag}"
  selection="select-${tag}"
  file="$TMPDIR/conflict-${tag}.html"
  url="https://user:${cred}@conflict-${tag}.example/path?token=${query}"
  printf x > "$file"
  log="$TMPDIR/conflict-${tag}.log"
  : > "$log"
  export XDG_CONFIG_HOME="$TMPDIR/conflict-config-${tag}"
  mkdir -p "$XDG_CONFIG_HOME/browser-picker"
  cat > "$XDG_CONFIG_HOME/browser-picker/config.toml" <<EOF
version = 1

[[destinations]]
id = "work"
label = "Work Browser"

[destinations.application]
type = "manual"
label = "Work Browser Application"
executable = "$TMPDIR/controlled-browser"
args = ["--work", "{target}"]

[[rules]]
id = "auto"
name = "Automatic"
enabled = true

[rules.action]
type = "open"
destination = "work"
mode = "normal"

[[rules.groups]]

[[rules.groups.conditions]]
type = "host"
value = "rename.example"

[fallback]
action = "open"
destination = "work"
mode = "normal"
EOF
  chmod 600 "$XDG_CONFIG_HOME/browser-picker/config.toml"
  python3 - "$XDG_CONFIG_HOME/browser-picker/config.toml" "$TMPDIR/conflict-before-${tag}.toml" <<'PY'
from pathlib import Path
import sys
import tomllib
src, dest = Path(sys.argv[1]), Path(sys.argv[2])
dest.write_bytes(src.read_bytes())
tomllib.load(src.open('rb'))
PY

  "$BROWSER_PICKER" config >>"$log" 2>&1 &
  setup=$!
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  inspect wait "Configuration" --timeout 20
  inspect node "Remove AND condition" --role "button" --enabled
  inspect activate "Remove AND condition"
  inspect wait "condition groups must not be empty"
  inspect wait "condition groups must not be empty"
  xdotool key --clearmodifiers alt+s
  sleep 0.3
  python3 - "$XDG_CONFIG_HOME/browser-picker/config.toml" "$TMPDIR/conflict-before-${tag}.toml" <<'PY'
from pathlib import Path
import sys
import tomllib
current = tomllib.load(Path(sys.argv[1]).open('rb'))
before = tomllib.load(Path(sys.argv[2]).open('rb'))
if current != before:
    raise SystemExit(f"invalid draft overwrote saved configuration: {current} != {before}")
print("invalid draft left saved configuration unchanged")
PY

  inspect activate "Add AND condition"
  inspect wait "URL Condition value"
  inspect set-text -- "URL Condition value" "conflict-${tag}.example"
  python3 - "$XDG_CONFIG_HOME/browser-picker/config.toml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
if 'label = "Work Browser"' not in text:
    raise SystemExit(f"conflict fixture missing dest label:\n{text}")
path.write_text(text.replace('label = "Work Browser"', 'label = "External Edit"', 1))
if "External Edit" not in path.read_text():
    raise SystemExit("external editor change was not written")
print("external editor changed saved destination label")
PY
  inspect tab-to "Save configuration"
  inspect activate "Save configuration"
  inspect wait "Configuration changed on disk" --timeout 15
  inspect node "Overwrite" --role "button" --enabled
  inspect activate "Overwrite"
  for attempt in $(seq 1 50); do
    assert_config dest.work.label="Work Browser" rule.auto.host="conflict-${tag}.example" && break
    sleep 0.1
  done
  assert_config dest.work.label="Work Browser" rule.auto.host="conflict-${tag}.example"
  set -- "$XDG_CONFIG_HOME/browser-picker"/config.toml.bak-*
  test -f "$1"
  grep -q "External Edit" "$1"
  if grep -q "External Edit" "$XDG_CONFIG_HOME/browser-picker/config.toml"; then
    printf '%s\n' "overwrite left external edit in live configuration"
    exit 1
  fi
  xdotool windowfocus --sync "$window"
  xdotool key --clearmodifiers ctrl+w
  wait "$setup" || true
  scan_privacy "$log" -- "$url" "$cred" "$query" "$search" "$selection" "$file"
  export XDG_CONFIG_HOME="$TMPDIR/xdg-config"
}

run_migration_keeps_pending() {
  tag=$(scenario_tag)
  cred="cred-${tag}"
  query="query-${tag}"
  search="search-${tag}"
  selection="select-${tag}"
  file="$TMPDIR/migrate-${tag}.html"
  url="https://user:${cred}@migrate-${tag}.example/pending?token=${query}"
  printf x > "$file"
  log="$TMPDIR/migrate-${tag}.log"
  : > "$log"
  export XDG_CONFIG_HOME="$TMPDIR/migrate-config-${tag}"
  mkdir -p "$XDG_CONFIG_HOME/browser-picker"
  export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/migrate-${tag}-argv"
  rm -f "$BROWSER_PICKER_TEST_OUTPUT"
  write_default_config
  sed -i "s/version = 1/version = 0/" "$XDG_CONFIG_HOME/browser-picker/config.toml"
  grep -q "version = 0" "$XDG_CONFIG_HOME/browser-picker/config.toml"
  "$BROWSER_PICKER" "$url" >>"$log" 2>&1 &
  picker=$!
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  inspect wait "Configuration migration required" --timeout 20
  inspect node "Migrate" --role "button" --enabled --shortcut "<Alt>m"
  inspect node "Cancel" --role "button" --enabled
  test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
  inspect activate "Migrate"
  for attempt in $(seq 1 50); do
    grep -q "version = 1" "$XDG_CONFIG_HOME/browser-picker/config.toml" && break
    sleep 0.1
  done
  assert_config version=1
  test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
  inspect wait "Picker" --timeout 20
  inspect wait "Picker"
  xdotool windowfocus --sync "$(wait_window)"
  xdotool key alt+1
  wait_receipt "--alpha
$url"
  wait "$picker"
  scan_privacy "$log" -- "$cred" "$query" "$search" "$selection" "$file" "user:${cred}"
  export XDG_CONFIG_HOME="$TMPDIR/xdg-config"
  unset BROWSER_PICKER_TEST_OUTPUT
}

run_unavailable_and_launch_recovery() {
  tag=$(scenario_tag)
  cred="cred-${tag}"
  query="query-${tag}"
  search="search-${tag}"
  selection="select-${tag}"
  file="$TMPDIR/recover-${tag}.html"
  printf x > "$file"
  log="$TMPDIR/recover-${tag}.log"
  : > "$log"
  export XDG_CONFIG_HOME="$TMPDIR/recover-config-${tag}"
  mkdir -p "$XDG_CONFIG_HOME/browser-picker" "$TMPDIR/private-profile-${tag}"
  export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/recover-${tag}-argv"
  rm -f "$BROWSER_PICKER_TEST_OUTPUT"
  cat > "$XDG_CONFIG_HOME/browser-picker/config.toml" <<EOF
version = 1

[[destinations]]
id = "unavailable"
label = "Unavailable Browser"

[destinations.application]
type = "manual"
executable = "/definitely/missing/browser"
args = ["{target}"]

[[destinations]]
id = "controlled"
label = "Work Browser"

[destinations.application]
type = "manual"
executable = "$TMPDIR/controlled-browser"
args = ["--normal", "{target}"]
private_args = ["--private", "{target}"]

[[destinations]]
id = "firefox-work"
label = "Work Firefox"
profile_label = "Work"

[destinations.application]
type = "firefox-profile"
desktop_id = "firefox.desktop"
name = "Work"
path = "$TMPDIR/private-profile-${tag}"

[[rules]]
id = "launch-failure"
name = "Launch failure"
enabled = true

[rules.action]
type = "open"
destination = "unavailable"
mode = "normal"

[[rules.groups]]

[[rules.groups.conditions]]
type = "host"
value = "launch-failure.example"

[[rules]]
id = "private-failure"
name = "Private failure"
enabled = true

[rules.action]
type = "open"
destination = "firefox-work"
mode = "private"

[[rules.groups]]

[[rules.groups.conditions]]
type = "host"
value = "private-failure.example"

[fallback]
action = "show-picker"
EOF
  mkdir -p "$TMPDIR/snap/bin"
  cat > "$TMPDIR/snap/bin/firefox" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" >> "${BROWSER_PICKER_TEST_OUTPUT:-$TMPDIR/received-argv}"
EOF
  chmod 700 "$TMPDIR/snap/bin/firefox"
  cat > "$XDG_DATA_HOME/applications/firefox.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Firefox
Exec=$TMPDIR/snap/bin/firefox %u
MimeType=x-scheme-handler/http;x-scheme-handler/https;
Terminal=false
EOF

  failed="https://user:${cred}@launch-failure.example/${tag}?token=${query}"
  "$BROWSER_PICKER" "$failed" "$file" >>"$log" 2>&1 &
  picker=$!
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  inspect wait "executable was not found"
  inspect wait "could not accept dispatch"
  inspect node "Unavailable Browser" --role "list item" --selected --focused
  inspect node "Repair configuration" --role "button" --enabled --shortcut "<Alt>r"
  inspect node "Repair" --role "button" --enabled
  test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
  xdotool key alt+2
  wait_receipt "--normal
$failed"
  inspect wait "$(basename "$file")"
  xdotool key alt+2
  wait_receipt "--normal
$failed
--normal
file://$file"
  wait "$picker"

  rm -f "$BROWSER_PICKER_TEST_OUTPUT"
  private="https://user:${cred}@private-failure.example/${tag}?token=${query}"
  "$BROWSER_PICKER" "$private" >>"$log" 2>&1 &
  picker=$!
  window=$(wait_window)
  xdotool windowfocus --sync "$window"
  inspect wait "private Launch Mode is not available"
  inspect wait "could not accept dispatch"
  inspect node "Work Firefox" --role "list item" --selected --focused
  inspect node "Repair configuration" --role "button" --enabled --shortcut "<Alt>r"
  test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
  xdotool key alt+2
  wait_receipt "--private
$private"
  wait "$picker"
  scan_privacy "$log" -- "$cred" "$query" "$search" "$selection" "$file" "user:${cred}"
  export XDG_CONFIG_HOME="$TMPDIR/xdg-config"
  unset BROWSER_PICKER_TEST_OUTPUT
}

run_picker_reveal_filter_repair
run_configuration_reorder_and_unsaved
run_contextual_rule_and_saved_routing
run_manual_and_profile_setup
run_invalid_draft_and_external_conflict
run_migration_keeps_pending
run_unavailable_and_launch_recovery
