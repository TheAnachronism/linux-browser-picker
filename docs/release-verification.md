# Release verification

This document maps parent specification #1 onto automated proof and the remaining workstation-only checks. It does not change the current Rofi picker or any desktop MIME default.

## Claim kinds

| Kind | Meaning |
| --- | --- |
| Automated | Proven by `nix flake check` on x86_64-linux. Fake destinations record argv. No real Browser Application is launched. |
| Workstation-observed | Proven on the current x86_64-linux niri session with the packaged binary, without changing Rofi or MIME defaults. |
| KDE-unverified | Repeatable KDE Plasma session procedure exists (`nix/plasma-session.sh`). It has not been run on a Plasma host; this niri workstation cannot host it. |
| aarch64-unverified | The aarch64-linux package derivation is evaluated. Runtime desktop, GTK, AT-SPI, and handler behavior are not claimed. |

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
| KDE Plasma session | `nix/plasma-session.sh` | Packaged binary inside an actual Plasma session: GIO association status, GApplication forwarding, Picker focus/keyboard/AT-SPI names, and KDE System Settings instructions, using isolated XDG without changing operator defaults | KDE-unverified |

Never launch a real Browser Application from the automated suite. Fake destinations record argv instead.

## Manual accessibility and desktop checklist

Observed 2026-09-20 on x86_64-linux niri after `nix build .#browser-picker`, using an isolated `XDG_CONFIG_HOME` so the operator's Rofi picker and MIME defaults were not cut over.

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

### Architecture note

- [x] aarch64-linux package evaluation is covered by `checks.aarch64-defined`. Runtime desktop proof on aarch64 is aarch64-unverified until that hardware or emulation exercises the GTK surface.

## Parent specification coverage

Every externally observable parent criterion is either in the automated table above or named in the manual checklist, with the claim kind recorded. Out-of-scope parent items (Rofi replacement, automatic MIME takeover, telemetry, extra locales, aarch64 runtime claims) remain out of scope and are not marked as proven. Live GNOME/KDE screen-reader speech and pointer drag on this niri host are left unchecked because they were not workstation-observed. The KDE Plasma session procedure is recorded as KDE-unverified until it is run on a Plasma host.
