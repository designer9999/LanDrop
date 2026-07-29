# Security policy

## Reporting a vulnerability

If this repository shows GitHub's **Report a vulnerability** button, use that private
reporting form. The maintainer should enable GitHub Private Vulnerability Reporting
in repository settings. Until it is enabled, open a minimal public issue asking the
maintainer to establish a private channel; do not include an exploit, private key,
sensitive file, or identifying network data in that issue.

Include:

- affected LanDrop version and operating system;
- reproducible steps or a minimal proof of concept;
- expected and observed behavior;
- realistic impact;
- any suggested mitigation.

## Current security boundaries

LanDrop is intended for trusted local networks. In the current protocol:

- discovery records and device UUIDs are not cryptographic identities;
- file and text transfers are neither encrypted nor authenticated;
- receivers do not ask for approval before writing an incoming transfer;
- confidentiality and sender authenticity must not be assumed.

Do not expose LanDrop's TCP port directly to the internet. Firewall access should be
limited to private networks.

## Android signing incident

An Android release keystore and its password were committed to the public repository.
That identity must be considered compromised even after removal from the current
branch, because it remains in Git history.

Until a migration is announced:

- do not trust a new APK merely because Android accepts it as an update to an older
  LanDrop APK;
- verify downloads against an independently published hash or repository release;
- do not reuse the exposed key in CI secrets;
- do not publish another Android release without a new application identity or a
  defensible signing-key migration plan.

The technical details and response checklist are in
[`documentation/technical-audit-2026-07.md`](documentation/technical-audit-2026-07.md).
