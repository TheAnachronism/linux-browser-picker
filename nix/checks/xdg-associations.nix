{
  runCommand,
  browser-picker,
  desktop-file-utils,
  glib,
  python3,
  shared-mime-info,
  desktopId,
}:

runCommand "browser-picker-xdg-associations"
  {
    nativeBuildInputs = [
      browser-picker
      desktop-file-utils
      glib
      python3
      shared-mime-info
    ];
    BROWSER_PICKER = "${browser-picker}";
    DESKTOP_ID = desktopId;
    CHECK_DESKTOP_ENTRY = ./check_desktop_entry.py;
  }
  ''
    export BROWSER_PICKER DESKTOP_ID CHECK_DESKTOP_ENTRY
    bash ${./xdg-associations.sh}
  ''
