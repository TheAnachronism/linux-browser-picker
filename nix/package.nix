{
  lib,
  rustPlatform,
  gettext,
  pkg-config,
  wrapGAppsHook4,
  gtk4,
  libadwaita,
  appstream,
  desktop-file-utils,
  python3,
  shared-mime-info,
  hicolor-icon-theme,
  systems,
}:

rustPlatform.buildRustPackage {
  pname = "browser-picker";
  version = "0.1.0";
  src = lib.cleanSourceWith {
    src = ../.;
    filter =
      path: type:
      let
        name = baseNameOf path;
      in
      name != ".git"
        && name != ".envrc"
        && name != "target"
        && name != "a11y_inspect.py"
        && name != "release-proof.sh"
        && name != "release-proof.nix";
  };
  cargoLock.lockFile = ../Cargo.lock;
  nativeBuildInputs = [
    gettext
    pkg-config
    wrapGAppsHook4
  ];
  buildInputs = [
    gtk4
    libadwaita
    shared-mime-info
    hicolor-icon-theme
  ];
  nativeCheckInputs = [
    appstream
    desktop-file-utils
    python3
  ];
  postCheck = ''
    desktop-file-validate data/io.github.TheAnachronism.BrowserPicker.desktop
    appstreamcli validate --no-net data/io.github.TheAnachronism.BrowserPicker.metainfo.xml
    msgfmt --check po/en.po -o /dev/null
    python3 nix/i18n_coverage.py src po/en.po
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
    license = lib.licenses.gpl3Plus;
    mainProgram = "browser-picker";
    platforms = systems;
  };
}
