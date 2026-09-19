{
  lib,
  runCommand,
  browser-picker,
  dbus,
  glib,
  gtk3,
  gtk4,
  atk,
  at-spi2-core,
  gobject-introspection,
  python3,
  xdotool,
  xvfb-run,
}:

runCommand "browser-picker-gui-smoke"
  {
    nativeBuildInputs = [
      browser-picker
      dbus
      glib
      gtk3
      gtk4
      atk
      at-spi2-core
      gobject-introspection
      (python3.withPackages (ps: [ ps.pygobject3 ]))
      xdotool
      xvfb-run
    ];
    GI_TYPELIB_PATH = lib.makeSearchPath "lib/girepository-1.0" [
      at-spi2-core
      gobject-introspection
      gtk4
      atk
    ];
    BROWSER_PICKER = "${browser-picker}";
    A11Y_INSPECT = ../a11y_inspect.py;
    DBUS_SESSION_CONF = "${dbus}/share/dbus-1/session.conf";
    AT_SPI_LAUNCHER = "${at-spi2-core}/libexec/at-spi-bus-launcher";
    AT_SPI_REGISTRY = "${at-spi2-core}/libexec/at-spi2-registryd";
  }
  ''
    export BROWSER_PICKER A11Y_INSPECT DBUS_SESSION_CONF AT_SPI_LAUNCHER AT_SPI_REGISTRY
    dbus-run-session --config-file="$DBUS_SESSION_CONF" -- \
      xvfb-run -a -s '-screen 0 1024x768x24' \
      bash ${./gui-smoke.sh}
    touch "$out"
  ''
