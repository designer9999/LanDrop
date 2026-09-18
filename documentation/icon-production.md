# LanDrop icon production review

Reviewed 2026-09-18. Scope: the owner's orange/cream speed-folder artwork,
Windows desktop/installer/tray use, and matching existing platform assets.
This is a production plan, not a claim that the replacement is installed.

## Findings

- The supplied 1254 × 1254 PNG is RGB, not RGBA. Its checkerboard is actual
  picture content. It must not be shipped as if that were transparency.
- Its tile occupies roughly 84% of the canvas width. Simply scaling the whole
  picture to 16 px leaves the tile approximately 13.5 px wide; the folder itself
  is smaller still. The broad outer border serves an illustration preview, not
  a compact Windows notification-area asset.
- Microsoft lists 16, 20, 24, 32, 40, 48 and 64 px notification-area/title-bar
  sizes across 100–400% scaling. Windows prefers an exact size and otherwise
  downsamples a larger image. The documented minimum app ICO sizes are 16,
  24, 32, 48 and 256 px. See [icon construction][construction].
- Microsoft recommends a simple, balanced silhouette with readable details at
  small sizes, and checking light/dark backgrounds. A single reduced textured
  illustration is not sufficient evidence of small-size legibility. See
  [icon design][design].
- Windows controls the notification-area slot and surrounding spacing. An app
  can improve the pixels inside its icon, not legitimately claim the whole tray
  hit target or eliminate Explorer's spacing. See [notification area][tray].
- LanDrop currently passes `default_window_icon()` to its Tauri tray builder.
  The installed `tray-icon 0.24.2` RGBA path creates one HICON; it does not load
  every frame from the executable ICO. Merely adding ICO sizes does not prove
  DPI-perfect tray selection. A dedicated embedded tray image is appropriate;
  actual shell rendering still needs verification. See [Tauri tray][tauri].

## Production treatment

Keep the approved orange/cream speed-folder identity. Remove the preview border
and provide genuine transparent rounded corners. Fit the orange tile tightly
inside each intended canvas, without cutting antialiased edges. Do not distort
the horizontal folder to fill both dimensions.

The owner's follow-up screenshots clarified that the orange tile must fill the
whole square, with no outer border. The selected master uses that tight framing.
The folder proportions and texture remain, rather than introducing an alternate
small-size logo. Small-size derivatives still need visual checks for readable
gaps, strokes and contrast.
Use a multi-frame ICO for the executable/installer and a separate embedded tray
asset. Maintain PNG/ICNS, in-app and toast assets from the same master. Android
status notifications require the monochrome symbol rather than an opaque orange
square; their native validation remains separate from desktop.

Before shipping, inspect 16/20/24/32/48/64 px renders on light and dark surfaces,
verify actual alpha and occupied bounds, inspect ICO frame dimensions, compile
the Windows release, and check the installed tray/taskbar/shortcut/notification
surfaces. Do not clear the user's Explorer icon cache or restart Explorer as a
routine asset-generation step. Previously delivered notifications may retain
their old imagery; newly delivered notifications use the new bundled asset.

## Current status

The first three image-generation background-extraction outputs were rejected.
After the owner's explicit framing screenshots, the built-in image tool produced
the selected full-canvas master. It is an image-tool edit, not pixel-identical
masking of the original. Its measured alpha >=128 bounds cover 0,0–1253,1253;
the 32px export covers 0,0–31,31. Master top-left alpha is 0; small-export corner
alpha is 3 (antialiasing), not an opaque checkerboard. The ICO contains
16/24/32/48/64/256px 32-bit frames. These are asset measurements, not a visual
acceptance test of Explorer at every DPI.

All desktop assets and existing Android launcher images have been regenerated.
The tray explicitly embeds the new 64px PNG; native toast and frontend assets
share the 128px image. Old Android template artwork is replaced and its status
notification vector follows the same speed-folder identity. Two Rust regression
tests check full-canvas tray coverage/transparent corners, shared in-app/toast
artwork, and the ICO size inventory. Build/test results are recorded below when
complete. Executable/installer resource verification is distinct from visual
acceptance of every Windows shell surface. New-icon publication is not claimed.

The in-progress v1.8.0 release predates this icon request and retains its fixed
tagged artwork. Icon changes belong in a subsequent version, not a moved tag or
silently replaced published installer.

## Verification

- **Passed:** Windows Rust MSVC 1.98.1 formatting, strict Clippy, and all-target /
  all-feature tests: 66 passed, 3 opt-in network diagnostics ignored.
- **Passed:** Windows Node 24.18.0 frontend verification: zero Svelte diagnostics,
  lint, formatting, 87 tests, optimized Vite build.
- **Passed:** generation script executed with pinned Tauri CLI 2.11.4; PowerShell
  syntax check and the three changed Android XML resources parsed successfully.
- **Passed (asset inspection):** 16/24/32px renders show the full-canvas tile and
  recognizable cream folder, with no checkerboard margin. In-app and native toast
  128px PNGs have identical SHA-256.
- **Not run:** Android native build/device test; macOS/Linux installed-icon visual
  checks; exhaustive Windows multi-monitor/DPI/theme checks.
- **Passed:** local 1.8.1 optimized Windows NSIS build (Rust release compilation
  3m31s, not an app startup measurement). Installer exit code 0 after the owner
  explicitly authorized closing the old app and installing the icon build.
- **Passed:** installed executable metadata reports 1.8.1; its bytes match the
  local release binary with only Tauri's documented `UNK` → `NSS` bundle-type
  marker applied. A naive unbundled/installed hash equality check initially
  stopped verification; a byte comparison and `tauri-utils 2.9.3` source resolved
  that expected packaging difference. No executable modification was performed.
- **Passed:** Windows extracted orange icons from the built executable, installer,
  installed executable and installed uninstaller. Start Menu/desktop shortcuts
  target the installed executable. The reopened process reports 1.8.1/responding.
- **Not verified visually:** the actual tray/taskbar and app pages after installation.
  An attempted screen capture was occluded by another foreground application and
  was not accepted as evidence. Explorer was not restarted or its cache cleared.

Local installer: `src-tauri/target/release/bundle/nsis/LanDrop_1.8.1_x64-setup.exe`
(4,784,040 bytes), SHA-256
`db1159211d397f88593d970866e3fb2d2856fc3a9a8c96e744c8369d41895436`.
Installed executable SHA-256:
`ffd3c94dd91a7b874b4e12395e6b21aff1efbbddc1bbe9e09d649cd3cb3aa5e7`.
The local build used an external validation-only config disabling updater signing;
it is not a signed public update artifact. The repository's release configuration
and signing public key remain unchanged.

Master SHA-256: `e105ebcb9c820ba5471f25883964e01ec4829f0e020e322cff323d89108c61f1`.

[construction]: https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction
[design]: https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-design
[tray]: https://learn.microsoft.com/en-us/windows/win32/shell/notification-area
[tauri]: https://v2.tauri.app/learn/system-tray/
