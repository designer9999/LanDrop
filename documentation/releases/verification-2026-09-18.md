# September 18, 2026 release preparation

Scope: validate and deliver the completed 1.7.1 Windows safety update, not the
proposed automatic Windows Tailscale discovery replacement or performance fixes.
Base revision: `89c1016507e91bbc9cb114f2fe8b3d65a3361eb6`.

## Changes

- Update locked `rustls` 0.23.44 to 0.23.45 for
  [GHSA-2mjx-qc3c-rqvc](https://github.com/rustls/rustls/security/advisories/GHSA-2mjx-qc3c-rqvc).
- Update development-only `devalue` 5.8.2 to 5.9.2 for
  [GHSA-9rgm-9g3h-6x36](https://github.com/sveltejs/devalue/security/advisories/GHSA-9rgm-9g3h-6x36).
- Preserve the existing package-manager declaration update to pnpm 12.4.2.
  npm remains the installer used with the existing npm lockfile and CI.
- Include the earlier audit, passive measurement script and proposed Tailscale
  design, all explicitly distinguished from implemented features.

## Validation

Tests ran in an isolated Windows-owned source copy using Windows Node 24.18.0,
npm 11.12.1 and Rust/Cargo MSVC 1.98.1. The first frontend pass accidentally used
the system npm launcher's adjacent Node 26; a temporary launcher pinned Node 24
and the complete frontend verification was repeated successfully. No application
source or user dependency tree was changed to accommodate that launcher.

| Check | Result |
| --- | --- |
| Windows frontend type check | Passed, zero errors/warnings |
| ESLint / Prettier | Passed |
| Windows frontend unit tests | Passed, 87 tests |
| Windows frontend production build | Passed |
| Windows Rust tests, locked, all targets/features | Passed, 52 tests |
| Windows Rust Clippy, warnings denied | Passed |
| Rust formatting | Passed before dependency-only patches |
| npm audit after patched lockfile / clean install | Passed, zero reported vulnerabilities |
| Rust advisory check after patched lockfile | No blocking advisories; limitations below |
| New remote two-device Tailscale acceptance test | Not run |
| New application installation / UI acceptance | Not run |

The Rust advisory database was refreshed September 18. Rechecking against this
database with no ignored advisories returned no blocking vulnerabilities. Six
upstream unmaintained-package warnings and the existing informational GLib
`RUSTSEC-2024-0429` warning remain. Registry/yanked-package checks were incomplete
because of timeouts and a stale local registry entry; this is not an entirely
clean supply-chain certification. Hosted CI must run its own full audit.

No Tailscale CLI/LocalAPI queries, VPN/firewall changes, synthetic messages,
production-data resets, or application restarts were made. Building an installer
does not imply installing it. Public release and updater availability require a
successful tagged, signed multi-platform release workflow, not merely a main
branch push. Android publishing remains disabled unless explicitly configured.
