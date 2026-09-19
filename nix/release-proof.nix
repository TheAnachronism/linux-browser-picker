{
  lib,
  runCommand,
  browser-picker,
  dbus,
  glib,
  gtk4,
  atk,
  at-spi2-core,
  gobject-introspection,
  python3,
  xdotool,
  xvfb-run,
  gettext,
}:

runCommand "browser-picker-release-proof"
  {
    nativeBuildInputs = [
      browser-picker
      dbus
      glib
      gtk4
      at-spi2-core
      gobject-introspection
      (python3.withPackages (ps: [ ps.pygobject3 ]))
      xdotool
      xvfb-run
      gettext
    ];
    GI_TYPELIB_PATH = lib.makeSearchPath "lib/girepository-1.0" [
      at-spi2-core
      gobject-introspection
      gtk4
      atk
    ];
    BROWSER_PICKER = "${browser-picker}/bin/browser-picker";
    A11Y_INSPECT = ./a11y_inspect.py;
    I18N_COVERAGE = ./i18n_coverage.py;
    SRC_DIR = ../src;
    PO_FILE = ../po/en.po;
    CHECKLIST = ../docs/release-verification.md;
    DBUS_SESSION_CONF = "${dbus}/share/dbus-1/session.conf";
    AT_SPI_LAUNCHER = "${at-spi2-core}/libexec/at-spi-bus-launcher";
    AT_SPI_REGISTRY = "${at-spi2-core}/libexec/at-spi2-registryd";
  }
  ''
    export HOME="$TMPDIR/home"
    export LANG=C.UTF-8
    export LC_ALL=C.UTF-8
    export XDG_RUNTIME_DIR="$TMPDIR/runtime"
    export XDG_DATA_HOME="$TMPDIR/xdg-data"
    export XDG_STATE_HOME="$TMPDIR/xdg-state"
    export XDG_CACHE_HOME="$TMPDIR/xdg-cache"
    export XDG_CONFIG_HOME="$TMPDIR/xdg-config"
    export XDG_DATA_DIRS="${browser-picker}/share''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
    export BROWSER_PICKER A11Y_INSPECT I18N_COVERAGE SRC_DIR PO_FILE CHECKLIST AT_SPI_LAUNCHER AT_SPI_REGISTRY
    dbus-run-session --config-file="$DBUS_SESSION_CONF" -- \
      xvfb-run -a -s '-screen 0 1920x1080x24' \
      bash ${./release-proof.sh}
    touch "$out"
  ''
