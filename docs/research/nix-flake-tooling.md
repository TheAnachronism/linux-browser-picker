# Nix flake tooling for Browser Picker

Assessment of established flake and nixpkgs tooling against the current
`flake.nix`. Distinguishes **flake-output composition** (how
`packages`/`checks`/`devShells` are generated per system) from **package
build/install** (Rust, GTK wrapping, desktop/AppStream/gettext). No flake
inputs or Nix code were changed for this note.

## What `flake.nix` currently does

| Lines | Responsibility | Kind |
| --- | --- | --- |
| 4 | Single input: `nixpkgs` | composition |
| 9–14 | Hard-coded `x86_64-linux` + `aarch64-linux`; `lib.genAttrs` + `import nixpkgs` | composition |
| 17–85 | `packages.*.browser-picker` via `rustPlatform.buildRustPackage` | build |
| 23–31 | Source filter excluding `.git`, `.envrc`, `target` | build |
| 32 | `cargoLock.lockFile = ./Cargo.lock` | build |
| 33–37 | `gettext`, `pkg-config`, `wrapGAppsHook4` | build |
| 38–41 | `gtk4`, `libadwaita` | build |
| 42–50 | `desktop-file-validate`, `appstreamcli validate --no-net`, `msgfmt --check` in `postCheck` | build/check |
| 52–71 | `postInstall`: localize and install existing `.desktop` and metainfo with `msgfmt`, install SVG, POT, LICENSE, `.mo` | install |
| 87–93 | `apps.default` pointing at the packaged binary | composition |
| 95–101 | `checks.package` = the package | composition |
| 102–456 | `checks.gui-smoke`: `runCommand` + dbus + `xvfb-run` + `xdotool` | check (app-specific) |
| 460–474 | `devShells.default` via `inputsFrom` + cargo/clippy/rustc/rustfmt | composition |

**Dominant source of file length:** `checks.gui-smoke` (lines 102–456, about
355 of 477 lines). The rest of the flake is a small per-system wrapper around
one `buildRustPackage`.

## Classification

Each option is **recommended**, **optional**, or **not justified** for this
repository.

### Flake-output composition

#### `lib.genAttrs` (current) — recommended keep

The current `forAllSystems` is the same pattern nixpkgs documents as
`lib.genAttrs` ([Nixpkgs lib.attrsets.genAttrs](https://nixos.org/manual/nixpkgs/unstable/#function-library-lib.attrsets.genAttrs)).
It already instantiates nixpkgs per system. Replacing it does not shrink the
GUI check or the package derivation.

#### flake-parts — optional, not justified now

[flake-parts](https://flake.parts/) is a module-system wrapper around the flake
schema. It can replace the `systems` list and `forAllSystems` with
`systems` + `perSystem` ([getting started](https://flake.parts/getting-started.html),
[systems option](https://flake.parts/options/flake-parts.html),
[working with system](https://flake.parts/system.html)).
`perSystem.packages` / `apps` / `checks` / `devShells` map 1:1 onto this
flake's output names.

**Can replace:** lines 9–14 and the repeated `forAllSystems` wrappers around
`packages`, `apps`, `checks`, `devShells`.

**Cannot replace:** `buildRustPackage`, `postInstall`, `postCheck`, or the
`gui-smoke` shell. Those are package/check bodies, not flake schema.

Adding an input and a module wrapper to delete ~15 lines of `genAttrs` is not
justified while the file is dominated by one check script. Revisit if the flake
grows extra packages, formatters, or imported modules.

#### flake-utils — not justified

[numtide/flake-utils](https://github.com/numtide/flake-utils) provides
`eachSystem` / `eachDefaultSystem` and `mkApp`. `eachDefaultSystem` defaults to
x86_64/aarch64 linux **and darwin**
([README `defaultSystems`](https://github.com/numtide/flake-utils/blob/main/README.md)).
This project is Linux-only GTK. `mkApp` would only rewrite the tiny
`apps.default` attrset.

**Can replace:** the same `genAttrs` loop flake-parts would replace, plus
`apps`.

**Cannot replace:** the package build or GUI check.

It is an extra input for less control than the current two-element list.
flake-parts is the better composition library if composition is revisited.

#### nix-systems — optional, not justified now

[nix-systems](https://github.com/nix-systems/nix-systems) makes the systems
list an overridable flake input (`import systems` → list of strings).
`github:nix-systems/default-linux` is exactly `aarch64-linux` + `x86_64-linux`.

**Can replace:** the hard-coded `systems` list (lines 9–12), so a consumer
could `--override-input systems`.

**Cannot replace:** anything in the package or checks.

This is a leaf application flake, not a library others compose. The override
pattern is unused here.

### Rust packaging

#### `rustPlatform.buildRustPackage` — recommended keep

Already in use. The in-tree pattern is the documented `cargoLock.lockFile`
approach, which vendors from `Cargo.lock` via fixed-output derivations and
avoids bumping `cargoHash` on every lockfile change
([Nixpkgs: Importing a Cargo.lock file](https://nixos.org/manual/nixpkgs/unstable/#importing-a-cargo.lock-file),
[Compiling Rust applications with Cargo](https://nixos.org/manual/nixpkgs/unstable/#compiling-rust-applications-with-cargo)).

`buildRustPackage` runs `cargo test` in `checkPhase` by default (same manual,
[Running package tests](https://nixos.org/manual/nixpkgs/unstable/#running-package-tests)).
This flake correctly layers desktop/AppStream/gettext checks in `postCheck`.
GTK4 wrapping belongs in `nativeBuildInputs` on this derivation (see below).
`postInstall` for data files is ordinary `stdenv` / `buildRustPackage`
behavior.

#### crane — not justified

[crane](https://github.com/ipetkov/crane) ([API](https://crane.dev/API.html))
splits Cargo dependency artifacts (`buildDepsOnly`) from the crate
(`buildPackage`) and offers `cargoClippy` / `cargoFmt` / `cargoTest` as
separate checks. That is useful for large workspaces and CI granularity.

It does **not** replace GTK wrapping, gettext install, desktop/AppStream
install, or `gui-smoke`. Those still attach as `nativeBuildInputs` /
`postInstall` / extra `checks` derivations.

Default `craneLib.cleanCargoSource` keeps only `.rs`, `.toml`, `Cargo.lock`,
and `.cargo/config` ([`filterCargoSources.nix`](https://github.com/ipetkov/crane/blob/master/lib/filterCargoSources.nix)).
That would drop `data/*.desktop`, `data/*.metainfo.xml`, `data/*.svg`, `po/`,
and `LICENSE` unless a custom filter is written. `wrapGAppsHook4` and
`msgfmt` also must not run on the dummy `buildDepsOnly` tree; crane documents
that hooks which need real sources must be split or gated on
`CRANE_BUILD_DEPS_ONLY`
([FAQ](https://crane.dev/faq/control-when-hooks-run.html)).

For a single small crate that already uses `cargoLock.lockFile`, crane adds an
input and source-filter/hook complexity without replacing the long part of the
flake.

#### naersk (other established option) — not justified

[naersk](https://github.com/nix-community/naersk) is another Cargo builder.
Nixpkgs itself points projects that want community toolchains at crane or
naersk as examples under fenix
([Using community maintained Rust toolchains](https://nixos.org/manual/nixpkgs/unstable/#using-community-maintained-rust-toolchains)).
It has the same gap as crane for desktop/gettext/GTK. crane is the more
actively documented of the two; neither is needed here.

### Desktop files

#### Installing an existing `.desktop` file by hand — recommended keep

The project already ships
`data/io.github.TheAnachronism.BrowserPicker.desktop` (reverse-DNS name as
required by the [Desktop Entry Specification, File naming](https://specifications.freedesktop.org/desktop-entry-spec/latest/file-naming.html)).
XDG data files are installed under `$datadir` (default `/usr/share`)
([XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/)).
Desktop IDs are defined relative to `$XDG_DATA_DIRS/.../applications/`.

GNU gettext's `msgfmt --desktop --template=...` is the documented way to merge
PO files into a desktop template
([msgfmt Invocation](https://www.gnu.org/software/gettext/manual/html_node/msgfmt-Invocation.html)).
The current `postInstall` does exactly that into
`$out/share/applications/`.

**Manually installing (or `msgfmt --desktop` installing) an existing desktop
file is the normal packaging path** when upstream already has one. nixpkgs
does this throughout; `install -Dm644 path.desktop $out/share/applications/`
is ordinary. This repo additionally localizes it, which `install` alone cannot
do.

#### `makeDesktopItem` — not justified; not intended for this situation

nixpkgs documents `makeDesktopItem` as **writing** an XDG desktop file to the
store from Nix attributes
([Trivial build helpers: makeDesktopItem](https://nixos.org/manual/nixpkgs/unstable/#trivial-builder-makeDesktopItem)).
The implementation is a `writeTextFile` that renders `[Desktop Entry]` keys
and then runs `desktop-file-validate`
([`make-desktopitem/default.nix`](https://github.com/NixOS/nixpkgs/blob/master/pkgs/build-support/make-desktopitem/default.nix)).
The documented usage is generating a file that does not already exist in
`src`, typically then copied with `copyDesktopItems`.

It is **not** an installer for an existing `.desktop`. Using it here would:

- Duplicate `Name`/`Exec`/`MimeType`/etc. in Nix instead of treating
  `data/*.desktop` as source of truth.
- Skip `msgfmt --desktop` localization.
- Still need a second path for the icon, metainfo, and `.mo` files.

#### `copyDesktopItems` — not justified

The [copy-desktop-items hook](https://github.com/NixOS/nixpkgs/blob/master/pkgs/build-support/setup-hooks/copy-desktop-items.sh)
copies either a raw `.desktop` path or
`$desktopItem/share/applications/*.desktop` into `$outputBin/share/applications`.
It does not run gettext. Combining it with `makeDesktopItem` is the nixpkgs
pattern for **generated** files. For a localized upstream file, `msgfmt
--desktop` into `$out/share/applications` is the right hook.

#### `desktop-file-validate` — recommended keep

`makeDesktopItem` itself validates with
`desktop-file-utils`'s `desktop-file-validate`. This flake already runs that
in `postCheck` on the source file. Keep the tool; do not add `makeDesktopItem`
just to get the same validator.

### AppStream metadata

There is **no** nixpkgs `copyMetaInfoItems` / `makeMetaInfoItem` analogue.
Upstream MetaInfo is XML that `appstreamcli` validates
([AppStream README: Data Validation](https://github.com/ximion/appstream):
`appstreamcli validate --pedantic /path/to/metadata.xml`;
[AppStream documentation](https://www.freedesktop.org/software/appstream/docs/)).
Install location is `$datadir/metainfo/<id>.metainfo.xml` (legacy
`share/appdata` is obsolete). Gettext localizes the XML with
`msgfmt --xml --template=...`
([msgfmt Invocation](https://www.gnu.org/software/gettext/manual/html_node/msgfmt-Invocation.html)).

**Recommended keep:** current `msgfmt --xml` into `$out/share/metainfo/` plus
`appstreamcli validate --no-net` in `postCheck` (`--no-net` is appropriate in
the Nix sandbox; `--pedantic` is optional strictness from upstream).

**Not justified:** generating metainfo from Nix attributes.

### Gettext

`gettext` in `nativeBuildInputs` provides `msgfmt`. There is no rustPlatform
setup hook that compiles `po/` automatically. Meson/ninja gettext modules are
irrelevant (this is not a Meson project).

Current uses of `msgfmt` are the documented ones:

- `--desktop` / `--xml` for desktop and AppStream templates
- default mode for `po/en.po` → `share/locale/en/LC_MESSAGES/browser-picker.mo`
- `--check` in `postCheck`

**Recommended keep.** `BROWSER_PICKER_LOCALEDIR` is application configuration,
not a nixpkgs helper.

### GTK wrapping

#### `wrapGAppsHook4` — recommended keep

Already in `nativeBuildInputs`. nixpkgs documents `wrapGAppsHook4` as the GTK
4 member of the wrapGApps family: it wraps `bin`/`libexec` with
`XDG_DATA_DIRS`, GSettings schemas, GdkPixbuf loaders, GIO modules, and
related variables, and pulls `gtk4` into the wrap set
([Onto wrapGApps\* hooks](https://nixos.org/manual/nixpkgs/unstable/#ssec-gnome-hooks),
[hook implementation](https://github.com/NixOS/nixpkgs/blob/master/pkgs/build-support/setup-hooks/wrap-gapps-hook/wrap-gapps-hook.sh)).

`pkg-config` + `gtk4`/`libadwaita` in `buildInputs` is the matching compile
link. No extra wrapper library is required.

`hicolor` icon theme cache handling is a GTK setup-hook concern; installing
the SVG under `share/icons/hicolor/scalable/apps/` with the application id is
the usual layout. Keep the `install -Dm644` of the existing SVG.

### GUI / integration checks

`checks.gui-smoke` is a `pkgs.runCommand` driving the real binary under
`dbus-run-session` + `xvfb-run` + `xdotool`. That is ordinary nixpkgs
(`runCommand` is a trivial builder
([runCommand](https://nixos.org/manual/nixpkgs/unstable/#trivial-builder-runCommand))).
No flake-parts or crane API encodes this scenario.

**NixOS VM tests** ([NixOS manual: NixOS tests](https://nixos.org/manual/nixos/unstable/#sec-nixos-tests))
are the other established option. They boot a full QEMU NixOS and can drive a
desktop session. They would not delete the scenario logic; they would add a
VM closure, a NixOS module, and more evaluation cost than Xvfb. **Not
justified** while the existing smoke test already exercises first-run,
`gtk-launch`, routing, and queue behavior.

crane's `cargoClippy` / `cargoFmt` could become extra `checks.*` entries later
(**optional**), but they are orthogonal to GUI smoke and would not shorten
`flake.nix`.

## Mapping: tool → flake section

| Tool | Verdict | Replaces in `flake.nix` | Must not replace |
| --- | --- | --- | --- |
| `lib.genAttrs` (current) | recommended | already implements per-system outputs | — |
| flake-parts | optional / not now | lines 9–14 and `forAllSystems` wrappers | package body, `postInstall`, `gui-smoke` |
| flake-utils | not justified | same loop; `mkApp` for lines 87–93 | same as above; would widen systems to Darwin by default |
| nix-systems | not justified | lines 9–12 only | everything else |
| `buildRustPackage` + `cargoLock` | recommended | already lines 20–79 | — |
| crane | not justified | Cargo fetch/build/split checks only | `wrapGAppsHook4`, gettext, desktop/metainfo, `gui-smoke`; default source clean drops `data/` and `po/` |
| naersk | not justified | same as crane's Cargo slice | same install/check bodies |
| `wrapGAppsHook4` | recommended | already lines 36, wrapping of `$out/bin` | — |
| `gettext` / `msgfmt` | recommended | already `postInstall` + `postCheck` | — |
| `makeDesktopItem` | not justified | none appropriately | existing `data/*.desktop` + `msgfmt --desktop` |
| `copyDesktopItems` | not justified | none appropriately | localized desktop install |
| `desktop-file-validate` | recommended | already `postCheck` | — |
| `appstreamcli validate` | recommended | already `postCheck` | — |
| NixOS VM tests | not justified | could rehost `gui-smoke` at higher cost | would not shrink scenario code |

## Low-complexity recommendation

1. **Keep the current flake composition.** Two Linux systems and
   `lib.genAttrs` are already the small, correct shape. Do not add
   flake-parts, flake-utils, or nix-systems until there are multiple packages
   or imported flake modules.
2. **Keep `rustPlatform.buildRustPackage` with `cargoLock.lockFile`.** Do not
   introduce crane unless Cargo dependency rebuild time becomes a measured
   problem; if you do, you must keep `data/`, `po/`, and `LICENSE` in `src`
   and keep GTK/gettext hooks off `buildDepsOnly`.
3. **Keep installing the existing desktop and metainfo files.**
   `makeDesktopItem` is for generating a desktop file from Nix, not for
   installing one the project already maintains and localizes.
4. **Keep `wrapGAppsHook4`, `gettext`, `desktop-file-validate`, and
   `appstreamcli`.** Those are the nixpkgs/upstream tools that match the
   GTK4 + gettext + AppStream install path.
5. **If `flake.nix` length is the problem, move `gui-smoke` out of the flake
   into a shell script (or a small `.nix` that only `readFile`s it).** That
   is a file split, not a new flake framework. No established tool replaces
   that app-specific Xvfb/xdotool script.

Net: this flake is long because of a hand-written GUI check, not because it
reinvents per-system outputs or Rust packaging. Adding composition or Crane
inputs would not address the length and would not replace the desktop,
gettext, or GTK logic that is already using the right nixpkgs helpers.
