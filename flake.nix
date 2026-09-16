{
  description = "Browser Picker";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems =
        function: nixpkgs.lib.genAttrs systems (system: function (import nixpkgs { inherit system; }));
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          browser-picker = pkgs.callPackage ./nix/package.nix { inherit systems; };
        in
        {
          default = browser-picker;
          inherit browser-picker;
        }
      );

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/browser-picker";
          meta.description = "Open Browser Picker configuration";
        };
      });

      checks = forAllSystems (
        pkgs:
        let
          browser-picker = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
        in
        {
          package = browser-picker;
          gui-smoke =
            pkgs.runCommand "browser-picker-gui-smoke"
              {
                nativeBuildInputs = with pkgs; [
                  browser-picker
                  dbus
                  glib
                  gtk3
                  xdotool
                  xvfb-run
                ];
              }
              ''
                                export HOME="$TMPDIR/home"
                                export LANG=C.UTF-8
                                export LC_ALL=C.UTF-8
                                export XDG_RUNTIME_DIR="$TMPDIR/runtime"
                                export XDG_DATA_HOME="$TMPDIR/xdg-data"
                                export XDG_DATA_DIRS="${browser-picker}/share''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
                                mkdir -p "$HOME" "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME/applications"
                                chmod 700 "$XDG_RUNTIME_DIR"

                                cat > "$TMPDIR/controlled-browser" <<'EOF'
                #!/bin/sh
                printf '%s\n' "$@" >> "''${BROWSER_PICKER_TEST_OUTPUT:-$TMPDIR/received-argv}"
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

                                dbus-run-session \
                                  --config-file=${pkgs.dbus}/share/dbus-1/session.conf \
                                  -- \
                                  xvfb-run -a -s '-screen 0 1024x768x24' \
                                  sh -eu -c '
                                    export ADW_DISABLE_PORTAL=1
                                    export GDK_BACKEND=x11
                                    export GSK_RENDERER=cairo
                                    export GSETTINGS_BACKEND=memory

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

                                    run_first_run_and_picker() {
                                      export XDG_CONFIG_HOME="$TMPDIR/first-run-config"
                                      mkdir -p "$XDG_CONFIG_HOME"
                                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/first-run-argv"
                                      target="https://example.com/first-run?token=kept-private"
                                      ${browser-picker}/bin/browser-picker "$target" &
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
                                      xdotool key --clearmodifiers alt+s
                                      for attempt in $(seq 1 100); do
                                        test ! -f "$XDG_CONFIG_HOME/browser-picker/config.toml" || break
                                        sleep 0.1
                                      done
                                      grep -q "type = \"discovered\"" "$XDG_CONFIG_HOME/browser-picker/config.toml"
                                      grep -q "aaa-controlled.desktop" "$XDG_CONFIG_HOME/browser-picker/config.toml"
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
                                      ${browser-picker}/bin/browser-picker "$target" &
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
                                      ${browser-picker}/bin/browser-picker "$suggested" &
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
                                      ${browser-picker}/bin/browser-picker "$pending" &
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
                                      ${browser-picker}/bin/browser-picker "$automatic"
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

                                    run_activation_with_preselection_and_automatic() {
                                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/mixed-activation-argv"
                                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                                      suggested="https://suggested.example/queued"
                                      automatic="https://automatic.example/immediate"
                                      ${browser-picker}/bin/browser-picker "$suggested" "$automatic" &
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

                                      ${browser-picker}/bin/browser-picker "$first" &
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

                                      ${browser-picker}/bin/browser-picker "$second" "$duplicate"
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

                                      ${browser-picker}/bin/browser-picker "$first" &
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
                                      if ${browser-picker}/bin/browser-picker "$@" 2>"$TMPDIR/queue-limit-error"; then
                                        false
                                      else
                                        test "$?" -eq 6
                                      fi
                                      grep -Fx "The Pending Request queue is full" "$TMPDIR/queue-limit-error"

                                      xdotool windowfocus --sync "$window"
                                      xdotool key Escape
                                      kill -0 "$launcher"
                                      test "$(xdotool getwindowname "$window")" = "Browser Picker"
                                      xdotool key alt+2
                                      for attempt in $(seq 1 100); do
                                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                                        sleep 0.1
                                      done
                                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
                https://queue-1.example/"
                                      xdotool key --clearmodifiers ctrl+w
                                      wait "$launcher"
                                      unset BROWSER_PICKER_TEST_OUTPUT
                                    }

                                    run_activation_limit() {
                                      export BROWSER_PICKER_TEST_OUTPUT="$TMPDIR/activation-limit-argv"
                                      rm -f "$BROWSER_PICKER_TEST_OUTPUT"
                                      ${browser-picker}/bin/browser-picker "https://activation-current.example/" &
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
                                      if ${browser-picker}/bin/browser-picker "$@" 2>"$TMPDIR/activation-limit-error"; then
                                        false
                                      else
                                        test "$?" -eq 6
                                      fi
                                      grep -Fx "One activation accepts at most 100 Open Targets" "$TMPDIR/activation-limit-error"
                                      xdotool windowfocus --sync "$window"
                                      xdotool key Escape
                                      xdotool key alt+2
                                      for attempt in $(seq 1 100); do
                                        test -f "$BROWSER_PICKER_TEST_OUTPUT" && break
                                        sleep 0.1
                                      done
                                      test "$(cat "$BROWSER_PICKER_TEST_OUTPUT")" = "--normal
                https://activation-1.example/"
                                      xdotool key --clearmodifiers ctrl+w
                                      wait "$launcher"
                                      unset BROWSER_PICKER_TEST_OUTPUT
                                    }

                                    run_first_run_and_picker
                                    run_and_assert_window ${browser-picker}/bin/browser-picker
                                    run_and_assert_window gtk-launch io.github.TheAnachronism.BrowserPicker
                                    run_picker_and_assert_launch
                                    run_routing_actions
                                    run_activation_with_preselection_and_automatic
                                    run_queue_and_assert_fifo
                                    run_queue_limit_and_escape
                                    run_activation_limit
                                  '

                                touch "$out"
              '';
        }
      );

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
          packages = with pkgs; [
            cargo
            clippy
            dbus
            rustc
            rustfmt
            xdotool
            xvfb-run
          ];
          BROWSER_PICKER_LOCALEDIR = "${toString ./.}/.local/share/locale";
        };
      });
    };
}
