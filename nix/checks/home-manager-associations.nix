{
  lib,
  pkgs,
  runCommand,
  browser-picker,
  desktopId,
  homeManagerConfiguration,
  homeManagerModule,
}:

let
  homeManagerEval =
    {
      httpHttpsDefault ? false,
      htmlXhtmlDefault ? false,
    }:
    homeManagerConfiguration {
      inherit pkgs;
      modules = [
        homeManagerModule
        {
          home.username = "tester";
          home.homeDirectory = "/home/tester";
          home.stateVersion = "25.05";
          home.enableNixpkgsReleaseCheck = false;
          news.display = "silent";
          manual.manpages.enable = false;
          programs.browser-picker = {
            enable = true;
            package = lib.mkForce browser-picker;
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

  canonicalConfigNames =
    config:
    lib.filter (name: lib.hasSuffix "browser-picker/config.toml" name) (
      lib.attrNames config.xdg.configFile ++ lib.attrNames config.home.file
    );

  mimeappsSource =
    config:
    if config.xdg.configFile ? "mimeapps.list" then
      config.xdg.configFile."mimeapps.list".source
    else
      null;
in
assert builtins.elem browser-picker none.config.home.packages;
assert none.config.xdg.mimeApps.enable == false;
assert none.config.xdg.mimeApps.defaultApplications == { };
assert !(none.config.xdg.configFile ? "mimeapps.list");
assert canonicalConfigNames none.config == [ ];
assert web.config.xdg.mimeApps.enable == true;
assert
  web.config.xdg.mimeApps.defaultApplications == {
    "x-scheme-handler/http" = [ desktopId ];
    "x-scheme-handler/https" = [ desktopId ];
  };
assert !(web.config.xdg.mimeApps.defaultApplications ? "text/html");
assert !(web.config.xdg.mimeApps.defaultApplications ? "application/xhtml+xml");
assert canonicalConfigNames web.config == [ ];
assert docs.config.xdg.mimeApps.enable == true;
assert
  docs.config.xdg.mimeApps.defaultApplications == {
    "text/html" = [ desktopId ];
    "application/xhtml+xml" = [ desktopId ];
  };
assert !(docs.config.xdg.mimeApps.defaultApplications ? "x-scheme-handler/http");
assert !(docs.config.xdg.mimeApps.defaultApplications ? "x-scheme-handler/https");
assert canonicalConfigNames docs.config == [ ];
assert
  both.config.xdg.mimeApps.defaultApplications == {
    "x-scheme-handler/http" = [ desktopId ];
    "x-scheme-handler/https" = [ desktopId ];
    "text/html" = [ desktopId ];
    "application/xhtml+xml" = [ desktopId ];
  };
assert canonicalConfigNames both.config == [ ];
runCommand "browser-picker-home-manager-associations"
  {
    noneActivate = none.activationPackage;
    webActivate = web.activationPackage;
    docsActivate = docs.activationPackage;
    bothActivate = both.activationPackage;
    webMime = mimeappsSource web.config;
    docsMime = mimeappsSource docs.config;
    bothMime = mimeappsSource both.config;
    inherit desktopId;
  }
  ''
    set -eu
    assert_installed() {
      test -x "$1/home-path/bin/browser-picker"
    }
    assert_no_canonical_toml() {
      if find "$1" -path '*browser-picker/config.toml' -print | grep -q .; then
        echo "Home Manager artifacts must not contain canonical browser-picker/config.toml" >&2
        find "$1" -path '*browser-picker/config.toml' -print >&2
        exit 1
      fi
    }
    assert_installed "$noneActivate"
    assert_installed "$webActivate"
    assert_installed "$docsActivate"
    assert_installed "$bothActivate"
    assert_no_canonical_toml "$noneActivate"
    assert_no_canonical_toml "$webActivate"
    assert_no_canonical_toml "$docsActivate"
    assert_no_canonical_toml "$bothActivate"
    grep -F "x-scheme-handler/http=$desktopId" "$webMime"
    grep -F "x-scheme-handler/https=$desktopId" "$webMime"
    if grep -E '^(text/html|application/xhtml\+xml)=' "$webMime"; then
      echo "HTTP/HTTPS defaults must not declare HTML/XHTML handlers" >&2
      exit 1
    fi
    grep -F "text/html=$desktopId" "$docsMime"
    grep -F "application/xhtml+xml=$desktopId" "$docsMime"
    if grep -E '^(x-scheme-handler/http|x-scheme-handler/https)=' "$docsMime"; then
      echo "HTML/XHTML defaults must not declare HTTP/HTTPS handlers" >&2
      exit 1
    fi
    grep -F "x-scheme-handler/http=$desktopId" "$bothMime"
    grep -F "x-scheme-handler/https=$desktopId" "$bothMime"
    grep -F "text/html=$desktopId" "$bothMime"
    grep -F "application/xhtml+xml=$desktopId" "$bothMime"
    echo "Home Manager installs Browser Picker, sets independent defaults, and does not generate canonical TOML" > "$out"
  ''
