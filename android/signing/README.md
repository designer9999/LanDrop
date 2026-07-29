# Android signing

No Android signing key belongs in this repository.

The release workflow reads the following GitHub Actions secrets:

- `TAURI_SIGNING_PRIVATE_KEY`: private Tauri updater-signing key
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: updater-key password
- `ANDROID_KEYSTORE_BASE64`: base64-encoded release keystore
- `ANDROID_KEYSTORE_PASSWORD`: keystore password
- `ANDROID_KEY_ALIAS`: signing-key alias
- `ANDROID_KEY_PASSWORD`: signing-key password
- `ANDROID_EXPECTED_CERT_SHA256`: expected signer certificate SHA-256 fingerprint

Store all of these as **environment secrets** in a GitHub environment named
`release`; delete any repository-level duplicates. Protect that environment with
required reviewers. Separately, add repository rulesets that restrict creation and
updates of `v*` tags.

Android publishing is intentionally skipped unless the repository variable
`ENABLE_ANDROID_RELEASE` is exactly `true`.

The key previously committed to this repository must be considered compromised. Do
not put that key into these secrets. Existing APK installations cannot safely trust
ordinary updates signed by the exposed identity. Follow the
[incident and migration plan](../../documentation/technical-audit-2026-07.md) before
publishing another Android release.
