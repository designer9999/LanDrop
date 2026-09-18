# LanDrop Windows performance audit

September 18 implementation note: the new server-free Windows discovery provider
has separate [native verification and limitations](documentation/releases/v1.8.0-beta.2.md).
Its one-run provider classification timing is not an application-wide benchmark
or a before/after comparison. The September 11 measurements below still describe
the installed 1.7.1 safety build; they must not be attributed to the beta.

Execution date: September 11, 2026. Source revision: `89c1016507e91bbc9cb114f2fe8b3d65a3361eb6`.
Application: installed 1.7.1 Windows safety build. Mode: **audit-only**.
Research status: **Partially verified**. Verdict: **Audit incomplete** for
application-wide performance; passive native measurements and static review are
complete for the limited scope below. No performance patch or new application
installation was performed during this audit.

## Scope, hypotheses and stop criteria

The immediate concern is recurring background work after the Tailscale incident,
plus the cost of opening saved conversations and attachments. The leading
hypotheses were unnecessary discovery probes, eager media loading, and full-history
persistence work. Competing explanations include ordinary WebView2 rendering,
animations, active transfers, user interaction and unrelated machine load.

The initial measurement contract was passive observation of the existing shipping
process and its WebView2 descendants: cumulative CPU-time deltas, private commit,
handles, threads and generic process I/O counters. Two roughly one-minute windows
would establish an ambient baseline, not startup latency or a leak test. Native
stacks, renderer traces and controlled media workloads are needed to attribute
costs to particular functions. No universal memory or idle CPU budget was assumed.

Stop conditions were process exit/PID replacement, collection failure, or a need
to change the active app, VPN, security settings or production data. The app was
not restarted, navigated, hidden, closed, or sent synthetic messages. No Tailscale
CLI/LocalAPI call or VPN-setting change was made. The supplied core-project-rules
file was not found in this repository; the supplied standalone performance
contract was used. Android was outside this Windows performance scope.

## Environment and artifact

| Item | Observed value or limitation |
| --- | --- |
| Windows | Windows 11 Pro 25H2, build `26200.9445` |
| CPU / RAM | Ryzen 7 5800X, 8 cores / 16 logical processors; approximately 63.9 GiB OS-reported physical RAM |
| GPU / driver | NVIDIA GeForce RTX 3070; Windows driver `32.0.16.1088` |
| Display | WMI reports 2560 × 1080 at 100 Hz; actual frame presentation was not measured |
| Scaling / window | `GetDpiForWindow` returned 120 DPI (125%); visible, not minimized, not foreground at 20:24:19 UTC |
| Power | AC online; High performance power scheme. Effective Windows power-mode overlay not measured or changed |
| Boot / app lifetime | Last boot 16:26:28 UTC; app process started 18:28:19 UTC; measurements around 20:22–20:25 UTC |
| WebView2 | Actual app browser executable reports `152.0.4191.66`, under `Microsoft/EdgeWebView/Application`; not inferred from installed Edge |
| Native toolchain currently on PATH | Rust/Cargo 1.98.1, host `x86_64-pc-windows-msvc`, LLVM 22.1.8 |
| Windows JavaScript tools | Default Node 26.0.0, npm 11.12.1, pnpm 12.4.1; project requires Node 24.x |
| Isolated Windows Node used for analysis | Existing `pp-node-24/node-v24.18.0-win-x64/node.exe`; no toolchain upgrade performed |
| Frontend | Svelte 5.57.0, Vite 8.3.0, TypeScript 6.0.3, Tailwind 4.3.3; SPA, not Next.js or SSR hydration |
| Tauri | Rust 2.11.5, build crate 2.6.3, runtime-wry 2.11.4, Wry 0.55.1; JS API 2.11.1; declared CLI 2.11.4 |
| Tokio / discovery | Tokio 1.53.1, mdns-sd 0.21.3 |
| Native plugin lock versions | Clipboard 2.3.3; dialog 2.7.3; fs 2.5.2; global shortcut 2.3.2; notification 2.4.0; opener 2.5.5; process 2.3.1; single instance 2.4.4; store 2.4.4; updater 2.11.0; window state 2.4.1 |
| Source release profile | `opt-level="s"`, thin LTO, one codegen unit, symbol stripping, unwind panic strategy; default feature `desktop` |
| Historical build provenance | Installed hash matches the recorded safety artifact. Exact historical compiler environment/resolved feature graph was not recaptured in this audit |
| Saved-state size | Aggregate read-only count: 67,574 bytes; 78 messages, 4 historical devices, 47 top-level attachments including one video; 100 stored folder children. No message contents or attachment paths recorded |
| Workload control | Real existing app/profile, not an isolated benchmark fixture. Visible route, peer count, user activity, network conditions and competing load not continuously observed |

Installed executable: `%LOCALAPPDATA%\LanDrop\landrop.exe`, **15,315,968 bytes**.
SHA-256: `601c422f68a4c4eff75f452d799f3dc51701d0c8358a1d7c6f07ff9e2041cf3a`.
Existing NSIS installer: **4,541,959 bytes**, as recorded in the
[safety installation record](documentation/tailscale-incident-2026-09.md).
Executable bytes are not total installed size; installer bytes are not runtime
memory. Build/install duration was not measured this pass and is not startup time.

The package manifest declares pnpm, but the tracked application lockfile and
launcher use npm. `dev.bat` can automatically run `npm install` when Windows native
bindings are absent. Those bindings are absent in the current shared dependency
tree, so that launcher was not run. A `pnpm --version` bootstrap generated a
package-manager-only `pnpm-lock.yaml`; it was moved recoverably to the diagnostic
directory, not accepted as an application dependency migration. Tracked manifests
and both tracked lockfiles remain unchanged.

## Passive Windows baseline

[Measure-LanDropProcesses.ps1](scripts/perf/Measure-LanDropProcesses.ps1) uses
Windows PowerShell 5.1, CIM and process counters. It follows the selected LanDrop
PID and recursive `msedgewebview2.exe` descendants, not all WebView processes on
the machine. The observed stable group contained seven processes: native app,
browser, renderer, GPU process, network utility, storage utility and crash handler.
Role classification was separately extracted as allowlisted fields; raw command
lines were not retained.

Sampling used a monotonic stopwatch and an approximately five-second pause between
snapshots, after one 10-second collector smoke check. CPU percentages below are
`100 × delta CPU seconds / elapsed seconds / 16`, so 100% means all logical
processors, not one core. Snapshot-start timestamps precede synchronous counter
reads; collection is not atomic, and percentages are approximate. No reliable
tail percentile is claimed from twelve intervals per run.

| Workflow | Metric / direction | Before: run A | Repeat baseline: run B | Samples / spread | After / change |
| --- | --- | --- | --- | --- | --- |
| Ambient running app | Whole-group CPU, machine-normalized %, lower | Median 0.60% | Median 0.54% | 12 intervals each; A 0.38–0.75%, B 0.35–0.69% | Not measured; no patch |
| Same | Whole-group private commit, MiB, lower with correctness preserved | 200.34–201.18 | 200.32–200.39 | 13 snapshots each | Not measured; no patch |
| Same | Native-app CPU consumed over window, seconds | 0.125 | 0.297 | One cumulative delta per run | Not a before/after comparison |
| Same | Whole-group handles | 3,645–3,661 | 3,645–3,663 | 13 snapshots each | No sustained growth conclusion |
| Same | Whole-group threads | 212–213 | 212–213 | 13 snapshots each | No sustained growth conclusion |
| Same | Observed window duration, seconds | 60.31 | 60.17 | One window each | Similar, not identical conditions |

Most sampled CPU time was outside Rust: the GPU **process consumed CPU time** of
4.17 and 3.64 seconds, and the renderer 1.30 and 1.31 seconds. These are not GPU
engine utilization percentages. They challenge a claim that native LAN scanning
necessarily dominates this particular ambient workload. Renderer/compositor
activity needs a controlled trace before calling it excessive or changing visuals.

The renderer and GPU process also had roughly matching 98.8/98.6 MiB write/read
counter deltas in the two runs. These are generic process I/O counters and may
include IPC; they do **not** establish 99 MiB of disk writes, network traffic, or
file transfer. Private commit is not unique resident physical RAM or JavaScript
heap. Shared working sets were deliberately not summed into an “app RAM” claim.

No process/start-time changes were observed within either run. A short stable
window does not prove absence of leaks. PID reuse between CIM and process reads,
shared/reparented WebView processes, sampling overhead and ambient system activity
remain attribution limitations. Observer overhead was not separately calibrated.
No new debugger endpoint, test plugin, forced garbage collection or elevated trace
was enabled. A read-only UIAutomation check returned zero descendant elements, so
it did not establish UI readiness or identify the displayed route.

### Raw evidence and reproduction

Raw JSON remains local under
`%LOCALAPPDATA%\LanDrop\Diagnostics\Performance`, not in Git or an external upload.
It contains hardware/process metadata, including the local executable path, but
not message contents, credentials or full command lines.

| Record | SHA-256 |
| --- | --- |
| `processes-20260911T202217790Z-6c87ffe4.json` — 10-second smoke | `2f4bfa69b6b33562a093f8e091f570b6ce7ac7e9869d000dc2c75b2a16bb0833` |
| `processes-20260911T202306901Z-517dc00e.json` — run A | `7a9984dd9ee618ae9367a951493c38aa17d033812c225fc54ea439303bc0bb42` |
| `processes-20260911T202408555Z-8e702d2b.json` — run B | `6089f3759778ec3ce1b368485f8f76d5dfe1cdd1ea313fcb5bb6c12e2db506c8` |

Collector SHA-256 at capture:
`17bbce068cd2efa890cfc13b3ce125feabd27dddcc5b61da2446e79f8f6c63ab`.
The archived bootstrap lock is `pnpm-bootstrap-lock-20260911T2021.yaml` in the same
directory. No original project lockfile was deleted or replaced.

To repeat a passive capture, resolve the current LanDrop PID first and use it
instead of the historical example below. The collector neither launches the app
nor controls its UI. Keep machine/runtime, visible route, history, cache category,
power conditions and background workload comparable before comparing runs.

```powershell
.\scripts\perf\Measure-LanDropProcesses.ps1 -AppProcessId 3600 -DurationSeconds 60 -IntervalSeconds 5
```

## Evidence-linked findings

### F1. Whole-file video acquisition and non-hard cache budget

Priority: high for media-heavy histories. Evidence: **Code-confirmed**, with the
cache behavior **Reproduced** in Windows Node, not in a WebView memory trace.
[VideoAttachment](src/features/transfer/VideoAttachment.svelte) starts acquisition
when mounted, before hover. [The bridge](src/lib/api/bridge.ts) receives whole-file
bytes, creates a typed array/copy and a Blob; [the native reader](src-tauri/src/commands/fs_info.rs)
uses `std::fs::read`. `preload="metadata"` cannot undo that preceding whole-file
read. All displayed history messages mount in [ChatArea](src/features/transfer/ChatArea.svelte).

[VideoLruCache](src/lib/api/video-cache.ts) evicts only unreferenced entries over
the 512 MiB threshold. Referenced entries can exceed it, as an existing test also
explicitly expects. A Windows Node 24.18.0 probe inserted two referenced 60-unit
entries into a 100-unit budget: 120 retained, zero evicted. This checks a data
structure contract, not a measured 512 MiB app limit or a leak. The first
strip-only invocation could not run TypeScript parameter properties; the repeated
probe used Node's documented experimental transform flag and passed.

Recommended experiment: scoped file streaming/range access and visibility-aware
acquisition while preserving hover playback, seeking and access validation.
Measure actual renderer/native private commit, acquisition bytes and visible
preview latency using isolated representative media. Do not evict an active Blob
under a playing video or broaden filesystem permissions as a shortcut.

### F2. Thumbnail decode admission is not globally bounded

Priority: high for large image histories. Evidence: **Code-confirmed**, peak cost
unmeasured. ChatArea starts candidates immediately, up to 200 recent-history
images plus composer images. Each native thumbnail request uses `spawn_blocking`
and full decoding. Per-image dimension/allocation limits are valuable but do not
bound aggregate concurrent work. Cache updates can also retrigger candidate scans.

Propose a small bounded queue and obsolete-request handling, preserving decoder
limits and error behavior. Verify simultaneous IPC requests, blocking threads,
peak commit and time to visible thumbnails. A frontend timeout alone would not
cancel an already started blocking decode.

### F3. Small text previews still acquire complete files

Priority: medium/high for large logs. Evidence: **Code-confirmed** in
[`read_file_preview`](src-tauri/src/commands/fs_info.rs). It reads the entire UTF-8
file and collects every line before retaining 200 lines. Recognized text types
have no input-size ceiling, and a single enormous line is still enormous.

A streaming implementation could reduce temporary allocations while preserving
exact total line count and whole-file UTF-8 error semantics. Those semantics still
require examining the rest of the file: stopping after 200 lines or adding a
preview byte cap is a behavioral decision, not a free equivalent optimization.
Benchmark a representative large log and long-line file before choosing a change.

### F4. Startup existence checks recursively enumerate saved folders

Priority: medium for populated history/slow storage. Evidence: **Code-confirmed**.
[`repairStoredMessagePaths`](src/App.svelte) sequentially invokes `getFileInfo` to
check existing received paths; folder metadata recursively walks descendants and
sums their sizes. Repair runs after discovery starts but before the later hotkey
setup call. This is not evidence that the first window waits for every traversal.

Propose a narrowly validated existence/metadata-only operation for this caller,
without changing the full folder-info command used by the composer. Verify
metadata calls, filesystem traversal, discovery readiness and hotkey readiness
separately with isolated missing/existing folder fixtures.

### F5. Persistence preparation happens before the debounce

Priority: medium for bursts and large history. Evidence: **Code-confirmed**.
[App.svelte](src/App.svelte) exports/sanitizes and JSON-stringifies the entire
snapshot before its 200 ms save timer. Device spreading in
[sanitation](src/lib/persistence/sanitize.ts) also reads live fields later stripped
from persistence, allowing rediscovery to schedule work even when saved JSON is
unchanged. [Store persistence](src/lib/persistence/app-store.ts) sends the full
snapshot and explicitly saves it. The installed store plugin's explicit save
cancels pending autosave; this audit does not claim automatic duplicate writes.

Propose measuring snapshot preparation separately from disk I/O, then moving only
safe work behind coalescing or narrowing tracked persistence inputs. Retain exit
flush, validation, durability and latest-state correctness. No serialization or
disk-write improvement is measured yet.

### F6. LAN recovery and liveness perform overlapping work

Priority: medium investigation target, not the measured dominant CPU source.
Evidence: **Code-confirmed** in [discovery.rs](src-tauri/src/lan/discovery.rs).
The healer traverses approximately 253 `/24` addresses every 30 seconds with
16 concurrent, 700 ms probes, even if no peers exist or mDNS works. Nominally this
is 30,360 attempted connections/hour under stable timing and a valid LAN, not an
observed packet count. Known routes are separately checked every 15 seconds and
mDNS resolutions trigger verification. Interface enumeration repeats during
admission and send-route checks, including while the peer-map lock is held.

Candidate changes are sharing sufficiently recent verification evidence and using
one interface snapshot per operation. Reducing scan frequency can increase
discovery recovery time; caching admission across network changes can weaken
correctness. Neither should be changed without timing, stale-route and network-
change tests. The disabled Windows tailnet worker still wakes every 30 seconds,
but performs **zero Tailscale CLI/LocalAPI calls**; its small allocations are a
different issue from the LAN scan.

### F7. Large-folder preparation and tiny-file progress need workload tests

Priority: medium. Evidence: **Code-confirmed** in
[transfer.rs](src-tauri/src/lan/transfer.rs). Outgoing enumeration synchronously
collects entries on an async worker before enforcing the 100,000-file cap, then
awaits metadata serially. Many-file receives reset progress throttling per file
and emit completion updates per file. The throttle is time **or** byte progress,
not a strict 10 Hz maximum.

Propose early enforcement of the existing entry cap, bounded preparation offload
and measured progress coalescing that retains start/error/final events. Preserve
symlink rejection, overwrite protection, byte totals and cancellation semantics.
File payloads already stream through 256 KiB buffers; they are not whole-file
loads like the video-preview path.

### F8. Late preview results can outlive the interaction

Priority: medium correctness/lifetime issue; performance impact unmeasured.
Evidence: **Code-confirmed** in ChatArea's `openLightbox` and `openFilePreview`.
Results lack a latest-request/disposal check, so overlapping or closed preview
operations may install stale results. Reproduce with controlled delayed reads
before claiming a visible reopen or retained-object leak.

A request generation/disposal guard is a narrow candidate. It prevents stale
publication but is not cancellation of native I/O. Main listener registration and
video tile loading already handle late disposal; those safeguards should remain.
Closing the desktop window intentionally hides it to the tray, rather than
destroying the WebView or stopping receive work.

## Coverage and next gate

| Check | Status | Meaning |
| --- | --- | --- |
| Installed version, path and safety-build hash | Passed | Existing 1.7.1 confirmed; not a new installation |
| PowerShell 5.1 parser, wrong-PID guard, 10-second smoke | Passed | Collector usable without app/VPN control |
| Two 60-second native ambient captures | Passed | Limited baseline above; no before/after speedup |
| Video-cache contract probe on Windows Node 24 | Passed after runner correction | Pure class behavior only, not WebView2 rendering |
| App/source manifests and tracked lockfiles unchanged | Passed | No dependency or toolchain migration |
| Native UI readiness via existing UIAutomation surface | Blocked | Zero descendant elements; no readiness assertion |
| Cold/warm launch and action-to-paint timing | Not run | App not restarted; no isolated profile/markers prepared |
| Heavy media, repeated navigation, leak/retention testing | Not run | Would need isolated representative fixtures and native renderer capture |
| GPU engine/frame-time traces, native stacks | Not run | No elevated ETW capture or debugger session authorized/enabled |
| Long active/idle, sleep/resume, loss/retry, two-PC transfers | Not run | Outside this passive baseline; existing VPN left alone |
| New release build, signing, GitHub publishing, installation | Not run | No performance patch or approved discovery implementation to ship |
| Android profiling | Not applicable | Windows-only scope |

WPR and WPA are installed, but availability is not permission to start elevated
tracing. Current Tauri WebdriverIO or local-only WebView2/CDP automation can support
a later isolated test setup. Neither test plugins nor debugger endpoints belong
in the shipping build. Existing 1.7.1 correctness-test results are recorded in the
incident report, not reclassified as tests executed by this performance pass.

Next gate: authorize a narrowly scoped performance-fix pass, choose one
representative workload and prepare an isolated Windows-native test environment.
Preserve the approved UI and behavior, baseline that workload, apply the smallest
supported change and repeat it. Automatic Tailscale presence remains a separate
[proposed feature](documentation/tailscale-discovery-design.md) requiring the
directory/setup decision. This audit does not approve its deployment or remove
the Windows safety guard.

## Official guidance and applicability

- [WebView2 process model](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model): process-group/user-data-folder attribution; actual executable version was inspected. [Evergreen versus fixed runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version) and [release notes](https://learn.microsoft.com/en-us/microsoft-edge/webview2/release-notes/) reviewed; runtime update/security certification is not claimed.
- [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html): installed profile matches the documented size/optimization/symbol tradeoffs. No compiler-profile experiment or `target-cpu=native` change was made.
- [Tokio 1.53.1 blocking tasks](https://docs.rs/tokio/1.53.1/tokio/task/fn.spawn_blocking.html), [versioned interval implementation](https://github.com/tokio-rs/tokio/blob/tokio-1.53.1/tokio/src/time/interval.rs), and [shutdown guidance](https://tokio.rs/tokio/topics/shutdown): bounded admission and cancellation distinctions apply to the installed version. The versioned interval docs endpoint failed, so versioned source supplied the check.
- [Tauri frontend communication](https://v2.tauri.app/develop/calling-frontend/) and [Tauri 2.11.5 release](https://github.com/tauri-apps/tauri/releases/tag/tauri-v2.11.5): relevant IPC/version context. No channel migration or backpressure guarantee assumed.
- [Svelte effects](https://svelte.dev/docs/svelte/$effect), [Svelte 5.57.0 release](https://github.com/sveltejs/svelte/releases/tag/svelte@5.57.0) and [Vite 8 migration](https://vite.dev/guide/migration): reactive dependencies and Rolldown/Oxc apply; React/Next.js hydration advice does not.
- [Tauri origin-confusion advisory](https://github.com/tauri-apps/tauri/security/advisories/GHSA-7gmj-67g7-phm9): published May 6, 2026, affected through 2.11.0, patched from 2.11.1. Locked Tauri 2.11.5 is outside that affected range. This selected check is not a comprehensive dependency/advisory audit.
- [Windows Performance Toolkit](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/), [Tauri native automation](https://v2.tauri.app/develop/tests/webdriver/) and [Playwright WebView2](https://playwright.dev/docs/webview2): available later measurement paths, not tools claimed to have captured traces here.
- [GetDpiForWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow): DPI depends on window awareness; the actual existing window returned 120.
- [Node release lifecycle](https://nodejs.org/en/about/previous-releases) and [Node 24 TypeScript support](https://nodejs.org/docs/latest-v24.x/api/typescript.html): distinguish the Windows PATH mismatch from the deliberately selected analysis runtime and its experimental transform flag.

Sources checked September 11, 2026. Research is sufficient for these mechanisms
and measurement limits, not a claim that every installed dependency, runtime,
platform workflow or security advisory was exhaustively requalified.
