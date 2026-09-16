{
  config,
  lib,
  ...
}:

let
  cfg = config.programs.browser-picker;
  desktopId = "io.github.TheAnachronism.BrowserPicker.desktop";
in
{
  options.programs.browser-picker = {
    enable = lib.mkEnableOption "Browser Picker";

    package = lib.mkOption {
      type = lib.types.package;
      description = "Browser Picker package to install. The module never generates canonical TOML.";
    };

    httpHttpsDefault = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Declare Browser Picker as the HTTP and HTTPS default handler. When false, those associations are left untouched.";
    };

    htmlXhtmlDefault = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Declare Browser Picker as the HTML and XHTML default handler. When false, those associations are left untouched.";
    };
  };

  config = lib.mkMerge [
    (lib.mkIf cfg.enable {
      home.packages = [ cfg.package ];
    })
    (lib.mkIf (cfg.enable && cfg.httpHttpsDefault) {
      xdg.mimeApps.enable = true;
      xdg.mimeApps.defaultApplications = {
        "x-scheme-handler/http" = desktopId;
        "x-scheme-handler/https" = desktopId;
      };
    })
    (lib.mkIf (cfg.enable && cfg.htmlXhtmlDefault) {
      xdg.mimeApps.enable = true;
      xdg.mimeApps.defaultApplications = {
        "text/html" = desktopId;
        "application/xhtml+xml" = desktopId;
      };
    })
  ];
}
