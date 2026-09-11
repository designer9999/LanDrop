# LanDrop application icon

`landrop-icon.png` is the generated master. The built-in image-generation tool
created it; the image tool selects its model, so no unverified model-version claim
is made. A second built-in edit requested perimeter/alpha cleanup. The result was
copied into this repository before generating the platform assets.

Final generation prompt:

> Create one production square desktop icon for LanDrop, a local and Tailscale
> file-and-message sharing app. Use a bold integrated droplet and two-way transfer
> symbol, readable at small sizes; contemporary Windows Fluent and Material 3
> compatible, softly rounded geometry, subtle layered depth, crisp edges, purple
> brand family and a light contrasting mark. One centered icon, approximately 84%
> canvas coverage, consistent transparent padding; rounded purple tile allowed.
> No words, letters, badges, mockups, frames, watermarks, tiny details, excessive
> glow, or scene background.

Final edit instruction:

> Preserve the tile, white folded droplet, opposing arrows, proportions, and colors.
> Clean only the alpha/background and outer perimeter; remove stray colored
> pixels outside the tile, retain genuinely transparent padding and smooth edges.

Regenerate platform assets with the repository's pinned Tauri CLI:

```sh
npm run tauri -- icon assets/branding/landrop-icon.png --output /path/to/icon-output
```

Copy the desktop PNG/ICO/ICNS outputs to `src-tauri/icons/`, and the 128px PNG to
`public/app-icon.png`. Mobile exports are not automatically copied into Android
or iOS projects. This release is desktop-first and does not change the Android
signing migration requirement.
