# Windows Tailscale incident — September 11, 2026

## Confirmed regression

The 1.7.0 integration launched `tailscale status --json` on a 15-second interval.
Windows service logs showed repeated client connection, profile selection,
`NeedsLogin -> NoState`, client disconnection, and `NoState -> NeedsLogin`.
Examples occurred at 15:56:56, 15:57:11, 15:57:26, and 15:57:41 UTC. Each profile
activation/release took only milliseconds. Diagnostic status requests triggered
the same sequence after LanDrop was stopped.

Tailscale v1.102.4's [LocalAPI request wrapper](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/ipnserver/server.go)
sets the current user when the first non-SYSTEM Windows request begins, then
clears it when the last request completes. Its
[backend profile lifecycle](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/ipnlocal/local.go)
can select a different profile and reset networking in response. This matches
the observed closed-client cycle. A GET handler being read-only does not make
its surrounding request lifecycle side-effect-free. The earlier assurance was
incorrect.

## Other findings and uncertainty

- The parser did not exclude Mullvad VPN service nodes. Tailscale's
  [human-readable status implementation](https://github.com/tailscale/tailscale/blob/v1.102.4/cmd/tailscale/cli/status.go)
  filters those nodes separately; the JSON peer map does not provide that
  filtering for callers. A captured map contained 538 entries, but zero online
  entries while the backend was `NoState`. This is not evidence that all 538 were
  being probed at that moment.
- A qualifying peer sweep used 16 concurrent three-second probes. Failed peers
  had no backoff; long sweeps could immediately start another overdue interval.
- Tailscale's desktop log also contained DNS failures and socket permission
  errors on bootstrap connections. Their precise cause is not established.
- Inspected LanDrop firewall rules were inbound allow rules. Installer firewall
  commands did not change relative to 1.6.14. No outbound LanDrop block was found.
- LanDrop was fully stopped with permission. The user reported working internet,
  but the subsequent timed comparison was aborted before launch because no
  Tailscale desktop client was running. Ethernet was the observed default route.
  A connected-Mullvad, LanDrop-on/off reproduction has therefore not completed.
- The profile-reset mechanism depends on the absence of another active client
  request. It does not alone prove the cause of failure while the Tailscale GUI
  maintains its own long-running connection.

Raw diagnostic material stays on the user's PC, not in Git. No credentials,
packet captures, message contents, or full private peer lists belong in this report.

## 1.7.1 containment

Windows automatic discovery returns an explicit safety-paused error before any
Tailscale executable or LocalAPI request is reached. No process-presence check,
unattended-mode change, permanent watch client, firewall bypass, or DNS change is
used as a workaround. LAN discovery remains independent.

Automatic Windows tailnet discovery is unavailable in this build. Re-enabling it
requires a design that respects Windows user/profile ownership and successful
connected-Mullvad testing, including tray-client exit and daemon restarts.

Mullvad filtering and bounded/backed-off probing harden discovery on platforms
where CLI discovery remains enabled. These are separate from containing Windows
profile churn. Local diagnostics must not themselves call the Tailscale CLI.

The discovery budget is 16 unconfirmed candidates per cycle, with four concurrent
probes and a 30-second pause after completed work. Failed attempts back off from
60 seconds to 15 minutes. Oldest-attempt ordering avoids starving later addresses;
the route monitor continues maintaining confirmed live devices separately.

## Acceptance boundaries

Verify the Windows guard without a Tailscale connection, automated peer-filter
and scheduling cases, native Windows build, and unchanged frontend behavior.
Only then consider an explicitly approved, locally recorded Mullvad test that
can survive losing the remote session. Do not label the entire VPN outage fixed
on the basis of unit tests or ordinary Ethernet connectivity.

## Safety-patch verification

The frontend verification passed all 87 tests, Svelte diagnostics, lint, format,
and production build checks. Linux Rust verification passed 51 tests; Windows
passed 52, including the disabled-discovery test. Both platforms passed strict
all-target/all-feature Clippy and formatting. The diagnostic recorder passed
Windows PowerShell 5.1 parsing, a normal 10-second capture, and a process-local
simulated HTTPS failure capture that still saved both failures successfully.

These tests do not reconnect Tailscale or establish that Mullvad connectivity has
recovered. Native packaging and installed-app checks are recorded separately.

## Local Windows installation

Installed the verified 1.7.1 safety build without reopening Tailscale. The local
NSIS package used a temporary copy of the installer hooks with all five firewall
commands removed. The `/UPDATE` path skips the old NSIS uninstaller; registry
inspection confirmed no machine-level LanDrop MSI migration applied. Repository
installer hooks and signing configuration were not changed.

Installer size: 4,541,959 bytes. SHA-256:
`908ffc4097178c4f5998d1eb06e0db439f20551b8260e01037c485b6a68e6563`.
Installed application SHA-256:
`601c422f68a4c4eff75f452d799f3dc51701d0c8358a1d7c6f07ff9e2041cf3a`.
The installed hash exactly matches the production executable after Tauri's
expected NSIS bundle-marker replacement. The package is unsigned and has not
been published to GitHub.

All 76 saved message IDs were preserved. Settings and the previous installation
were backed up to the user's Local Temp directory, `landrop-safety-1.7.1.REfmlN`.
All 11 inspected LanDrop firewall rules had identical normalized hashes before
and after installation. The installed app listens on LAN TCP port 29171.

A desktop shortcut named **LanDrop Network Diagnostics** points to a reviewed
local copy of the recorder. It was prepared without starting a VPN test.
