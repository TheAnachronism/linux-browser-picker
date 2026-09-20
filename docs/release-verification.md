# Release verification

This document maps parent specification #1 onto automated proof and the remaining workstation-only checks. It does not change the current Rofi picker or any desktop MIME default. Broad GNOME or KDE support is claimed only after the corresponding session scenario has completed; those procedures exist and remain unverified on this niri host.

## Claim kinds

| Kind | Meaning |
| --- | --- |
| Automated | Proven by `nix flake check` on x86_64-linux. Fake destinations record argv. No real Browser Application is launched. |
| Workstation-observed | Proven on the current x86_64-linux niri session with the packaged binary, without changing Rofi or MIME defaults. This kind is scoped to Zen, Rofi, niri, and current MIME-default fixtures; it is not generic product support. |
| GNOME-observed | Proven by completing `nix/gnome-session.sh` on an actual GNOME graphical session. Unused until that scenario has run. |
| KDE-observed | Proven by completing `nix/plasma-session.sh` on an actual KDE Plasma graphical session. Unused until that scenario has run. |
| GNOME-unverified | Repeatable GNOME session procedure exists (`nix/gnome-session.sh`). It has not been run on a GNOME host; this niri workstation cannot host it. |
| KDE-unverified | Repeatable KDE Plasma session procedure exists (`nix/plasma-session.sh`). It has not been run on a Plasma host; this niri workstation cannot host it. |
| aarch64-unverified | The aarch64-linux package derivation is evaluated. Runtime desktop, GTK, AT-SPI, and handler behavior are not claimed. |
| Out of scope | Explicitly excluded by issue #1. Absence of the excluded behavior is the first-release contract, not a missing test. |

## Automated coverage

Run from the repository root on x86_64-linux:

```
nix flake check
```

| Surface | Check | Covers | Kind |
| --- | --- | --- | --- |
| Packaged executable | `checks.package`, `checks.xdg-associations` | Installed binary, desktop entry, icon, metainfo, locale, HTTP/HTTPS/HTML/XHTML reporting | Automated |
| Isolated XDG + private D-Bus | `checks.gui-smoke` | Target validation, routing precedence, automatic/Preselection/fallback, automatic launch-failure recovery with retained private intent, pending preservation across first-run and rule-edit saves, post-save routing, mixed URL/file input, concurrent secondary activations with successful exits, FIFO order, duplicate preservation, overflow that keeps accepted requests, Escape versus close-all cancellation, automatic bypass of an open or paused Picker, choice-requiring join during configuration pause, primary-process restart without restoring Pending Requests, launch errors, migration, recovery | Automated |
| Keyboard Picker and configuration | `checks.gui-smoke`, `checks.release-proof` | Focus, filter, Enter, Escape, Alt+number, Ctrl+Shift+P, destination and Routing Rule order, unsaved-change dialog, queue transitions, reveal, no-result recovery, Repair, contextual Routing Rule creation, manual and profile setup, invalid drafts, external conflicts, migration, unavailable capability, synchronous launch recovery | Automated |
| AT-SPI tree | `checks.release-proof` | Intended node role, name, state, focus, sensitivity, shortcut, and transition; no icon- or position-only identification | Automated |
| Privacy of logs and XDG state | `tests/environment_cli.rs`, `checks.release-proof` | Each GTK scenario generates unique URL, credential, query, search, selection, and file-path sentinels and scans logs and XDG state, exempting only the controlled destination receipt; saved TOML order and references are parsed structurally | Automated |
| Nix outputs | `checks.package`, `checks.app`, `checks.development-shell`, `checks.aarch64-defined` | x86_64-linux package; default flake app executes `version`; development shell executes rustc and cargo; aarch64-linux package derivation is evaluated | Automated; aarch64 runtime remains aarch64-unverified |
| Home Manager associations | `checks.home-manager-associations` | Faithful Home Manager evaluation installs Browser Picker, independently sets HTTP/HTTPS versus HTML/XHTML defaults, and generates no canonical `browser-picker/config.toml` | Automated |
| English-complete, translation-ready copy | `checks.package` `postCheck`, `checks.release-proof` | `msgfmt --check`, catalog/source coverage, no placeholder catalog entries | Automated |

Never launch a real Browser Application from the automated suite. Fake destinations record argv instead.

## Desktop session coverage

The parent specification names GNOME and KDE as the desktops to prove, but those session scenarios have not run. The procedures below are the claims' citations; they remain unverified.

| Surface | Scenario | Covers | Kind |
| --- | --- | --- | --- |
| GNOME session | `nix/gnome-session.sh` | Packaged binary inside an actual GNOME session: GIO association status, GApplication forwarding, Picker focus/keyboard/AT-SPI names, and GNOME Settings instructions, using isolated XDG without changing operator defaults | GNOME-unverified |
| KDE Plasma session | `nix/plasma-session.sh` | Packaged binary inside an actual Plasma session: GIO association status, GApplication forwarding, Picker focus/keyboard/AT-SPI names, and KDE System Settings instructions, using isolated XDG without changing operator defaults | KDE-unverified |

## Manual accessibility and desktop checklist

Observed 2026-09-20 on x86_64-linux niri after `nix build .#browser-picker`, using an isolated `XDG_CONFIG_HOME` so the operator's Rofi picker and MIME defaults were not cut over. The following checked items are workstation-scoped fixtures, not generic GNOME or KDE product proof.

### Workstation Zen applications

- [x] Discover the existing primary and secondary Zen desktop applications as two Browser Candidates (`zen-beta` and `zen-secondary`). Both advertise HTTP and HTTPS, `should_show` is true, and leftover `zen-beta-2` / `zen-beta-3` desktop files are NoDisplay MIME helpers rather than Browser Applications.
- [x] Enable each as a distinct Browser Destination with its own display label (`Zen Primary` as a verified Firefox-family profile destination on `zen-beta.desktop`; `Zen Secondary` as a generic discovered destination on `zen-secondary.desktop`). The secondary wrapper uses a custom `--profile` directory, so it stays generic rather than inheriting `~/.zen` family assumptions.
- [x] From Browser Picker, send one controlled HTTP URL to the primary Zen destination and confirm that Zen receives it. `https://zen-primary-ws31.invalid/acceptance` is present in the running `zen-beta` session store.
- [x] Send a second controlled URL to the secondary Zen destination and confirm that the other Zen application receives it. `zen-secondary` opened a distinct window titled `Problem loading page` for `https://zen-secondary-ws31.invalid/acceptance`.
- [x] Confirm private Launch Mode is offered only when that destination declared private capability. `diagnose` reports `private=available` for the verified `zen-beta` profile destination and `private=unavailable` for generic `zen-secondary`. With `Zen Primary` selected, AT-SPI exposes `Private Launch Mode` as a sensitive checkbox with `<Control><Shift>p`.

### Rofi picker

- [x] Leave `zen-link-picker` / the existing Rofi script installed and unmodified. SHA-256 of the wrapper and desktop entry was unchanged after verification.
- [x] Do not import, wrap, or switch the Rofi picker from Browser Picker.

### MIME defaults

- [x] Do not change HTTP, HTTPS, HTML, or XHTML defaults from inside Browser Picker.
- [x] If associations are installed, use the Home Manager options or an explicit user action outside this verification.
- [x] After verification, `xdg-mime query default x-scheme-handler/https` still names `zen-link-picker.desktop`. HTTP and HTML remain `zen-link-picker.desktop`; XHTML remains `helium.desktop`. No `~/.config/browser-picker` was created.

### Keyboard and screen reader

- [x] On the packaged Picker, filter starts focused; destination rows expose Alt+1 / Alt+2; private Launch Mode exposes Ctrl+Shift+P; Edit Routing Rules exposes Alt+e. Those names, roles, and shortcuts match `checks.release-proof`. Full Tab-order and unsaved-change keyboard paths remain automated (`checks.gui-smoke`, `checks.release-proof`).
- [ ] Orca (GNOME) or the KDE screen reader is not available on this niri session (`GTK_A11Y=none`, Orca not installed). Live speech is not claimed. AT-SPI names and states were dumped from the packaged binary with `GTK_A11Y=atspi` and match the automated tree: destination labels, Pending Request count, filter, private Launch Mode, and privacy-boundary description.
- [ ] Pointer drag-and-drop reordering remains optional. Automated `checks.gui-smoke` and `checks.release-proof` cover drag handles plus Alt+Shift+arrows and Alt+arrows. This Wayland session did not exercise pointer drag, so that path is not workstation-observed.

### KDE Plasma session

Not observed on this niri host. Repeat from a KDE Plasma graphical session, without changing that host's MIME defaults or replacing any existing picker:

```
nix build .#browser-picker
BROWSER_PICKER=$PWD/result bash nix/plasma-session.sh
```

The script refuses GNOME, niri, Xvfb, and tty substitutes, isolates `XDG_*`, and fails if operator `mimeapps.list` files change.

- [ ] The packaged application runs inside an actual KDE Plasma session (`plasmashell`, `XDG_CURRENT_DESKTOP` contains KDE or Plasma).
- [ ] HTTP, HTTPS, HTML, and XHTML status from `browser-picker associations` agrees with Plasma/GIO, including a KDE-specific `kde-mimeapps.list` override.
- [ ] GApplication `Open` forwarding queues a second Pending Request in the primary Picker; filter starts focused; Alt+1/Alt+2 and Ctrl+Shift+P names are exposed; activating a destination dispatches through the installed package.
- [ ] Configuration shows KDE System Settings → Applications → Default Applications instructions and no in-application takeover action.
- [ ] Operator MIME defaults are unchanged after the scenario.

### GNOME session

Not observed on this niri host. Repeat from a GNOME graphical session, without changing that host's MIME defaults or replacing any existing picker:

```
nix build .#browser-picker
BROWSER_PICKER=$PWD/result bash nix/gnome-session.sh
```

The script refuses KDE, Plasma, niri, Xvfb, and tty substitutes, isolates `XDG_*`, and fails if operator `mimeapps.list` files change.

- [ ] The packaged application runs inside an actual GNOME session (`gnome-shell`, `XDG_CURRENT_DESKTOP` contains GNOME).
- [ ] HTTP, HTTPS, HTML, and XHTML status from `browser-picker associations` agrees with GNOME/GIO, including a GNOME-specific `gnome-mimeapps.list` override.
- [ ] GApplication `Open` forwarding queues a second Pending Request in the primary Picker; filter starts focused; Alt+1/Alt+2 and Ctrl+Shift+P names are exposed; activating a destination dispatches through the installed package.
- [ ] Configuration shows GNOME Settings → Apps → Default Apps instructions and no in-application takeover action.
- [ ] Operator MIME defaults are unchanged after the scenario.

### Architecture note

- [x] aarch64-linux package evaluation is covered by `checks.aarch64-defined`. Runtime desktop proof on aarch64 is aarch64-unverified until that hardware or emulation exercises the GTK surface.

## Parent criterion map

Every numbered issue #1 user story maps to one evidence row. GNOME-observed and KDE-observed are unused.

| Criteria | Evidence | Kind |
| --- | --- | --- |
| 1–5 | Packaged desktop MIME registration and independent HTTP/HTTPS/HTML/XHTML status (`checks.package`, `checks.xdg-associations`) | Automated |
| 6 | Desktop-appropriate copy and no takeover action (`src/associations.rs` tests); GNOME Settings and KDE System Settings paths still await their session scripts | Automated; GNOME-unverified; KDE-unverified |
| 7 | Launching without an Open Target opens configuration (`checks.gui-smoke`, CLI tests) | Automated |
| 8–10 | Guided setup, pending preservation, and Picker after save (`checks.gui-smoke`, `checks.release-proof`) | Automated |
| 11–18 | Discovery, labels, IDs, order, and keyboard reorder (`checks.gui-smoke`, `checks.release-proof`, destination tests) | Automated |
| 19 | Drag handles plus Alt/Alt+Shift arrows (`checks.gui-smoke`, `checks.release-proof`); live pointer drag on this niri session is unchecked | Automated |
| 20–21 | Browser Application icons and optional overrides in setup and Picker | Automated |
| 22–23, 25–29 | Firefox- and Chromium-family profile discovery, locators, validation, and running-instance argv (`tests/routing_cli.rs`, `src/profiles.rs`, `checks.release-proof`) | Automated |
| 24 | Zen desktop IDs use Firefox-family assumptions without unknown derivatives (`src/profiles.rs`); actual `zen-beta` / `zen-secondary` destinations are workstation fixtures | Automated; Workstation-observed |
| 30 | Flatpak and Snap packaging stay generic discovered destinations (`src/profiles.rs`) | Automated |
| 31–37 | Manual argv destinations and GAppInfo launch for discovered applications (`tests/routing_cli.rs`, `checks.gui-smoke`) | Automated |
| 38–44 | Private Launch Mode as an action option, capability, disclaimer, and Ctrl+Shift+P (`checks.gui-smoke`, `checks.release-proof`) | Automated |
| 45–70 | Matching URL, structured/glob/regex conditions, ordered Routing Rules, Preselection, Fallback Action, file bypass, and rule editor (`tests/routing_cli.rs`, `checks.gui-smoke`, `checks.release-proof`) | Automated |
| 71–76 | Host emphasis, IDN forms, reveal/copy of URL and path, concealed credentials (`checks.release-proof`) | Automated |
| 77–82 | File URI/path intake, symlink and authority limits, no MIME sniffing, revalidation (`tests/routing_cli.rs`, `checks.gui-smoke`) | Automated |
| 83 | Desktop entry advertises only HTML and XHTML local documents (`checks.package`) | Automated |
| 84–87 | Type-to-filter, arrows, Enter, Alt+1–Alt+9, Preselection focus (`checks.gui-smoke`, `checks.release-proof`) | Automated |
| 88 | AT-SPI names, roles, states, and shortcuts (`checks.release-proof`); live Orca/KDE speech is unchecked | Automated |
| 89 | Normal presentation without always-on-top (`checks.gui-smoke`, `checks.release-proof`) | Automated |
| 90–103 | Escape versus close-all, reused Picker, no-result recovery, launch-failure recovery, queue FIFO, duplicates, mixed URL/file (`checks.gui-smoke`, `checks.release-proof`) | Automated |
| 104–107 | Activation and queue caps, 64 KiB bound, in-memory Pending Requests (`checks.gui-smoke`, `tests/routing_cli.rs`, `tests/environment_cli.rs`) | Automated |
| 108–111 | One GApplication primary, successful secondary exits, no-D-Bus automatic-only path (`checks.gui-smoke`, `tests/environment_cli.rs`) | Automated |
| 112–115 | Headless validate/help/version and graphical routing CLI (`tests/cli.rs`, `tests/environment_cli.rs`) | Automated |
| 116–137 | Canonical TOML, drafts, conflicts, backups, permissions, migration, paused configuration (`tests` under `configuration::store`, `checks.gui-smoke`, `checks.release-proof`) | Automated |
| 138–141 | Non-sensitive window state, redacted diagnostics, no persisted searches or Pending Requests (`tests/environment_cli.rs`, `checks.release-proof`) | Automated |
| 142 | No telemetry, crash reporting, network services, or update checks in the product; also listed as excluded | Automated; Out of scope |
| 143 | Flake package, app, and development shell (`checks.package`, `checks.app`, `checks.development-shell`) | Automated |
| 144 | aarch64-linux derivation evaluates (`checks.aarch64-defined`); runtime desktop behavior is not claimed | Automated; aarch64-unverified |
| 145–147 | Home Manager installs Browser Picker, splits HTTP/HTTPS from HTML/XHTML, and does not own `config.toml` (`checks.home-manager-associations`) | Automated |
| 148 | Repeatable GNOME and KDE scenarios exist (`nix/gnome-session.sh`, `nix/plasma-session.sh`) and have not been completed | GNOME-unverified; KDE-unverified |
| 149 | Generic XDG instructions and GIO association reporting; no extra desktop-specific guarantee | Automated |
| 150–151 | English-complete, translation-ready copy (`checks.package` `postCheck`, `checks.release-proof`) | Automated |
| 152 | GPL-3.0-or-later license installed with the package (`checks.package`) | Automated |
| 153 | Current `zen-beta` and `zen-secondary` receive controlled URLs as distinct destinations | Workstation-observed |
| 154 | Existing Rofi picker and MIME defaults left untouched on this niri host | Workstation-observed |

## Out of scope

Issue #1 excludes the following. They are not proven product features.

- Changing, removing, importing, or wrapping the current workstation's Rofi-based picker or MIME associations.
- Automatically changing HTTP, HTTPS, HTML, or XHTML defaults from inside Browser Picker.
- Persistent daemon operation, background tray behavior, PID-file coordination, or Unix-socket IPC fallback.
- Browser installation, update management, history, tab/session management, or general browser management.
- Creating, renaming, deleting, cloning, or repairing browser-owned profiles.
- Guaranteed automatic profile discovery or private-mode adapters for Flatpak and Snap browsers.
- Treating every browser derivative as verified merely because it resembles Firefox or Chromium.
- Forced independent browser processes, shared-profile concurrency, or isolated ephemeral private profiles.
- Claims that private/incognito mode prevents all disk writes, downloads, browser fingerprinting, or process sharing.
- URI schemes other than HTTP, HTTPS, and local `file` input.
- Automatic Routing Rules or Fallback Actions for local files.
- Desktop MIME registration beyond HTML and XHTML documents.
- File content sniffing, MIME-based trust decisions, directory routing, special-file routing, or remote file authorities.
- Arbitrary boolean-expression trees, computed rule specificity, substring-default patterns, PCRE/backtracking regex, or shell-command destinations.
- Manual destination environment overrides, custom working directories, shell expansion, conditionals, or multiple target substitutions.
- One-click inferred “remember this choice” rules.
- Usage history, recent-domain lists, most-recently-used destination ordering, persisted Picker searches, or recoverable on-disk queues.
- Full-target diagnostic logging, telemetry, crash uploads, remote services, or application-managed update checks.
- Secret-service storage for query values.
- Automatic config migration, partial loading of invalid config, silent unknown-key tolerance, or silent conflict overwrite.
- Home Manager ownership of the mutable Browser Picker TOML schema.
- Generic release archives, distro-specific packages, Flatpak packaging, or Snap packaging in the first release.
- Runtime support claims for aarch64-linux until the desktop behavior is exercised on that architecture.
- Shipping non-English translations in the first release.
- Always-on-top behavior, terminal Picker fallback, or separate CLI/GUI/daemon binaries.

## Parent specification coverage

Every externally observable parent criterion is in the parent criterion map, with the claim kind recorded. Out-of-scope parent items remain out of scope and are not marked as proven. Live GNOME/KDE screen-reader speech and pointer drag on this niri host are left unchecked because they were not workstation-observed. The GNOME and KDE session procedures are cited as the repeatable scenarios for criterion 148 and remain GNOME-unverified and KDE-unverified until those hosts complete them. Zen, Rofi, niri, and current MIME-default observations stay workstation-scoped.
