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

Android publishing is skipped unless the repository variable
`ENABLE_ANDROID_RELEASE` is exactly `true`. Enable it only after the replacement
identity's release qualification is complete. Do not enable it while an older
tagged release workflow is still pending its Android job.

The key previously committed to this repository must be considered compromised. Do
not put that key into these secrets. Existing APK installations cannot safely trust
ordinary updates signed by the exposed identity. Follow the
[incident and migration plan](../../documentation/technical-audit-2026-07.md) before
publishing another Android release.

## Replacement identity (September 18, 2026)

The owner approved a separate Android application, `io.github.designer9999.landrop`.
The desktop identifier remains `com.landrop.app`; desktop updater trust and saved
data are unaffected. Android 1.8.2 starts the replacement identity. It does not
overwrite the old Android installation or automatically import its history.

Fresh release certificate SHA-256:

```text
ab80892384f01e61d3a1c6b9387f9c65111b46bccad383f254415c5f9e9c0c03
```

The private RSA-4096 key was generated with Java 21 `keytool`, in a password-
protected PKCS#12 keystore outside this repository. Local custody is under
`%LOCALAPPDATA%\LanDropRelease\Android\io.github.designer9999.landrop\signing`.
The directory ACL permits only the current Windows user and SYSTEM. The randomly
generated password is protected with Windows DPAPI CurrentUser, not a plaintext
environment file. `scripts/Android-Signing.ps1` refuses to overwrite an existing
vault and can sign an explicitly selected, aligned APK. Initialization must not
be repeated to produce a different key for subsequent updates.

All five Android secrets are now configured in GitHub's `release` environment;
the three superseded repository-level Android secrets were removed. No desktop
signing secret was changed. The workflow pins Android Build Tools 36.0.0, checks
the APK identity/version/ABI, verifies the selected certificate, and explicitly
rejects the publicly exposed old certificate. An APK always needs a valid Android
signature, even when it is distributed directly on GitHub rather than Google Play.

### Recovery responsibility

The DPAPI password file is tied to this Windows account. Copying it to another
computer is **not** a portable backup. GitHub secrets cannot be read back through
the API. Before reinstalling Windows or deleting this account, the owner must
export the key and password into a trusted encrypted backup/password manager.
No independent portable backup or required-reviewer/tag protection is claimed by
this setup. Never put a decrypted password or keystore into Git, chat, logs, APK
assets, or a repository `.env` file.

References: [Android signing](https://developer.android.com/studio/publish/app-signing),
[Java 21 keytool](https://docs.oracle.com/en/java/javase/21/docs/specs/man/keytool.html),
[Windows DPAPI](https://learn.microsoft.com/en-us/dotnet/api/system.security.cryptography.protecteddata),
[GitHub environment secrets](https://docs.github.com/en/rest/actions/secrets#create-or-update-an-environment-secret).
