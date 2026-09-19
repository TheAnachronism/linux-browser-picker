{
  description = "Browser Picker";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.home-manager.url = "github:nix-community/home-manager";
  inputs.home-manager.inputs.nixpkgs.follows = "nixpkgs";

  outputs =
    {
      self,
      nixpkgs,
      home-manager,
    }:
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

      homeManagerModules = {
        default =
          { pkgs, lib, ... }:
          {
            imports = [ ./nix/home-manager.nix ];
            config.programs.browser-picker.package =
              lib.mkDefault
                self.packages.${pkgs.stdenv.hostPlatform.system}.default;
          };
        browser-picker = self.homeManagerModules.default;
      };

      checks = forAllSystems (
        pkgs:
        let
          system = pkgs.stdenv.hostPlatform.system;
          browser-picker = self.packages.${system}.default;
          desktopId = "io.github.TheAnachronism.BrowserPicker.desktop";
        in
        {
          package = browser-picker;
          home-manager-associations = pkgs.callPackage ./nix/checks/home-manager-associations.nix {
            inherit browser-picker desktopId;
            homeManagerConfiguration = home-manager.lib.homeManagerConfiguration;
            homeManagerModule = self.homeManagerModules.default;
          };
          xdg-associations = pkgs.callPackage ./nix/checks/xdg-associations.nix {
            inherit browser-picker desktopId;
          };
          aarch64-defined =
            let
              aarch64Package = self.packages.aarch64-linux.browser-picker;
              # Evaluate the aarch64 derivation without depending on its output.
              drvPath = builtins.unsafeDiscardStringContext aarch64Package.drvPath;
            in
            assert builtins.elem "aarch64-linux" systems;
            assert aarch64Package.system == "aarch64-linux";
            assert aarch64Package.name == browser-picker.name;
            assert builtins.isString aarch64Package.drvPath;
            assert drvPath != "";
            pkgs.writeText "browser-picker-aarch64-defined" ''
              ${drvPath}
              aarch64-linux package derivation evaluated; runtime desktop support remains unclaimed
            '';
          app =
            assert self.apps.${system} ? default;
            assert self.apps.${system}.default.type == "app";
            pkgs.runCommand "browser-picker-app"
              {
                app = self.apps.${system}.default.program;
              }
              ''
                output="$("$app" version)"
                echo "$output" | grep -F "Browser Picker ${browser-picker.version}"
                echo "app output executed Browser Picker through the public flake app" > "$out"
              '';
          development-shell = self.devShells.${system}.default.overrideAttrs (_: {
            name = "browser-picker-development-shell";
            phases = [
              "buildPhase"
              "installPhase"
            ];
            buildPhase = ''
              command -v rustc
              command -v cargo
              rustc --version
              cargo --version
            '';
            installPhase = ''
              mkdir "$out"
              rustc --version > "$out/rustc-version"
              cargo --version > "$out/cargo-version"
            '';
          });
          release-proof = pkgs.callPackage ./nix/release-proof.nix {
            inherit browser-picker;
          };
          gui-smoke = pkgs.callPackage ./nix/checks/gui-smoke.nix {
            inherit browser-picker;
          };
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
