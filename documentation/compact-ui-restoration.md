# Compact desktop UI restoration — September 11, 2026

Restored the top-bar appearance from `07efc15` at the owner's request. This is a
visual revision of the unreleased 1.7.0 build, not a rollback of discovery,
notifications, updates, or saved-history handling.

- Device buttons return to 32px circular avatars and status dots. Names and
  LAN/Tailscale route descriptions remain in tooltips and accessible names.
- Refresh returns to 28px; title-bar icon buttons use a scoped 40px size. Other
  shared icon buttons retain their existing 48px default.
- History/Saved filters return to compact rounded pills and the original colors.
- Discovery-only peers, merged LAN/Tailscale identities, offline history access,
  keyboard semantics, focus handling, and error reporting are unchanged.
- Scroll arrows retain their accessibility labels and resize observation; the
  right arrow is offset to avoid overlapping the refresh control.

This intentionally restores desktop density; it does not claim 48px touch
targets for the restored compact controls.

## Verification

The complete frontend verification passed: Svelte diagnostics (zero errors and
warnings), ESLint, formatting, 87 tests, and the production Vite build.

A production-browser smoke test with mocked Windows IPC passed at 520×720 and
420×550. It exercised route tooltips, LAN/Tailscale identity deduplication,
selected-device settings, overflowing device navigation, discovery failure,
offline-device removal, and preserved history. No uncaught page errors occurred.
Measured desktop geometry: 32×32 device buttons, 28×28 refresh, 40×40 title-bar
controls, a 57px title bar, and 20.5px history pills. The rendered screenshots were
visually inspected.

These browser checks do not substitute for real remote Tailscale transfers or
native notification delivery. No Tailscale configuration changes are part of
this revision.

## Local installation

The production Windows NSIS installer built and installed successfully after the
user approved closing LanDrop. Version remains 1.7.0 because this revision has
not been publicly released. Installer size: 4,555,716 bytes; SHA-256:
`94c59d437fcbb08224c93adfbb1f232705d21bb167bfbc8257cf510611fbed9f`.

All 74 saved message IDs were preserved. Settings and the previous executable
were backed up under the user's Local Temp directory as
`landrop-compact-backup.HTfiNL`. The installed executable matches the built binary
exactly after accounting for Tauri's expected `UNK` to `NSS` bundle-type marker
replacement. It embeds the verified compact frontend assets.

Native Windows UI Automation confirmed the app launches, the updates protocol
opens Settings/About, and the installed version is 1.7.0. Title-bar controls
measure 50 physical pixels high at the current 125% display scale, matching the
restored 40px CSS sizing. The installer is unsigned; no updater signature or
GitHub release was published.
