#!/usr/bin/env bash
set -eu

                export ADW_DISABLE_PORTAL=1
                export GDK_BACKEND=x11
                export GSK_RENDERER=cairo
                export GSETTINGS_BACKEND=memory
                export NO_AT_BRIDGE=0
                "$AT_SPI_LAUNCHER" --launch-immediately &
                "$AT_SPI_REGISTRY" &
                sleep 0.4
                inspect="$A11Y_INSPECT"

                export HOME="$TMPDIR/home"
                export LANG=C.UTF-8
                export LC_ALL=C.UTF-8
                export XDG_RUNTIME_DIR="$TMPDIR/runtime"
                export XDG_DATA_HOME="$TMPDIR/xdg-data"
                export XDG_STATE_HOME="$TMPDIR/xdg-state"
                export XDG_CACHE_HOME="$TMPDIR/xdg-cache"
                export XDG_DATA_DIRS="$BROWSER_PICKER/share${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
                mkdir -p "$HOME" "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME/applications" \
                  "$XDG_STATE_HOME" "$XDG_CACHE_HOME"
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
                cat > "$XDG_DATA_HOME/applications/http-only.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=HTTP Only Browser
Exec=$TMPDIR/controlled-browser %u
MimeType=x-scheme-handler/http;
Terminal=false
EOF
                cat > "$XDG_DATA_HOME/applications/mimeinfo.cache" <<'EOF'
[MIME Cache]
x-scheme-handler/http=aaa-controlled.desktop;http-only.desktop;
x-scheme-handler/https=aaa-controlled.desktop;
EOF

                export XDG_CONFIG_HOME="$TMPDIR/config"
                mkdir -p "$XDG_CONFIG_HOME/browser-picker"
                cat > "$XDG_CONFIG_HOME/browser-picker/config.toml" <<EOF
# Browser Picker configuration
version = 1

[[destinations]]
id = "unavailable"
label = "Unavailable Browser"

[destinations.application]
type = "manual"
label = "Missing Browser Application"
executable = "/definitely/missing/browser"
args = ["{target}"]

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

[fallback]
action = "show-picker"
EOF


                    export ADW_DISABLE_PORTAL=1
                    export GDK_BACKEND=x11
                    export GSK_RENDERER=cairo
                    export GSETTINGS_BACKEND=memory
                    export NO_AT_BRIDGE=0
                    "$AT_SPI_LAUNCHER" --launch-immediately &
                    "$AT_SPI_REGISTRY" &
                    sleep 0.4
                    inspect="$A11Y_INSPECT"

                    run_and_assert_window() {
                      launcher=
                      pid=
                      cleanup() {
                        test -z "$pid" || kill "$pid" 2>/dev/null || true
                        test -z "$launcher" || kill "$launcher" 2>/dev/null || true
                      }
                      trap cleanup EXIT

                      "$@" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        kill -0 "$launcher" 2>/dev/null || true
                        sleep 0.1
                      done

                      test -n "$window"
                      test "$(xdotool getwindowname "$window")" = "Browser Picker"
                      for attempt in $(seq 1 100); do
                        gdbus introspect \
                          --session \
                          --dest io.github.TheAnachronism.BrowserPicker \
                          --object-path /io/github/TheAnachronism/BrowserPicker \
                          >/dev/null 2>/dev/null && break
                        sleep 0.1
                      done
                      gdbus introspect \
                        --session \
                        --dest io.github.TheAnachronism.BrowserPicker \
                        --object-path /io/github/TheAnachronism/BrowserPicker \
                        >/dev/null
                      pid=$(xdotool getwindowpid "$window")
                      test "$pid" -gt 1
                      kill "$pid"
                      pid=
                      wait "$launcher" || true
                      launcher=
                      trap - EXIT
                    }

                    gapplication_open() {
                      gdbus call --session \
                        --dest io.github.TheAnachronism.BrowserPicker \
                        --object-path /io/github/TheAnachronism/BrowserPicker \
                        --method org.freedesktop.Application.Open \
                        "$1" \
                        "{}" >/dev/null
                    }


                    run_installed_handler_activations() {
                      mkdir -p "$XDG_CONFIG_HOME"
                      cat > "$XDG_CONFIG_HOME/mimeapps.list" <<'EOF'
[Default Applications]
x-scheme-handler/http=io.github.TheAnachronism.BrowserPicker.desktop
x-scheme-handler/https=io.github.TheAnachronism.BrowserPicker.desktop
text/html=io.github.TheAnachronism.BrowserPicker.desktop
application/xhtml+xml=io.github.TheAnachronism.BrowserPicker.desktop
EOF
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/handler-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      mkdir -p "$TMPDIR/docs"
                      printf x > "$TMPDIR/docs/bootstrap.txt"
                      printf x > "$TMPDIR/docs/handler.html"
                      printf '<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"></html>' > "$TMPDIR/docs/handler.xhtml"
                      html="$TMPDIR/docs/handler.html"
                      xhtml="$TMPDIR/docs/handler.xhtml"
                      $BROWSER_PICKER/bin/browser-picker "$TMPDIR/docs/bootstrap.txt" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"

                      gapplication_open "['file://$html']"
                      gapplication_open "['file://$xhtml']"
                      python3 "$inspect" wait "3 Pending Requests"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"

                      http="https://automatic.example/from-http-handler"
                      https="https://automatic.example/from-https-handler"
                      gapplication_open "['$http']"
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$http"

                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      gapplication_open "['$https']"
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$https"

                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      python3 "$inspect" wait "3 Pending Requests"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      kill -0 "$launcher"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers ctrl+w
                      wait "$launcher"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_local_file_actions() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/file-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      mkdir -p "$TMPDIR/docs"
                      printf x > "$TMPDIR/docs/page.html"
                      file="$TMPDIR/docs/page.html"
                      uri="file://$file"
                      $BROWSER_PICKER/bin/browser-picker "$file" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      xdotool windowfocus --sync "$window"
                      xdotool type "work profile"
                      xdotool key Down
                      xdotool key ctrl+shift+p
                      xdotool key alt+2
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--private
$uri"
                      wait "$launcher"

                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      printf x > "$TMPDIR/docs/queued.html"
                      queued="$TMPDIR/docs/queued.html"
                      automatic="https://automatic.example/path"
                      $BROWSER_PICKER/bin/browser-picker "$queued" "$automatic" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic"
                      kill -0 "$launcher"
                      xdotool windowfocus --sync "$window"
                      xdotool key alt+2
                      for attempt in $(seq 1 100); do
                        actual_lines=$(wc -l < "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || printf 0)
                        test "$actual_lines" -lt 4 || break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic
--normal
file://$queued"
                      wait "$launcher"

                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      printf x > "$TMPDIR/docs/race.html"
                      raced="$TMPDIR/docs/race.html"
                      $BROWSER_PICKER/bin/browser-picker "$raced" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      rm -f "$raced"
                      xdotool windowfocus --sync "$window"
                      xdotool key alt+2
                      sleep 0.4
                      kill -0 "$launcher"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      xdotool key --clearmodifiers ctrl+w
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_first_run_and_picker() {
                      export XDG_CONFIG_HOME="$TMPDIR/first-run-config"
                      mkdir -p "$XDG_CONFIG_HOME"
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/first-run-argv"
                      target="https://example.com/first-run?token=kept-private"
                      $BROWSER_PICKER/bin/browser-picker "$target" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      test ! -f "$XDG_CONFIG_HOME/browser-picker/config.toml"
                      sleep 0.3
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers space
                      sleep 0.2
                      python3 "$inspect" activate "General"
                      python3 "$inspect" combo "Show Picker" "Open AAA Controlled Browser" --last
                      xdotool key --clearmodifiers alt+s
                      for attempt in $(seq 1 100); do
                        test ! -f "$XDG_CONFIG_HOME/browser-picker/config.toml" || break
                        sleep 0.1
                      done
                      grep -q "type = \"discovered\"" "$XDG_CONFIG_HOME/browser-picker/config.toml"
                      grep -q "aaa-controlled.desktop" "$XDG_CONFIG_HOME/browser-picker/config.toml"
                      grep -A2 "^\[fallback\]$" "$XDG_CONFIG_HOME/browser-picker/config.toml" | grep -q "action = \"open\""
                      test ! -f "$TMPDIR/first-run-argv"
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      xdotool windowfocus --sync "$window"
                      xdotool key alt+1
                      for attempt in $(seq 1 100); do
                        test ! -f "$TMPDIR/first-run-argv" || break
                        sleep 0.1
                      done
                      test "$(cat "$TMPDIR/first-run-argv")" = "$target"
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                      export XDG_CONFIG_HOME="$TMPDIR/config"
                    }

                    run_picker_and_assert_launch() {
                      target="https://xn--bcher-kva.example/path?token=kept-private"
                      $BROWSER_PICKER/bin/browser-picker "$target" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      xdotool windowfocus --sync "$window"

                      xdotool key alt+1
                      sleep 0.1
                      kill -0 "$launcher"

                      xdotool type "no result"
                      xdotool key ctrl+a BackSpace
                      xdotool type "work profile"
                      xdotool key Down
                      xdotool key ctrl+shift+p
                      xdotool key alt+2

                      for attempt in $(seq 1 100); do
                        test ! -f "$TMPDIR/received-argv" || break
                        sleep 0.1
                      done
                      test "$(cat "$TMPDIR/received-argv")" = "--private
$target"
                      wait "$launcher"
                    }

                    run_routing_actions() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/routing-actions-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      suggested="https://suggested.example/path"
                      $BROWSER_PICKER/bin/browser-picker "$suggested" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      xdotool windowfocus --sync "$window"
                      xdotool key Return
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--private
$suggested"
                      wait "$launcher"

                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      pending="https://pending.example/"
                      automatic="https://automatic.example/path"
                      $BROWSER_PICKER/bin/browser-picker "$pending" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      $BROWSER_PICKER/bin/browser-picker "$automatic"
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic"
                      kill -0 "$launcher"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers ctrl+w
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_automatic_failure_recovery_and_fifo() {
                      cat >> "$XDG_CONFIG_HOME/browser-picker/config.toml" <<'EOF'

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
EOF
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/automatic-recovery-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      failed="https://launch-failure.example/failed"
                      printf x > "$TMPDIR/docs/recovery-queued.html"
                      queued="$TMPDIR/docs/recovery-queued.html"
                      $BROWSER_PICKER/bin/browser-picker "$failed" "$queued" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      python3 "$inspect" wait "executable was not found"
                      python3 "$inspect" assert "unavailable" "executable was not found"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers alt+2
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$failed"
                      kill -0 "$launcher"
                      python3 "$inspect" wait "recovery-queued.html"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers alt+2
                      for attempt in $(seq 1 100); do
                        actual_lines=$(wc -l < "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || printf 0)
                        test "$actual_lines" -lt 4 || break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$failed
--normal
file://$queued"
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_private_failure_preserves_intent() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/private-recovery-argv"
                      cat >> "$XDG_CONFIG_HOME/browser-picker/config.toml" <<EOF

[[destinations]]
id = "firefox-work"
label = "Work Firefox"
profile_label = "Work"

[destinations.application]
type = "firefox-profile"
desktop_id = "firefox.desktop"
name = "Work"
path = "$TMPDIR/private-profile"

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
EOF
                      mkdir -p "$TMPDIR/snap/bin" "$TMPDIR/private-profile"
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
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      target="https://private-failure.example/private"
                      $BROWSER_PICKER/bin/browser-picker "$target" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      python3 "$inspect" wait "private Launch Mode is not available"
                      python3 "$inspect" assert "firefox-work" "private Launch Mode is not available"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers alt+2
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--private
$target"
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_activation_with_preselection_and_automatic() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/mixed-activation-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      suggested="https://suggested.example/queued"
                      automatic="https://automatic.example/immediate"
                      $BROWSER_PICKER/bin/browser-picker "$suggested" "$automatic" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic"
                      kill -0 "$launcher"
                      xdotool windowfocus --sync "$window"
                      xdotool key Return
                      for attempt in $(seq 1 100); do
                        actual_lines=$(wc -l < "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || printf 0)
                        test "$actual_lines" -lt 4 || break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic
--private
$suggested"
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_queue_and_assert_fifo() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/queue-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      first="https://first.example/"
                      second="https://second.example/path"
                      duplicate="https://first.example/"

                      $BROWSER_PICKER/bin/browser-picker "$first" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      xdotool windowfocus --sync "$window"

                      $BROWSER_PICKER/bin/browser-picker "$second" "$duplicate" &
                      secondary=$!
                      wait "$secondary"
                      kill -0 "$launcher"

                      for expected_lines in 2 4 6; do
                        xdotool key alt+2
                        for attempt in $(seq 1 100); do
                          actual_lines=$(wc -l < "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || printf 0)
                          test "$actual_lines" -lt "$expected_lines" || break
                          sleep 0.1
                        done
                      done

                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$first
--normal
$second
--normal
$duplicate"
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_queue_limit_and_escape() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/queue-limit-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      first="https://cancelled.example/"

                      $BROWSER_PICKER/bin/browser-picker "$first" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"

                      set --
                      for index in $(seq 1 100); do
                        set -- "$@" "https://queue-$index.example/"
                      done
                      if $BROWSER_PICKER/bin/browser-picker "$@" 2>"$TMPDIR/queue-limit-error"; then
                        false
                      else
                        test "$?" -eq 6
                      fi
                      grep -Fx "The Pending Request queue is full" "$TMPDIR/queue-limit-error"

                      xdotool windowfocus --sync "$window"
                      xdotool key Escape
                      kill -0 "$launcher"
                      test "$(xdotool getwindowname "$window")" = "Browser Picker"
                      for expected_lines in 2 4; do
                        xdotool key alt+2
                        for attempt in $(seq 1 100); do
                          actual_lines=$(wc -l < "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || printf 0)
                          test "$actual_lines" -lt "$expected_lines" || break
                          sleep 0.1
                        done
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
https://queue-1.example/
--normal
https://queue-2.example/"
                      xdotool key --clearmodifiers ctrl+w
                      wait "$launcher"
                      if grep -Fq "https://queue-3.example/" "$BROWSER_PICKER_TEST_OUTPUT"; then
                        echo "closing the Picker launched a cancelled Pending Request" >&2
                        exit 1
                      fi
                      if grep -Fq "https://queue-100.example/" "$BROWSER_PICKER_TEST_OUTPUT"; then
                        echo "overflow rejected request was launched" >&2
                        exit 1
                      fi
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_activation_limit() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/activation-limit-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      $BROWSER_PICKER/bin/browser-picker "https://activation-current.example/" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"

                      set --
                      for index in $(seq 1 101); do
                        set -- "$@" "https://activation-$index.example/"
                      done
                      if $BROWSER_PICKER/bin/browser-picker "$@" 2>"$TMPDIR/activation-limit-error"; then
                        false
                      else
                        test "$?" -eq 6
                      fi
                      grep -Fx "One activation accepts at most 100 Open Targets" "$TMPDIR/activation-limit-error"
                      xdotool windowfocus --sync "$window"
                      xdotool key Escape
                      for expected_lines in 2 4; do
                        xdotool key alt+2
                        for attempt in $(seq 1 100); do
                          actual_lines=$(wc -l < "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || printf 0)
                          test "$actual_lines" -lt "$expected_lines" || break
                          sleep 0.1
                        done
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
https://activation-1.example/
--normal
https://activation-2.example/"
                      xdotool key --clearmodifiers ctrl+w
                      wait "$launcher"
                      if grep -Fq "https://activation-3.example/" "$BROWSER_PICKER_TEST_OUTPUT"; then
                        echo "closing the Picker launched a cancelled Pending Request" >&2
                        exit 1
                      fi
                      if grep -Fq "https://activation-101.example/" "$BROWSER_PICKER_TEST_OUTPUT"; then
                        echo "activation overflow rejected request was launched" >&2
                        exit 1
                      fi
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_paused_configuration_and_live_routing() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/paused-live-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      $BROWSER_PICKER/bin/browser-picker "https://pending.example/queue" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      $BROWSER_PICKER/bin/browser-picker
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -ge 2 ]; then
                          break
                        fi
                        sleep 0.1
                      done
                      set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                      test "$#" -ge 2
                      python3 "$inspect" wait "Configuration"
                      config_window=
                      for candidate in "$@"; do
                        if [ "$candidate" != "$window" ]; then
                          config_window=$candidate
                          break
                        fi
                      done
                      test -n "$config_window"
                      xdotool windowfocus --sync "$config_window"
                      xdotool key --clearmodifiers ctrl+w
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -eq 1 ]; then
                          break
                        fi
                        sleep 0.1
                      done
                      set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                      test "$#" -eq 1
                      $BROWSER_PICKER/bin/browser-picker config
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -ge 2 ]; then
                          break
                        fi
                        sleep 0.1
                      done
                      set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                      test "$#" -ge 2
                      config_window=
                      for candidate in "$@"; do
                        if [ "$candidate" != "$window" ]; then
                          config_window=$candidate
                          break
                        fi
                      done
                      test -n "$config_window"
                      xdotool windowfocus --sync "$window" || true
                      $BROWSER_PICKER/bin/browser-picker config
                      test "$(xdotool getwindowfocus)" = "$config_window"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      $BROWSER_PICKER/bin/browser-picker "https://automatic.example/live"
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
https://automatic.example/live"
                      kill -0 "$launcher"
                      xdotool windowfocus --sync "$window" || true
                      xdotool key --clearmodifiers alt+2
                      sleep 0.4
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
https://automatic.example/live"
                      grep -q "# Browser Picker configuration" "$XDG_CONFIG_HOME/browser-picker/config.toml"
                      for round in $(seq 1 8); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -eq 0 ]; then
                          break
                        fi
                        for candidate in "$@"; do
                          xdotool windowfocus --sync "$candidate" || true
                          xdotool key --clearmodifiers ctrl+w
                        done
                        sleep 0.15
                      done
                      set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                      test "$#" -eq 0
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_save_and_apply_keeps_pending() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/save-and-apply-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      pending="https://pending.example/queue"
                      $BROWSER_PICKER/bin/browser-picker "$pending" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers alt+e
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -ge 2 ]; then
                          break
                        fi
                        sleep 0.1
                      done
                      set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                      test "$#" -ge 2
                      python3 "$inspect" activate "General"
                      python3 "$inspect" combo "Show Picker" "Open Work Browser" --last
                      xdotool key --clearmodifiers alt+s
                      for attempt in $(seq 1 100); do
                        grep -A3 "^\[fallback\]$" "$XDG_CONFIG_HOME/browser-picker/config.toml" | grep -q "destination = \"controlled\"" && break
                        sleep 0.1
                      done
                      grep -A3 "^\[fallback\]$" "$XDG_CONFIG_HOME/browser-picker/config.toml" | grep -q "destination = \"controlled\""
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -eq 1 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      xdotool windowfocus --sync "$window"
                      xdotool key Return
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$pending"
                      wait "$launcher"

                      next="https://new-arrival.example/after-save"
                      $BROWSER_PICKER/bin/browser-picker "$next"
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$pending
--normal
$next"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_invalid_configuration_recovery() {
                      export XDG_CONFIG_HOME="$TMPDIR/invalid-config"
                      mkdir -p "$XDG_CONFIG_HOME/browser-picker"
                      cat > "$XDG_CONFIG_HOME/browser-picker/config.toml" <<EOF
version = 1
this is not [[toml
secret = "https://user:pass@example.com/?q=1"
EOF
                      original=$(cat "$XDG_CONFIG_HOME/browser-picker/config.toml")
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/invalid-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      target="https://example.com/recover?token=kept-private"
                      $BROWSER_PICKER/bin/browser-picker "$target" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      test "$(cat "$XDG_CONFIG_HOME/browser-picker/config.toml")" = "$original"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers alt+1
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      grep -F "$target" "$BROWSER_PICKER_TEST_OUTPUT"
                      test "$(cat "$XDG_CONFIG_HOME/browser-picker/config.toml")" = "$original"
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                      export XDG_CONFIG_HOME="$TMPDIR/config"
                    }

                    run_old_schema_migration_keeps_target_pending() {
                      export XDG_CONFIG_HOME="$TMPDIR/migrate-config"
                      mkdir -p "$XDG_CONFIG_HOME/browser-picker"
                      sed "s/version = 1/version = 0/" "$TMPDIR/config/browser-picker/config.toml" > "$XDG_CONFIG_HOME/browser-picker/config.toml"
                      original=$(cat "$XDG_CONFIG_HOME/browser-picker/config.toml")
                      grep -q "version = 0" "$XDG_CONFIG_HOME/browser-picker/config.toml"
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/migrate-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      target="https://automatic.example/migrated?token=kept-private"
                      $BROWSER_PICKER/bin/browser-picker "$target" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      test "$(cat "$XDG_CONFIG_HOME/browser-picker/config.toml")" = "$original"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers alt+m
                      for attempt in $(seq 1 100); do
                        grep -q "version = 1" "$XDG_CONFIG_HOME/browser-picker/config.toml" && break
                        sleep 0.1
                      done
                      grep -q "version = 1" "$XDG_CONFIG_HOME/browser-picker/config.toml"
                      if grep -q "version = 0" "$XDG_CONFIG_HOME/browser-picker/config.toml"; then exit 1; fi
                      test ! -f "$BROWSER_PICKER_TEST_OUTPUT"
                      set -- "$XDG_CONFIG_HOME/browser-picker"/config.toml.bak-*
                      test -f "$1"
                      test "$(stat -c %a "$1")" = "600"
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers alt+2
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$target"
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                      export XDG_CONFIG_HOME="$TMPDIR/config"
                    }

                    run_manual_destination_configuration() {
                      export XDG_CONFIG_HOME="$TMPDIR/manual-config"
                      mkdir -p "$XDG_CONFIG_HOME"
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/manual-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      cat > "$TMPDIR/graphical-browser" <<'EOF'
#!/bin/sh
printf "%s\n" "$@" > "$BROWSER_PICKER_TEST_OUTPUT"
EOF
                      chmod 700 "$TMPDIR/graphical-browser"

                      $BROWSER_PICKER/bin/browser-picker config &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      python3 "$inspect" focus "Add Manual Browser Application"
                      xdotool key --clearmodifiers space
                      sleep 0.2
                      python3 "$inspect" assert \
                        "Browser Application label" \
                        "Manual executable" \
                        "Normal literal arguments" \
                        "Private literal arguments"
                      xdotool key --clearmodifiers ctrl+a
                      xdotool type "graphical-manual"
                      xdotool key Tab
                      xdotool key --clearmodifiers ctrl+a
                      xdotool type "Graphical Manual Browser"
                      xdotool key Tab
                      xdotool key --clearmodifiers ctrl+a
                      xdotool type "applications-internet"
                      xdotool key Tab
                      xdotool key --clearmodifiers ctrl+a
                      xdotool type "Graphical Browser Application"
                      xdotool key Tab
                      xdotool key --clearmodifiers ctrl+a
                      xdotool type "$TMPDIR/graphical-browser"
                      xdotool key Tab
                      xdotool key --clearmodifiers ctrl+a
                      xdotool type -- "--graphical"
                      xdotool key Return
                      xdotool type -- "{target}"
                      xdotool key Tab
                      xdotool type -- "--private"
                      xdotool key Return
                      xdotool type -- "{target}"
                      python3 "$inspect" activate "General"
                      python3 "$inspect" combo "Show Picker" "Open Graphical Manual Browser" --last
                      xdotool key --clearmodifiers alt+s

                      for attempt in $(seq 1 100); do
                        test -f "$XDG_CONFIG_HOME/browser-picker/config.toml" && break
                        sleep 0.1
                      done
                      config="$XDG_CONFIG_HOME/browser-picker/config.toml"
                      grep -q "id = \"graphical-manual\"" "$config"
                      grep -q "label = \"Graphical Manual Browser\"" "$config"
                      grep -q "label = \"Graphical Browser Application\"" "$config"
                      grep -q "icon = \"applications-internet\"" "$config"
                      grep -q "args = \[" "$config"
                      grep -q "\"--graphical\"" "$config"
                      grep -q "private_args = \[" "$config"
                      grep -q "\"--private\"" "$config"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers ctrl+w
                      wait "$launcher"

                      target="https://manual.example/reloaded"
                      $BROWSER_PICKER/bin/browser-picker "$target"
                      expected="--graphical
$target"
                      for attempt in $(seq 1 100); do
                        test "$(cat "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || true)" = "$expected" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "$expected"
                      unset BROWSER_PICKER_TEST_OUTPUT
                      export XDG_CONFIG_HOME="$TMPDIR/config"
                    }

                    run_reference_safe_configuration() {
                      export XDG_CONFIG_HOME="$TMPDIR/reference-safe-config"
                      mkdir -p "$XDG_CONFIG_HOME/browser-picker"
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/reference-safe-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
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

[[destinations]]
id = "spare"
label = "Spare Browser"

[destinations.application]
type = "manual"
label = "Spare Browser Application"
executable = "$TMPDIR/controlled-browser"
args = ["--spare", "{target}"]

[[rules]]
id = "later"
name = "Later"
enabled = true

[rules.action]
type = "open"
destination = "spare"
mode = "normal"

[[rules.groups]]

[[rules.groups.conditions]]
type = "host"
value = "later.example"

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
value = "other.example"

[[rules.groups]]

[[rules.groups.conditions]]
type = "scheme"
value = "https"

[[rules.groups.conditions]]
type = "host"
value = "rename.example"

[fallback]
action = "open"
destination = "work"
mode = "normal"
EOF
                      config="$XDG_CONFIG_HOME/browser-picker/config.toml"

                      $BROWSER_PICKER/bin/browser-picker config &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      echo "reference-safe: window ready" >&2
                      python3 "$inspect" activate "Rules"
                      python3 "$inspect" activate "Later"
                      python3 "$inspect" wait "Remove OR group"
                      python3 "$inspect" assert \
                        "Remove Routing Rule" \
                        "Remove OR group" \
                        "Remove AND condition"
                      python3 "$inspect" activate "Destinations"
                      python3 "$inspect" disabled "Enable Browser Candidate Work Browser"
                      python3 "$inspect" checked "Enable Browser Candidate Work Browser"
                      python3 "$inspect" focus "Browser Destination ID"
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers ctrl+a
                      xdotool key --clearmodifiers BackSpace
                      xdotool type "renamed-work"
                      xdotool key --clearmodifiers alt+s
                      for attempt in $(seq 1 100); do
                        grep -q "id = \"renamed-work\"" "$config" && break
                        sleep 0.1
                      done
                      grep -q "id = \"renamed-work\"" "$config" || { echo "missing renamed destination id"; cat "$config"; exit 1; }
                      grep -q "destination = \"renamed-work\"" "$config" || { echo "missing renamed destination reference"; cat "$config"; exit 1; }
                      grep -A3 "^\[fallback\]$" "$config" | grep -q "destination = \"renamed-work\"" || { echo "fallback was not rewritten"; cat "$config"; exit 1; }
                      grep -q "id = \"work\"" "$config" && { echo "old destination id remained"; cat "$config"; exit 1; } || true
                      echo "reference-safe: renamed" >&2
                      python3 "$inspect" activate "General"
                      python3 "$inspect" combo "Open Work Browser" "Show Picker" --last
                      sleep 0.2
                      python3 "$inspect" activate "Destinations"
                      python3 "$inspect" disabled "Enable Browser Candidate Work Browser"
                      python3 "$inspect" activate "Rules"
                      python3 "$inspect" activate "Later"
                      python3 "$inspect" wait "Routing Rule ID"
                      python3 "$inspect" pointer "Routing Rule ID"
                      python3 "$inspect" activate "Remove Routing Rule"
                      sleep 0.2
                      python3 "$inspect" pointer "Action Browser Destination ID"
                      xdotool key --clearmodifiers ctrl+a
                      xdotool type "spare"
                      python3 "$inspect" activate "Destinations"
                      python3 "$inspect" enabled "Enable Browser Candidate Work Browser"
                      python3 "$inspect" checked "Enable Browser Candidate Work Browser"
                      python3 "$inspect" activate "Rules"
                      python3 "$inspect" activate "Automatic"
                      python3 "$inspect" wait "Remove OR group"
                      python3 "$inspect" activate "Remove OR group"
                      sleep 0.2
                      python3 "$inspect" activate "Remove AND condition"
                      sleep 0.2
                      xdotool key --clearmodifiers alt+s
                      for attempt in $(seq 1 100); do
                        grep -q "action = \"show-picker\"" "$config" || continue
                        grep -q "value = \"rename.example\"" "$config" && break
                        sleep 0.1
                      done
                      echo "reference-safe: second save" >&2
                      grep -q "id = \"renamed-work\"" "$config" || { echo "missing renamed destination"; cat "$config"; exit 1; }
                      grep -q "destination = \"renamed-work\"" "$config" && { echo "renamed destination still referenced"; cat "$config"; exit 1; } || true
                      grep -q "id = \"later\"" "$config" && { echo "later rule was not removed"; cat "$config"; exit 1; } || true
                      grep -q "value = \"other.example\"" "$config" && { echo "other.example OR group remained"; cat "$config"; exit 1; } || true
                      grep -q "value = \"https\"" "$config" && { echo "https AND condition remained"; cat "$config"; exit 1; } || true
                      grep -q "id = \"auto\"" "$config" || { echo "missing auto rule"; cat "$config"; exit 1; }
                      grep -q "id = \"spare\"" "$config" || { echo "missing spare destination"; cat "$config"; exit 1; }
                      grep -q "destination = \"spare\"" "$config" || { echo "missing spare destination reference"; cat "$config"; exit 1; }
                      grep -q "value = \"rename.example\"" "$config" || { echo "missing rename host"; cat "$config"; exit 1; }
                      grep -A2 "^\[fallback\]$" "$config" | grep -q "action = \"show-picker\"" || { echo "fallback was not show-picker"; cat "$config"; exit 1; }
                      cp "$config" "$TMPDIR/reference-safe-saved.toml"
                      python3 "$inspect" activate "Remove AND condition"
                      python3 "$inspect" wait "condition groups must not be empty"
                      xdotool key --clearmodifiers alt+s
                      sleep 0.3
                      cmp "$config" "$TMPDIR/reference-safe-saved.toml" || { echo "invalid draft overwrote saved configuration"; cat "$config"; exit 1; }
                      xdotool windowfocus --sync "$window"
                      xdotool key --clearmodifiers ctrl+w
                      python3 "$inspect" activate "Discard"
                      wait "$launcher"

                      target="https://rename.example/reloaded"
                      $BROWSER_PICKER/bin/browser-picker "$target"
                      expected="--spare
$target"
                      for attempt in $(seq 1 100); do
                        test "$(cat "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || true)" = "$expected" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "$expected"
                      unset BROWSER_PICKER_TEST_OUTPUT
                      export XDG_CONFIG_HOME="$TMPDIR/config"
                    }

                    assert_open_targets_not_persisted() {
                      python3 - "$XDG_STATE_HOME" "$XDG_CACHE_HOME" "$XDG_CONFIG_HOME" "$HOME" "$@" <<'PY'
from pathlib import Path
import sys

roots = [Path(arg) for arg in sys.argv[1:5]]
needles = sys.argv[5:]
leaks = []
for root in roots:
    if not root.exists():
        continue
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        text = path.read_text(errors="replace")
        for needle in needles:
            if needle in text:
                leaks.append(f"{path}: {needle}")
if leaks:
    raise SystemExit("Pending Requests persisted to XDG state:\n" + "\n".join(leaks))
print("no Pending Request Open Targets persisted")
PY
                    }

                    run_concurrent_secondaries_fifo_duplicates_and_bypass() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/session-concurrent-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      first="https://session-first.example/"
                      second="https://session-second.example/path"
                      duplicate="https://session-first.example/"
                      overlap_a="https://session-overlap-a.example/"
                      overlap_b="https://session-overlap-b.example/"
                      automatic="https://automatic.example/open-bypass"

                      $BROWSER_PICKER/bin/browser-picker "$first" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      xdotool windowfocus --sync "$window"

                      $BROWSER_PICKER/bin/browser-picker "$automatic"
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic"
                      kill -0 "$launcher"

                      $BROWSER_PICKER/bin/browser-picker "$second" &
                      secondary_one=$!
                      wait "$secondary_one"
                      $BROWSER_PICKER/bin/browser-picker "$duplicate" &
                      secondary_two=$!
                      wait "$secondary_two"
                      $BROWSER_PICKER/bin/browser-picker "$overlap_a" &
                      overlap_one=$!
                      $BROWSER_PICKER/bin/browser-picker "$overlap_b" &
                      overlap_two=$!
                      wait "$overlap_one"
                      wait "$overlap_two"
                      kill -0 "$launcher"

                      xdotool windowfocus --sync "$window"
                      for expected_lines in 4 6 8 10 12; do
                        xdotool key alt+2
                        for attempt in $(seq 1 100); do
                          actual_lines=$(wc -l < "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || printf 0)
                          test "$actual_lines" -lt "$expected_lines" || break
                          sleep 0.1
                        done
                      done

                      python3 - "$BROWSER_PICKER_TEST_OUTPUT" "$first" "$second" "$duplicate" "$overlap_a" "$overlap_b" "$automatic" <<'PY'
from pathlib import Path
import sys

path, first, second, duplicate, overlap_a, overlap_b, automatic = sys.argv[1:]
pairs = list(zip(*[iter(Path(path).read_text().splitlines())] * 2))
if pairs[0] != ("--normal", automatic):
    raise SystemExit(f"automatic arrival did not bypass the open Picker: {pairs!r}")
if pairs[1] != ("--normal", first):
    raise SystemExit(f"primary FIFO did not start with the first Pending Request: {pairs!r}")
if pairs[2] != ("--normal", second):
    raise SystemExit(f"accepted secondary did not follow in FIFO order: {pairs!r}")
if pairs[3] != ("--normal", duplicate):
    raise SystemExit(f"duplicate Open Target was not preserved: {pairs!r}")
if {target for _mode, target in pairs[4:]} != {overlap_a, overlap_b}:
    raise SystemExit(f"overlapping secondaries were not both queued: {pairs!r}")
if len(pairs) != 6:
    raise SystemExit(f"unexpected launches: {pairs!r}")
print("concurrent secondaries preserved FIFO, duplicates, and automatic bypass")
PY
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_paused_choice_join_and_resume() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/paused-join-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      first="https://pending.example/first"
                      second="https://pending.example/second"
                      third="https://pending.example/third"
                      automatic="https://automatic.example/paused-bypass"

                      $BROWSER_PICKER/bin/browser-picker "$first" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"

                      $BROWSER_PICKER/bin/browser-picker
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -ge 2 ]; then
                          break
                        fi
                        sleep 0.1
                      done
                      set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                      test "$#" -ge 2
                      python3 "$inspect" wait "Configuration"
                      config_window=
                      for candidate in "$@"; do
                        if [ "$candidate" != "$window" ]; then
                          config_window=$candidate
                          break
                        fi
                      done
                      test -n "$config_window"

                      $BROWSER_PICKER/bin/browser-picker "$second"
                      $BROWSER_PICKER/bin/browser-picker "$third" "$second"
                      $BROWSER_PICKER/bin/browser-picker "$automatic"
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic"
                      kill -0 "$launcher"
                      xdotool windowfocus --sync "$window" || true
                      xdotool key --clearmodifiers alt+2
                      sleep 0.4
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic"

                      xdotool windowfocus --sync "$config_window"
                      xdotool key --clearmodifiers ctrl+w
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -eq 1 ]; then
                          break
                        fi
                        sleep 0.1
                      done
                      set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                      test "$#" -eq 1
                      window=$1
                      xdotool windowfocus --sync "$window"
                      python3 "$inspect" wait "pending.example"

                      for expected_lines in 4 6 8 10; do
                        xdotool key alt+2
                        for attempt in $(seq 1 100); do
                          actual_lines=$(wc -l < "$BROWSER_PICKER_TEST_OUTPUT" 2>/dev/null || printf 0)
                          test "$actual_lines" -lt "$expected_lines" || break
                          sleep 0.1
                        done
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$automatic
--normal
$first
--normal
$second
--normal
$third
--normal
$second"
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_primary_restart_drops_queue() {
                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/restart-argv"
                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                      before="https://queued-before.example/pending"
                      also="https://queued-also.example/pending"
                      after="https://queued-after.example/fresh"

                      $BROWSER_PICKER/bin/browser-picker "$before" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      $BROWSER_PICKER/bin/browser-picker "$also"
                      python3 "$inspect" wait "queued-before.example"
                      assert_open_targets_not_persisted \
                        "queued-before.example" "queued-also.example" \
                        "$before" "$also"

                      kill "$launcher" || true
                      for attempt in $(seq 1 50); do
                        kill -0 "$launcher" 2>/dev/null || break
                        sleep 0.1
                      done
                      kill -9 "$launcher" 2>/dev/null || true
                      wait "$launcher" || true
                      for attempt in $(seq 1 50); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -eq 0 ]; then
                          break
                        fi
                        sleep 0.1
                      done
                      set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                      test "$#" -eq 0
                      assert_open_targets_not_persisted \
                        "queued-before.example" "queued-also.example" \
                        "$before" "$also"

                      $BROWSER_PICKER/bin/browser-picker "$after" &
                      launcher=$!
                      window=
                      for attempt in $(seq 1 100); do
                        set -- $(xdotool search --onlyvisible --name "^Browser Picker$" 2>/dev/null || true)
                        if [ "$#" -gt 0 ]; then
                          window=$1
                          break
                        fi
                        sleep 0.1
                      done
                      test -n "$window"
                      python3 "$inspect" wait "queued-after.example"
                      dump=$(python3 "$inspect" dump)
                      if printf '%s\n' "$dump" | grep -Fq "queued-before.example"; then
                        echo "restart restored a Pending Request from disk" >&2
                        printf '%s\n' "$dump" >&2
                        exit 1
                      fi
                      if printf '%s\n' "$dump" | grep -Fq "queued-also.example"; then
                        echo "restart restored a Pending Request from disk" >&2
                        printf '%s\n' "$dump" >&2
                        exit 1
                      fi
                      xdotool windowfocus --sync "$window"
                      xdotool key alt+2
                      for attempt in $(seq 1 100); do
                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                        sleep 0.1
                      done
                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
$after"
                      if grep -Fq "queued-before.example" "$BROWSER_PICKER_TEST_OUTPUT"; then
                        echo "restart launched a restored Pending Request" >&2
                        exit 1
                      fi
                      wait "$launcher"
                      unset BROWSER_PICKER_TEST_OUTPUT
                    }

                    run_first_run_and_picker
                    run_manual_destination_configuration
                    run_reference_safe_configuration
                    run_and_assert_window $BROWSER_PICKER/bin/browser-picker
                    run_and_assert_window gtk-launch io.github.TheAnachronism.BrowserPicker
                    run_installed_handler_activations
                    run_picker_and_assert_launch
                    run_routing_actions
                    run_activation_with_preselection_and_automatic
                    run_local_file_actions
                    run_queue_and_assert_fifo
                    run_queue_limit_and_escape
                    run_activation_limit
                    run_paused_configuration_and_live_routing
                    run_concurrent_secondaries_fifo_duplicates_and_bypass
                    run_paused_choice_join_and_resume
                    run_primary_restart_drops_queue
                    run_save_and_apply_keeps_pending
                    run_invalid_configuration_recovery
                    run_old_schema_migration_keeps_target_pending
                    run_automatic_failure_recovery_and_fifo
                    run_private_failure_preserves_intent
