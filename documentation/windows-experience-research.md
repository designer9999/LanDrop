# Windows notifications, updates, and application identity

## Executive assessment

LanDrop can provide actionable Windows notifications without replacing its Svelte
interface or changing its LAN/Tailscale transport. The appropriate interaction is
a native system notification that opens a specific conversation, or an update
screen that offers an explicit installation decision. The Windows shell should
own the notification surface, accessibility, positioning, and dismissal behavior;
LanDrop should own the content, navigation, and safeguards against losing work.

The development PC was inspected read-only on September 11, 2026. It runs Windows
11 Pro 25H2, 64-bit, build **26200.9445**. Registry version fields and the Windows
CIM operating-system caption independently establish the version; the legacy
registry `ProductName` string alone is misleading. Microsoft's release table
identifies this build as the September 8 security update, KB5124008. No operating
system update or Insider enrollment is needed to implement the requested
experience.[^1][^2]

There is an important distinction between a current supported implementation and
the newest Microsoft-recommended SDK. Microsoft now recommends Windows App SDK
`AppNotificationManager` for new unpackaged Win32 notification integrations.
LanDrop's implementation instead uses the inbox WinRT toast API through its
existing Rust Windows bindings, with registered protocol activation. This is a
deliberate compatibility decision for its existing Tauri/NSIS distribution, not a
claim that the older API is Microsoft's preferred greenfield architecture.[^3]

## Stable Windows platform and servicing

The relevant production baseline is the stable servicing channel on supported
hardware, not the highest version number mentioned in a preview announcement.
Microsoft lists 25H2 build 26200.9445 and 24H2 build 26100.9445 for the September 8
update. Windows 11 26H1 exists for new devices but is not offered as an in-place
feature update from 24H2 or 25H2. Consequently, moving this development workstation
to 26H1 would not be an appropriate prerequisite for improving LanDrop.[^1]

Windows App SDK and Windows itself have separate release cycles. The SDK download
table lists 2.4.0, released August 13, 2026, as stable at the time of this assessment.
An experimental package is not automatically a better dependency for an
application already distributed to colleagues. A migration must account for
bootstrap initialization, runtime availability, deployment mode, servicing, and
failure handling on machines that lack the runtime.[^4][^5]

The practical recommendation is to keep the workstation on its supported stable
Windows channel, build against the pinned Rust/Tauri toolchain, and test the
installer on an ordinary non-development Windows account. New notification
features do not justify changing global Windows settings, disabling Do Not
Disturb, or broadening firewall rules. None of those operating-system changes
were made as part of this implementation.

## Screenshot assessment

The supplied `Screenshot 2026-09-11 154356.png` shows a LanDrop notification with
an application attribution header, sender name, received-file description, menu,
and dismiss control. The supplied `Screenshot 2026-09-11 154519.png` shows another
application's notification with a more explanatory title and body and a visible
update button. Both visually resemble shell-rendered notifications. Their
appearance alone cannot establish exactly which API or template generated them.

The actionable gap is clear: LanDrop's existing notification helper only supplied
title and body, used one incoming-notification identifier, and had no Windows
conversation-routing handler. The square purple application asset also provided
almost no recognizable identity. A richer message template and a real icon solve
those problems more directly than adding a floating HTML imitation of a Windows
notification.

An exact copy of the reference screenshot's blue translucent background is not a
reliable application requirement. Windows owns its notification surface and
attribution area. Theme, shell version, accessibility settings, and application
identity affect presentation. `appLogoOverride` supplies an image inside the
notification content; it does not replace all Windows 11 attribution branding.
The implementation therefore requests native layout rather than hard-coding a
background color or drawing fake system controls.[^6]

## API and deployment options

| Option | Strength | Limitation | Decision |
| --- | --- | --- | --- |
| Existing Tauri notification plugin | Small cross-platform API; already integrated | Desktop implementation does not expose the complete mobile-style action API | Retain permission-aware fallback outside Windows |
| Inbox WinRT toast plus registered protocol | Uses existing Windows bindings; native buttons and process launch | Legacy compatibility path; installer identity and protocol registration required | Implement for current Windows package |
| Windows App SDK notification manager | Microsoft's current recommended unpackaged Win32 path | Additional runtime/bootstrap and distribution engineering | Planned migration candidate, not silently bundled |
| Custom Svelte notification window | Complete visual control | Not Notification Center; must recreate focus, accessibility, placement, and shell behavior | Do not use as a substitute for native notifications |

Tauri's notification documentation identifies installed applications as the
Windows support context. Development behavior can have different attribution,
and a successful JavaScript call in a development build does not prove installed
branding. The plugin's desktop source forwards a narrower set of fields than its
cross-platform JavaScript type surface suggests. In particular, generic action
examples should not be treated as evidence that Windows desktop supports the
same callback mechanisms as mobile.[^7][^8]

The inbox implementation follows the historical Microsoft unpackaged-app model:
a stable AppUserModelID, a shortcut carrying a toast-activator CLSID, and protocol
activation when no COM activation server is registered. This supports opening
the application from a notification after its original process exits. Inline
reply is deliberately excluded. Microsoft's historical article documenting this
approach is archived; the current notification overview recommends the App SDK
route instead.[^3][^9]

Shortcut metadata requires care. `System.AppUserModel.ToastActivatorCLSID` is a
GUID property, not a string containing braces. The installer hook must write the
correct property type and operate on the actual LanDrop shortcut. Tauri's NSIS
helpers already work with shortcut property stores; using that integration point
is preferable to hard-coding another shortcut location or changing machine-wide
application identity.[^10][^11]

## Implemented notification architecture

`src-tauri/src/notifications.rs` defines two navigation targets: a peer UUID and
the update screen. Peer IDs are parsed, normalized, and rejected if invalid or
nil. Shell input accepts only the exact generated protocol routes. Query strings,
fragments, extra path segments, noncanonical identifiers, and arbitrary URLs are
not interpreted as commands. An update notification cannot supply a download
address, executable path, shell command, or installation argument.

The Windows implementation builds a `ToastGeneric` document. It limits title and
body length, removes unsuitable control/directional characters, and XML-escapes
content. Sender aliases, text messages, and filenames originate outside the
local application and are therefore never inserted into raw XML unescaped. The
notification uses the bundled icon from an application-owned location rather
than trusting a sender-supplied image path.

Incoming peer notifications provide **Open conversation**. Update notifications
provide **View update**. The notification body and its button activate the same
navigation target. Related peer notifications use a stable grouping identity,
and notifications have a bounded lifetime. The implementation does not use alarm
or call scenarios to demand attention for routine messages.

Activation can happen before the WebView has loaded persisted history. Rust
therefore keeps a bounded pending queue and assigns each activation its own ID.
The frontend loads its saved state, subscribes to the activation event, and drains
the queue. Live events are drained too. Deduplication prevents an action received
both through the event and the initial queue from opening twice; draining prevents
old actions being replayed after a later WebView reload.

The update route includes the trailing slash produced by Windows URI
normalization. An actual Windows `Foundation.Uri` round-trip regression protects
this detail: a parser that accepts only the visually similar slashless spelling
would reject the shell's normalized activation.

The existing single-instance handler forwards protocol arguments to the running
application, then reveals and focuses its window. A cold-start activation is
queued during setup. Merely clicking a notification cannot turn a historical
device into an online peer: the conversation may open while its sender is
offline, but the discovery strip continues to contain live devices only and
sending remains disabled for the offline conversation.

Opening a different conversation while a draft exists needs an explicit choice.
LanDrop now offers **Keep editing** or **Discard draft and open**. This avoids
carrying private text or attachments into a different recipient's composer.
Unknown or removed peers do not create a new device entry. Conversation switching
during an outgoing transfer is blocked with an explanation. File notifications
open the conversation rather than executing the received file.

## Notification attention, privacy, and failure behavior

The existing notification preference remains authoritative for incoming message
and file alerts. Foreground incoming traffic does not steal the draft recipient.
The separate existing pop-on-receive preference still controls whether the window
is automatically shown without a notification click. Windows notification sound
is suppressed in the new native path to avoid doubling LanDrop's existing receive
sound. Native update alerts are silent.

Focus and Do Not Disturb can suppress banners while preserving Notification
Center entries. That is expected operating-system behavior, not necessarily an
application delivery failure. LanDrop must not override those preferences to make
an update alert appear. Microsoft's guidance also favors timely, useful,
actionable notifications over redundant status chatter.[^12][^13]

Native notification failures are recorded in the application's diagnostic log.
They do not make a successfully received transfer fail, nor turn a successful
update check into an installation error. The in-app conversation and update
screen remain available if the shell suppresses or rejects a banner.

Message previews can expose content on screen or in Notification Center. This
release retains the existing preview behavior and notification switch; it does
not claim to implement a new privacy mode or per-peer lock-screen policy. A
future preview preference could replace message content with a generic notice
without changing routing. That preference should be designed explicitly rather
than inferred from the icon or theme.

## GitHub update lifecycle

The updater retains the existing GitHub release manifest endpoint and public
verification key. Update transport remains separate from the LAN/Tailscale
transfer protocol. The native notification's protocol arguments cannot select a
different release source. Tauri's updater requires artifact signatures; those
signatures must remain valid against the public key already embedded in installed
clients.[^14]

`src/lib/state/updater-state.svelte.ts` is the single owner of update resources
and phases. A metadata check no longer downloads, installs, or restarts. The
application schedules a check after startup and then every six hours while it
is running; this is polling, not a GitHub webhook or a background Windows service.
An available-version notification is deduplicated within the running session.
Manual checks remain available in Settings.

The update screen identifies the version, displays release notes as text, and
offers **Update now**. Download progress is shown when a total size is known and
falls back to an indeterminate state otherwise. Concurrent checks and installs
are coalesced. Native update resources are closed when replaced, abandoned, or
finished, and failed installation attempts can obtain a fresh resource on retry.

Installation is blocked while outgoing or tracked incoming transfers are active,
or while unsent text or attachments exist. The guard runs before download and
again before installation, because the user can create a draft or receive a file
while an update downloads. Outgoing sends are also refused while downloading or
installing an application update. These are application-level guards, not a
transactional network shutdown: installed-device testing must still cover an
inbound connection arriving at the exact installation boundary.

Immediately before installation or restart, the updater awaits a persisted-state
flush and checks the work-preservation guards again. A failed save prevents exit.
The root application cancels its ordinary debounce timer before that flush so a
newly received history entry is not left waiting in a timer when Windows closes
the process. Notification delivery is independent of the metadata-check lifetime;
a slow shell notification cannot leave an apparently enabled install button inert.

On Windows, launching the updater installer exits LanDrop. The screen explains
this before the user chooses installation. Platforms that require a subsequent
process relaunch get an explicit restart action with the same work-preservation
checks. A notification click itself never authorizes installation. This keeps a
shell protocol—which other local applications can invoke—from becoming an
unattended software-installation interface.[^14]

Publishing a source commit alone does not deliver an application update. The
release workflow must finish the platform builds, upload the signed updater
artifacts, validate `latest.json`, and publish the release. GitHub credentials are
not embedded in LanDrop for users to download public releases. No new release was
published during this local implementation.

## Icon and reusable application identity

The old desktop icon was a plain purple square. The replacement is a recognizable
purple tile with a light folded droplet and opposing transfer arrows. Its shape
connects the application name with two-way file and message exchange. The icon
is an identity asset, not a replacement for the Material Symbols used for
individual interface actions.

The source lives in `assets/branding/landrop-icon.png`. Tauri's icon generator
produces the desktop PNG sizes, Windows ICO, and macOS ICNS. The bundle now
explicitly includes the ICNS file. The tray consumes the embedded application
icon, Windows notification content uses a bundled PNG, and the welcome/About
surfaces and browser favicon reuse the same identity. There is no dependency on a
generated file outside the repository.[^15]

The image-generation prompt and provenance are recorded in the branding folder.
Visual checks should include small sizes on both light and dark backgrounds,
installer/Start-menu attribution, taskbar grouping, and the notification header.
Windows icon and shortcut caches may retain an old image until a properly
installed upgrade refreshes the relevant identity; overwriting a PNG in source
does not prove shell caches have updated.

The supplied Material 3 reference remains the guide for reusable in-app controls,
semantic colors, focus handling, and component spacing. Native Windows surfaces
use Windows conventions. Mixing these responsibilities is appropriate: Material
3 governs LanDrop's document UI, while the operating system governs its shell
notifications and installer integration.

## Validation and release acceptance

Local validation completed with **87 frontend tests**, **47 Linux Rust tests**,
and **47 Windows Rust tests** passing. Svelte type checking, ESLint, Prettier,
production frontend build, Rust formatting, and native Windows Clippy also passed.
The Windows suite includes real WinRT XML and toast-object construction, without
displaying a notification. NSIS installer hooks compiled with warnings treated as
errors. A temporary shortcut fixture also verified that Windows Shell reads the
expected GUID from the toast-activator property; it did not change the registry,
Start Menu, firewall, or installed application. Browser checks with simulated Windows IPC passed the conversation,
offline-history, draft-confirmation, and explicit-update-consent flows. An
additional simulated-IPC browser scenario started an inbound transfer during an
update download: installation stayed blocked until reception finished, then the
explicit install action saved history before invoking the installer. No automatic
restart occurred in the simulated non-exiting installer path.

WSL and native Windows builds sharing one checkout should use separate
`CARGO_TARGET_DIR` locations. Concurrent builds exposed a collision in the
unhashed debug library output, producing mixed-object linker warnings despite
passing Linux unit tests. Final native checks and production packaging were
serialized. This is a shared-local-build-cache constraint; separate GitHub Actions
runner workspaces do not share those output files.

Automated coverage includes navigation to the exact peer, offline-history access
without false discovery, missing-target rejection, draft protection, transfer
blocking, Windows dispatch, Android duplicate suppression, permission denial on
other desktops, activation deduplication, and listener cleanup. Updater tests
cover check-only behavior, concurrency, download progress, safety guards,
post-download rechecks, resource disposal, errors, and explicit restart.

Rust tests cover strict protocol parsing, target validation, bounded activation
queues, escaping, and notification content constraints. Native Windows compiler
checks and installer-hook compilation are distinct from Linux unit tests. A
mocked-IPC browser check can establish that an activation reaches the right
Svelte view, but cannot establish that Windows actually displays the toast or
persists it in Notification Center.

Before public release, complete this installed-Windows acceptance matrix:

| Scenario | Required outcome |
| --- | --- |
| Installed identity | LanDrop name and new icon; no PowerShell or unrelated attribution |
| Background incoming text | Native banner when allowed; correct sender and bounded preview |
| Received file | Conversation action; no automatic file execution |
| Hidden/running app | Click reveals one existing window and opens the exact peer |
| Exited app | Notification activates a new process and routes after history loads |
| Offline sender | History opens; sender remains absent from live discovery; send disabled |
| Different-recipient draft | Keep/discard choice; no silent retargeting |
| Do Not Disturb | Respect shell suppression and Notification Center behavior |
| Available release | View update opens Settings; nothing installs before explicit consent |
| Download failure | Clear retry state; app remains usable |
| Signature mismatch | Installer never runs |
| Transfer/draft during download | Download may finish; installation waits for work to be cleared |
| Existing-client upgrade | Valid signed release installs and restarts using the existing updater key |

Native OS signing is a separate release concern. Tauri updater signatures prove
that a downloaded update matches the configured signing key; they do not provide
Windows Authenticode identity or macOS notarization. SmartScreen reputation and
certificate management remain separate from this notification implementation.
Neither the icon refresh nor a successful local build removes that distinction.[^16]

### Local Windows installer

The final production-mode x64 binary and NSIS package built successfully. The
package includes the final frontend, notification hooks, and explicitly branded
installer/uninstaller icons. Its local path is
`src-tauri/target/release/bundle/nsis/LanDrop_1.7.0_x64-setup.exe`, size 4,554,758
bytes, SHA-256
`ebcfb2a4914d8c0c0052897bef282af3fb896b4131f225af7cc3cde4b87c9481`.

This is an unsigned local test installer: Authenticode reports `NotSigned`, and
no updater `.sig` was produced. A temporary packaging override disabled signing
artifacts for this test only; repository updater signing configuration and the
existing public key were preserved. Neither this installer nor the source changes
were published to GitHub. The installed app and user registry were not modified.
Actual installed toast display, button/body activation, Notification Center
persistence, and cold launch still require the acceptance checks above.

## Sources

All live web sources were checked September 11, 2026. Local evidence consists of
the Windows registry/CIM inspection, the two supplied screenshots, and the source
files identified in the implementation sections. Historical guidance is labeled
as historical rather than presented as the current recommendation.

[^1]: Microsoft. [Windows 11 release information](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information). September 2026 servicing table.
[^2]: Microsoft Support. [September 2026 security update, KB5124008](https://support.microsoft.com/en-us/servicing/os/windows-11/2026/09/kb5124008-windows-11-24h2-25h2-security-update). September 8, 2026.
[^3]: Microsoft Learn. [Windows notifications overview](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/). Current application-type/API recommendations.
[^4]: Microsoft Learn. [Windows App SDK downloads](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads). Stable and experimental channels.
[^5]: Microsoft Learn. [Windows App SDK deployment architecture](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/deployment-architecture).
[^6]: Microsoft Learn. [App notification content](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content).
[^7]: Tauri. [Notifications plugin](https://v2.tauri.app/plugin/notification/). Windows installed-application caveat.
[^8]: Tauri maintainers. [Desktop notification implementation](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/notification/src/desktop.rs). Official source.
[^9]: MicrosoftDocs. [Toast notifications for desktop apps—archived source](https://github.com/MicrosoftDocs/windows-dev-docs/blob/a2477a927c72cdbaa53b73b2cabe2cf71b7c2864/hub/apps/develop/notifications/app-notifications/toast-desktop-apps.md). Historical unpackaged protocol/stub-CLSID model.
[^10]: Microsoft Learn. [System.AppUserModel.ToastActivatorCLSID](https://learn.microsoft.com/en-us/windows/win32/properties/props-system-appusermodel-toastactivatorclsid).
[^11]: Tauri maintainers. [NSIS shortcut helper source](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/utils.nsh).
[^12]: Microsoft Learn. [Toast notification UX guidance](https://learn.microsoft.com/en-us/windows/apps/design/shell/tiles-and-notifications/toast-ux-guidance).
[^13]: Microsoft Support. [Notifications and Do Not Disturb in Windows](https://support.microsoft.com/en-us/windows/experience/notifications-and-do-not-disturb-in-windows).
[^14]: Tauri. [Updater plugin](https://v2.tauri.app/plugin/updater/). Signed artifacts, explicit update operations, and Windows process exit.
[^15]: Tauri. [App icons](https://v2.tauri.app/develop/icons/). Source-image and generated-format workflow.
[^16]: Tauri. [Windows code signing](https://v2.tauri.app/distribute/sign/windows/). Signing identity and SmartScreen considerations.
