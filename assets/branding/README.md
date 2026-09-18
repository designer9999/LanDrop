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

The executable, installer/uninstaller and shortcuts use the bundled ICO. The tray
uses an embedded full-canvas 64px PNG, not a padded preview. Windows toasts and
in-app branding share the same 128px image. Android status notifications use the
matching monochrome speed-folder vector because that surface requires a mask.

The generated ICO includes 16/24/32/48/64/256px frames. Tauri's tray RGBA path uses
one image, so this is not a claim of individual tray frame selection at every DPI.
See [production research and coverage](../../documentation/icon-production.md).
