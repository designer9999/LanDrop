# LanDrop audit and 2.0 modernization — October 2026

Scope: the whole application at `18258fd` (1.8.2): Svelte frontend, Rust/Tauri
backend, wire protocol, capabilities, dependencies and tooling. The owner asked for
a full re-audit, current packages, a more intuitive and polished interface, and
messenger features (edit, copy, paste, images, video). Design decisions were
delegated. The design record is
[2026-10-24-chat-redesign.md](design/2026-10-24-chat-redesign.md); user-facing
changes are in the [2.0.0 release notes](releases/v2.0.0.md).

## Findings and disposition

| Area | Finding | Disposition |
| --- | --- | --- |
| Crash | `sanitize_relative_path` sliced at byte 2 whenever the second *character* was `:`; a remote name like `🎉:party.png` panicked the receive session and left the UI believing a receive was active | Fixed with a regression test; sanitizer now only replaces characters no platform can store and prefixes Windows reserved names |
| Availability | After 5 accept errors the listener tried to rebind port 29171 while still holding it, so incoming transfers stayed dead | Fixed: the old socket is released first; accept errors back off |
| Data loss | Files finalized before a batch failed were never reported, leaving untracked files | Fixed: partial batches are reported and marked incomplete |
| Data loss | `receive_folders.json` written non-atomically; a torn write silently reset every per-device folder | Fixed: atomic write-and-rename (also for the alias) |
| Protocol | No shared message identity, no receipts, no version/capability signal; new frame types would break 1.8.x receivers silently | Protocol v2: optional `id`/`reply_to`, `caps` in TXT and identity, `ack` frames, capability-gated `chat` frames. 1.8.x encodings are byte-identical when the new fields are unset (tested) |
| Performance | Media crossed IPC as JSON number arrays (~3–4× size), Android copies twice; videos loaded whole into Blobs; thumbnails re-decoded on every view; text preview read whole files | Asset protocol with per-file scope, range streaming for video, cached thumbnails (JPEG, PNG when transparent), raw IPC bodies, 256 KiB preview cap |
| Performance | Receive wrote per TCP read; per-file send buffers; two writes per control frame; synchronous folder walk on async workers; per-file progress floods | 1 MiB buffered writer, one reused buffer, single-write frames, folder walk on the blocking pool, ~10 Hz batch progress |
| UX | Clicking a bubble copied it; right-click toggled a hidden select mode; no per-message actions; caption text silently stayed behind when sending files; one global progress bar; offline history hidden behind "View all history"; one shared draft retargeted by switching | Replaced by the redesigned conversation UI (see release notes) |
| UX | Lightbox capped images at 800 px | Full-resolution viewer with zoom, pan and gallery navigation |
| Security | `opener:default` + `allow-open-url` let the webview open any URL; pasted images used predictable `/tmp` names | Opener permissions removed (all opening goes through Rust commands); `open_chat_link` accepts only http(s) with a host and no credentials; unpredictable cache paths |
| Architecture | `App.svelte` orchestrated sending, receiving, progress and notifications | Moved into `lib/chat/chat-controller.svelte.ts` with pure, tested reducers in `lib/chat/` |
| Dependencies | Minor/patch updates available across Tauri, plugins, Svelte, Vite, Vitest, ESLint, Rust | Updated (npm and Cargo lockfiles, toolchain pins in CI) |

## Verification performed

- Rust (Windows MSVC, Rust 1.99.1): `cargo fmt --check`, `cargo clippy --locked
  --all-targets --all-features -D warnings`, `cargo test` — 90 passed. New tests
  cover v2 frame compatibility, delivered vs legacy acknowledgement timing,
  negative acks, chat round trips, cancellation with `.part` cleanup, thumbnail
  caching and transparency, preview caps, media scope filtering and link validation.
- Frontend (Node 24.21, Windows): svelte-check 0 errors/0 warnings, ESLint,
  Prettier, Vitest 122 passed, production build. New tests cover message reducers
  (authorisation of remote edits/deletes, ID spoofing, caption/batch merge,
  receipts), timeline grouping and unread divider, rich-text tokenising (no
  `javascript:` links), persistence sanitising, per-conversation drafts.
- `npm audit`: 0 vulnerabilities after transitive fixes.
- UI review in Chrome with a simulated Tauri bridge: wide and compact layouts,
  dark and light themes, context menu, reply and edit modes, image grid, media
  viewer, attachment tray, live send progress, first run and Settings. It found and
  fixed a read-marker effect loop and a pinned-scroll gap.
- Native Windows run (separate dev identity, WebView2 attached over CDP) with a
  protocol-level simulated peer: v2 text, caption + image batch, reply, edit,
  reaction and delete were all acknowledged by the real backend and rendered
  correctly; a 1.8-style message was received without an acknowledgement as
  designed. Raw-body paste, thumbnail caching (100 ms first, 4 ms cached), asset
  protocol loading under the CSP, media scope refusal of non-media files, and
  rejection of `javascript:` links were confirmed in the running app.

## Not verified / remaining work

- **Two real devices.** One PC cannot run two LanDrop instances (fixed port), so
  2.0↔2.0 and 2.0↔1.8 sends from the app to another machine, discovery of caps
  over mDNS, and Tailscale routes were covered by loopback tests and the simulated
  peer, not by a second physical device.
- **Android** was not built in this pass. The frontend is shared and the Android
  commands accept both raw and JSON bodies, but an APK build and emulator pass are
  required before publishing an Android 2.0.
- **P1, unchanged:** no transport encryption or device authentication. Needs its
  own reviewed protocol design (pairing, authenticated transport); nothing in v2
  makes that harder.
- The 5 s acknowledgement-timeout path has no automated test (it would slow the
  suite); the thumbnail cache is never pruned; LAN admission still assumes a /24;
  the finalize-and-rename race noted in September remains.
- TypeScript 7 and the Gradle 9 / AGP 9 / Kotlin 2 Android toolchain are deferred:
  Svelte tooling and typescript-eslint still require TypeScript 6, and the Android
  toolchain should follow Tauri's generated template.
- Package manager: `package.json` declares pnpm, but the lockfile, CI, `dev.bat` and
  Tauri hooks use npm. This pass stayed on npm to avoid an untested CI migration.
