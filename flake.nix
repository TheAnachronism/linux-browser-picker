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

      homeManagerModules = {
        default =
          { pkgs, lib, ... }:
          {
            imports = [ ./nix/home-manager.nix ];
            config.programs.browser-picker.package = lib.mkDefault self.packages.${pkgs.stdenv.hostPlatform.system}.default;
          };
        browser-picker = self.homeManagerModules.default;
      };

      checks = forAllSystems (
        pkgs:
        let
          inherit (pkgs) lib;
          system = pkgs.stdenv.hostPlatform.system;
          browser-picker = self.packages.${system}.default;
          desktopId = "io.github.TheAnachronism.BrowserPicker.desktop";
        in
        {
          package = browser-picker;
          home-manager-associations = pkgs.callPackage ./nix/checks/home-manager-associations.nix {
            inherit browser-picker desktopId;
            homeManagerModule = ./nix/home-manager.nix;
          };
          xdg-associations = pkgs.callPackage ./nix/checks/xdg-associations.nix {
            inherit browser-picker desktopId;
          };
          aarch64-defined =
            assert builtins.elem "aarch64-linux" systems;
            assert self.packages ? aarch64-linux;
            assert self.packages.aarch64-linux ? browser-picker;
            pkgs.writeText "browser-picker-aarch64-defined" ''
              aarch64-linux output is build-defined but runtime-unverified until exercised on a desktop.
            '';
          app =
            assert self.apps.${system} ? default;
            assert self.apps.${system}.default.type == "app";
            pkgs.runCommand "browser-picker-app" { } ''
              test -x ${self.apps.${system}.default.program}
              echo "app output points at the packaged Browser Picker executable" > "$out"
            '';
          development-shell = self.devShells.${system}.default;
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
