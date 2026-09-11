# Material 3 component conventions

LanDrop uses its existing Svelte components as the implementation of the supplied
Material 3 Design System reference. The reference folder contains design guidance,
not an npm library. Adding a second widget library would introduce another theme,
accessibility model, and set of component APIs to maintain.

The supplied reference is at
`C:\Users\konoba\Downloads\Material 3 Design System` (available in WSL under
`/mnt/c/Users/konoba/Downloads/Material 3 Design System`). This path is a development
reference only: the application must build and run without that folder.

## GitHub sources

The supplied folder is not a Git checkout. Its `design-tokens.txt` links to
[material-foundation/material-tokens](https://github.com/material-foundation/material-tokens),
which was archived on October 17, 2024. That is a token/DSP reference, not the
source repository for all of the supplied documents or a maintained Svelte widget
library. Google's [Material Web](https://github.com/material-components/material-web)
provides Material 3 web components, but its README currently states that it is in
maintenance mode pending new maintainers (checked September 11, 2026).

Keep the existing reusable Svelte primitives and Material color utilities rather
than adopting either repository as a new runtime dependency. The local reference
guides their visual and interaction conventions; it is not automatically synced
to an identified GitHub repository.

## Component ownership

| Concern | Implementation | Convention |
| --- | --- | --- |
| Dynamic color | `src/lib/theme/` | Generate semantic color roles once; consume the existing CSS variables. |
| Global theme, motion, focus | `src/app.css` | Put shared behavior here instead of repeating it in features. |
| Actions | `src/lib/ui/Button.svelte`, `IconButton.svelte` | Reuse for recurring actions; give icon-only actions an accessible name. |
| Input and selection | `src/lib/ui/TextField.svelte`, `Switch.svelte`, `Slider.svelte` | Keep interaction behavior in the primitive. |
| Confirmation | `src/lib/ui/Dialog.svelte` | Reuse button semantics, Escape dismissal, focus management, and clear action labels. |
| Feedback | `src/lib/ui/Snackbar.svelte` | Temporary action feedback; persistent failures also belong beside the affected feature. |
| Device selection | `src/features/peers/PeerChip.svelte` | A domain component composed from shared controls, with visible route and selected state. |
| Network labeling | `src/lib/utils/peer-utils.ts` | Share route descriptions between chips, conversations, and settings. |

## Rules for new work

Use semantic surface/on-surface and container/on-container pairs. A network route
must have a text label; color alone must not distinguish LAN from Tailscale or
online from offline. Keep identity stable when a route changes: a peer must not
become a second chip just because it has two addresses.

Use native buttons for actions, `aria-pressed` for toggle selection where appropriate,
and visible keyboard focus. Enter on a Cancel button must cancel, not confirm the
dialog. Keep focus inside a modal and return it to its trigger when the modal closes.
Do not add accessibility suppressions to avoid implementing the intended interaction.

Distinguish the visible container from its interaction target. The supplied chip
guidance describes a 32dp container and 48dp interaction targets; a smaller visual
control need not have a smaller hit area. Desktop density may differ, but touch
targets must remain usable on touch and coarse-pointer devices.

Prefer the existing motion tokens and honor reduced motion. Use the supplied
spacing guidance to organize layout rather than inserting unrelated padding values
into every component. Reuse a domain component when behavior and layout repeat;
do not build an abstraction solely because two elements happen to share a color.

## Verification

Check at the native minimum window size (420 × 550), the default size (520 × 720),
and a wider desktop window. Verify keyboard navigation, light and dark themes, long
device names, horizontal peer overflow, no peers, one peer, both network routes,
an offline selected conversation, and discovery failure. Browser checks validate
frontend behavior; native WebView checks remain necessary for platform rendering,
window controls, clipboard, and file dialogs.

## Reference inventory

The relevant supplied files are `design-tokens.txt`, `interaction-states.txt`,
`grids-spacing.txt`, `spacing.txt`, `breakpoints.txt`, `Motion.txt`, and
`whats-new-at-io26.txt`, plus `Components/chips.txt`, `Components/AllButtons.txt`,
`Components/dialogs.txt`, and `Components/Badges.txt`. Their local examples are design
inputs, not evidence that the whole application meets every Material specification.
