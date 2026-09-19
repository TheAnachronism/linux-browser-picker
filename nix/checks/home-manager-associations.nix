{
  lib,
  runCommand,
  browser-picker,
  desktopId,
  homeManagerModule,
}:

let
  homeManagerEval =
    {
      httpHttpsDefault ? false,
      htmlXhtmlDefault ? false,
    }:
    lib.evalModules {
      modules = [
        homeManagerModule
        {
          options.home.packages = lib.mkOption {
            type = lib.types.listOf lib.types.package;
            default = [ ];
          };
          options.xdg.mimeApps.enable = lib.mkOption {
            type = lib.types.bool;
            default = false;
          };
          options.xdg.mimeApps.defaultApplications = lib.mkOption {
            type = lib.types.attrsOf lib.types.str;
            default = { };
          };
          config.programs.browser-picker = {
            enable = true;
            package = browser-picker;
            inherit httpHttpsDefault htmlXhtmlDefault;
          };
        }
      ];
    };
  none = homeManagerEval { };
  web = homeManagerEval { httpHttpsDefault = true; };
  docs = homeManagerEval { htmlXhtmlDefault = true; };
  both = homeManagerEval {
    httpHttpsDefault = true;
    htmlXhtmlDefault = true;
  };
in
assert builtins.elem browser-picker none.config.home.packages;
assert none.config.xdg.mimeApps.enable == false;
assert none.config.xdg.mimeApps.defaultApplications == { };
assert web.config.xdg.mimeApps.defaultApplications == {
  "x-scheme-handler/http" = desktopId;
  "x-scheme-handler/https" = desktopId;
};
assert !(web.config.xdg.mimeApps.defaultApplications ? "text/html");
assert docs.config.xdg.mimeApps.defaultApplications == {
  "text/html" = desktopId;
  "application/xhtml+xml" = desktopId;
};
assert !(docs.config.xdg.mimeApps.defaultApplications ? "x-scheme-handler/http");
assert both.config.xdg.mimeApps.defaultApplications == {
  "x-scheme-handler/http" = desktopId;
  "x-scheme-handler/https" = desktopId;
  "text/html" = desktopId;
  "application/xhtml+xml" = desktopId;
};
runCommand "browser-picker-home-manager-associations" { } ''
  echo "Home Manager installs Browser Picker without generating canonical TOML" > "$out"
''
