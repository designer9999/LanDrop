# LanDrop application icon

`landrop-icon.png` is the orange/cream speed-folder master supplied and approved
by the owner on 2026-09-18. The source picture contained an opaque checkerboard.
The built-in image-generation skill prepared genuine alpha and the edge-to-edge
crop demonstrated by the owner's second screenshot. This is an image-tool edit,
not a claim of pixel-identical extraction. No separate logo was designed.

Final accepted preparation prompt:

> Use the original approved orange/cream speed-folder artwork and the owner's
> tight-crop screenshot. Zero outer margin: the orange rounded-square tile touches
> all four canvas edges at their midpoints. Only the rounded corners are truly
> transparent. Preserve the folder, speed lines, colors and grain; no checkerboard,
> extra border, shadow, padding, particles or redesign.

The tool selects its model; no unverified model-version claim is made. Earlier
padded/extraction attempts were rejected and are not application assets.

## Reproduction

Run with Windows Node and the project's installed pinned Tauri CLI:

```powershell
powershell -NoProfile -File scripts/Update-LanDropIcons.ps1
```

If the Windows dependency tree lives in a separate native staging project,
pass `-CliProjectPath <Windows-project-path>`. The script checks the master,
generates in an isolated temporary directory, preflights all expected outputs,
and updates desktop PNG/ICO/ICNS, the in-app PNG, and existing Android launcher
PNGs. Intermediate output is retained; signing and unrelated native configuration
are untouched.

The pinned Tauri CLI 2.11.4 has an upstream Android HDPI table error: it emits
49px launcher images instead of the required 72px (48dp at 1.5×). The script repairs
those two variants by using the same CLI to downsample its correctly masked 192px
variants, and checks all 15 Android PNG dimensions before copying. To repair only
those two Android files, use `-AndroidDensityRepairOnly`. No artwork is redesigned
and desktop assets are not touched in that mode. A CI test guards the density map.
See the [version-matched generator source](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-cli/src/icon.rs#L393).

Android 8/API 26 and later use native adaptive-icon XML instead of letting the
launcher shrink the legacy rounded-square bitmap inside a white circle. The
existing orange background fills the launcher mask; a 108dp vector foreground
reuses the status/TV speed-folder geometry in cream, inside the central safe zone.
API 33 adds a monochrome layer for user-enabled themed icons. Older Android keeps
the generated PNG fallback. The icon generation script deliberately preserves
these native XML resources; CI guards their references and shared geometry.
This is the existing logo adapted to the platform, not a new raster/logo design.
See [Android's adaptive-icon guidance](https://developer.android.com/develop/ui/compose/system/icon_design_adaptive).

The Android TV launcher uses a scalable `tv_banner.xml` with the same speed-folder
geometry and an outlined LanDrop wordmark, matching Android's 320×180 xhdpi banner
requirement. This supplies the existing TV launcher's missing metadata; it is not
a claim that TV remote/D-pad navigation has been tested.

The executable, installer/uninstaller and shortcuts use the bundled ICO. The tray
uses an embedded full-canvas 64px PNG, not a padded preview. Windows toasts and
in-app branding share the same 128px image. Android status notifications use the
matching monochrome speed-folder vector because that surface requires a mask.

The generated ICO includes 16/24/32/48/64/256px frames. Tauri's tray RGBA path uses
one image, so this is not a claim of individual tray frame selection at every DPI.
See [production research and coverage](../../documentation/icon-production.md).
