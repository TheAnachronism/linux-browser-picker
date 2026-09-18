# Release verification

This document maps parent specification #1 onto automated proof and the remaining workstation-only checks. It does not change the current Rofi picker or any desktop MIME default.

## Automated coverage

Run from the repository root on x86_64-linux:

```
nix flake check
```

| Surface | Check | Covers |
| --- | --- | --- |
| Packaged executable | `checks.package`, `checks.xdg-associations` | Installed binary, desktop entry, icon, metainfo, locale, HTTP/HTTPS/HTML/XHTML reporting |
| Isolated XDG + private D-Bus | `checks.gui-smoke` | Target validation, routing precedence, automatic/Preselection/fallback, automatic launch-failure recovery with retained private intent, pending preservation across first-run and rule-edit saves, post-save routing, mixed URL/file input, concurrent activations, FIFO queue, overflow, cancellation, launch errors, migration, recovery |
| Keyboard Picker and configuration | `checks.gui-smoke`, `checks.release-proof` | Focus, filter, Enter, Escape, Alt+number, Ctrl+Shift+P, destination and Routing Rule order, unsaved-change dialog, queue transitions |
| AT-SPI tree | `checks.release-proof` | Names, roles, states, selection, availability, rule/action context, shortcuts; no icon- or position-only identification |
| Privacy of logs and XDG state | `tests/environment_cli.rs`, `checks.release-proof` | No complete URLs, query values, credentials, file paths, searches, choices, or Pending Requests in diagnostics or persisted state |
| Nix outputs | `checks.package`, `checks.app`, `checks.development-shell`, `checks.aarch64-defined` | x86_64-linux package/app/development outputs; aarch64-linux package attribute is defined; runtime desktop proof is checklist-only |
| Home Manager associations | `checks.home-manager-associations` | Optional HTTP/HTTPS and HTML/XHTML defaults without generating canonical TOML |
| English-complete, translation-ready copy | `checks.package` `postCheck`, `checks.release-proof` | `msgfmt --check`, no placeholder catalog entries |

Never launch a real Browser Application from the automated suite. Fake destinations record argv instead.

## Manual accessibility and desktop checklist

Perform these on the workstation after `nix build` / `nix run`, without changing existing defaults.

### Workstation Zen applications

- [ ] Discover the existing primary and secondary Zen desktop applications as two Browser Candidates (`zen-beta-2` and `zen-beta-3`, or the current workstation names).
- [ ] Enable each as a distinct Browser Destination with its own display label.
- [ ] From Browser Picker, send one controlled HTTP URL to the primary Zen destination and confirm that Zen receives it.
- [ ] Send a second controlled URL to the secondary Zen destination and confirm that the other Zen application receives it.
- [ ] Confirm private Launch Mode is offered only when that destination declared private arguments.

### Rofi picker

- [ ] Leave `zen-link-picker` / the existing Rofi script installed and unmodified.
- [ ] Do not import, wrap, or switch the Rofi picker from Browser Picker.

### MIME defaults

- [ ] Do not change HTTP, HTTPS, HTML, or XHTML defaults from inside Browser Picker.
- [ ] If associations are installed, use the Home Manager options or an explicit user action outside this verification.
- [ ] After verification, `xdg-mime query default x-scheme-handler/https` still names the previously configured handler unless the operator chose otherwise.

### Keyboard and screen reader

- [ ] Tab order reaches filter, destination list, private Launch Mode, and configuration actions without a pointer.
- [ ] Orca (GNOME) or the KDE screen reader announces destination labels, availability, Pending Request count, and the unsaved-changes dialog from the same names as `checks.release-proof`.
- [ ] Pointer drag-and-drop reordering remains optional; Alt+Shift+arrows and Alt+arrows still work.

### Architecture note

- [ ] aarch64-linux package evaluation is covered by `checks.aarch64-defined`. Runtime desktop proof on aarch64 is deferred until that hardware or emulation exercises the GTK surface.

## Parent specification coverage

Every externally observable parent criterion is either in the automated table above or named in the manual checklist. Out-of-scope parent items (Rofi replacement, automatic MIME takeover, telemetry, extra locales, aarch64 runtime claims) remain out of scope.
