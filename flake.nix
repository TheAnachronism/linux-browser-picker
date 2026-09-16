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
          browser-picker = pkgs.rustPlatform.buildRustPackage {
            pname = "browser-picker";
            version = "0.1.0";
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter =
                path: type:
                let
                  name = builtins.baseNameOf path;
                in
                name != ".git" && name != ".envrc" && name != "target";
            };
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = with pkgs; [
              gettext
              pkg-config
              wrapGAppsHook4
            ];
            buildInputs = with pkgs; [
              gtk4
              libadwaita
            ];
            nativeCheckInputs = with pkgs; [
              appstream
              desktop-file-utils
            ];
            postCheck = ''
              desktop-file-validate data/io.github.TheAnachronism.BrowserPicker.desktop
              appstreamcli validate --no-net data/io.github.TheAnachronism.BrowserPicker.metainfo.xml
              msgfmt --check po/en.po -o /dev/null
            '';
            BROWSER_PICKER_LOCALEDIR = "${placeholder "out"}/share/locale";
            postInstall = ''
              mkdir -p "$out/share/applications" "$out/share/metainfo"
              msgfmt --desktop \
                --template=data/io.github.TheAnachronism.BrowserPicker.desktop \
                --locale=en \
                po/en.po \
                --output-file="$out/share/applications/io.github.TheAnachronism.BrowserPicker.desktop"
              install -Dm644 data/io.github.TheAnachronism.BrowserPicker.svg \
                "$out/share/icons/hicolor/scalable/apps/io.github.TheAnachronism.BrowserPicker.svg"
              msgfmt --xml \
                --template=data/io.github.TheAnachronism.BrowserPicker.metainfo.xml \
                --locale=en \
                po/en.po \
                --output-file="$out/share/metainfo/io.github.TheAnachronism.BrowserPicker.metainfo.xml"
              install -Dm644 po/browser-picker.pot \
                "$out/share/browser-picker/translations/browser-picker.pot"
              install -Dm644 LICENSE "$out/share/licenses/browser-picker/LICENSE"
              mkdir -p "$out/share/locale/en/LC_MESSAGES"
              msgfmt po/en.po -o "$out/share/locale/en/LC_MESSAGES/browser-picker.mo"
            '';
            meta = {
              description = "Choose where links and local files open";
              homepage = "https://github.com/TheAnachronism/linux-browser-picker";
              license = pkgs.lib.licenses.gpl3Plus;
              mainProgram = "browser-picker";
              platforms = systems;
            };
          };
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
                export XDG_DATA_DIRS="${browser-picker}/share''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
                mkdir -p "$HOME" "$XDG_RUNTIME_DIR"
                chmod 700 "$XDG_RUNTIME_DIR"

                export XDG_CONFIG_HOME="$TMPDIR/config"
                mkdir -p "$XDG_CONFIG_HOME/browser-picker"
                cat > "$TMPDIR/controlled-browser" <<'EOF'
#!/bin/sh
printf '%s\n' "$@" > "$TMPDIR/received-argv"
EOF
                chmod 700 "$TMPDIR/controlled-browser"
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

                    run_and_assert_window ${browser-picker}/bin/browser-picker
                    run_and_assert_window gtk-launch io.github.TheAnachronism.BrowserPicker
                    run_picker_and_assert_launch
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
