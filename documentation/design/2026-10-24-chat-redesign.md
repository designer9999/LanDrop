# LanDrop 2.0 — chat protocol, media pipeline and interface redesign

Date: 2026-10-24. Owner direction: "re-audit everything, make it more intuitive,
faster and more polished; editable chats, copy, edit, paste, images and videos."
The owner delegated product and design judgement for this pass.

LanDrop is used between the owner's own devices and a colleague's devices. Edits and
deletions therefore follow familiar messenger rules (Telegram/WhatsApp), not a
single-user notebook model.

## Goals

1. Messages become shared objects: a sender-generated ID lets both sides agree on
   edits, deletions, reactions and replies.
2. Sending feels instant: optimistic bubbles with per-message state
   (sending → sent → delivered, or failed with Retry) instead of blocking the composer.
3. Text and attachments are one message (caption + files), with per-message progress
   and cancellation instead of a single global progress bar.
4. Media is served to the webview by the asset protocol and cached thumbnails, not
   by JSON number arrays or base64 strings.
5. A clearer Material 3 Expressive interface: adaptive two-pane layout on wide
   windows, conversation header, message context menu and hover actions, reply/edit
   banners, date separators, unread markers and a jump-to-latest button.
6. Remain wire-compatible with LanDrop 1.8.x peers. New behaviour is used only
   when the peer advertises support; otherwise LanDrop degrades to the old protocol.

## Non-goals for this pass

- Authenticated/encrypted transport and pairing. Still a P1 trust-boundary item,
  but it needs its own reviewed protocol design. Nothing here makes it harder.
- Resume of interrupted transfers, store-and-forward to offline peers.
- Android Gradle/AGP/Kotlin major migration and TypeScript 7 (blocked upstream).

## Wire protocol v2 (backward compatible)

Framing is unchanged: 16-byte UUID handshake, then `u32` big-endian length + JSON,
1 MiB control frame cap, raw file bytes after `file` frames. serde already ignores
unknown fields, so optional fields on existing frames are safe for 1.8.x receivers.

| Frame | Change |
| --- | --- |
| `identity` | adds `caps: string[]` (default `[]`) |
| `text` | adds optional `id`, `reply_to`, `sent_at` (unix ms) |
| `batch` | adds optional `id`, `reply_to` |
| `ack` *(new)* | `{ id, ok, error? }` — receiver → sender after committing a unit that carried an `id` |
| `chat` *(new)* | `{ kind: "edit" \| "delete" \| "react", id, text?, emoji? }` |

Capabilities: `chat2` (understands `chat` frames) and `ack` (sends `ack`). They are
advertised in the mDNS TXT record (`caps=ack,chat2`) and in `identity`. A 1.8.x
peer advertises neither.

Rules:

- **Ack.** After `done`, a sender whose unit carried an `id` waits up to 5 s for an
  `ack`. `ack{ok:true}` → `delivered`; EOF, timeout or a 1.8.x peer → `sent`.
  `ack{ok:false}` → error. A receiver only sends `ack` when the unit carried an `id`,
  so 1.8.x senders never receive an unexpected frame.
- **Chat frames** are only sent to peers whose caps include `chat2`. When caps are
  unknown (peer found by the /24 healer scan), the backend performs one identity
  probe and caches the result. If unsupported, the command returns `unsupported`
  and the UI applies the change locally and says so.
- **Text + files.** A message with a caption sends `text{id}` then `batch{id}` with
  the same `id`, on separate connections as today. A v2 receiver merges them into
  one conversation entry. A 1.8.x receiver shows two bubbles, as before.
- Receivers only apply `edit`/`delete` to messages that the same peer sent, and
  `react` to messages in that conversation. This check is in the frontend because
  it owns history.

## Native command and event contract

All new parameters are optional so call sites can migrate gradually.

### Commands

```ts
lan_send_text(peerId, text, peerIp?, messageId?, replyTo?) -> SendOutcome
lan_send_files(peerId, paths, peerIp?, messageId?, replyTo?) -> SendOutcome
lan_send_chat_event(peerId, event: ChatEvent, peerIp?) -> SendOutcome
lan_cancel_transfer(messageId) -> boolean            // true if a send was cancelled
allow_media_paths(paths: string[]) -> string[]       // paths now readable via asset protocol
get_thumbnail(path, maxPx?) -> string | null         // path of cached JPEG thumbnail (asset-allowed)
read_file_bytes(path) -> ArrayBuffer                 // raw ipc::Response, not number[]
read_file_preview(path, maxLines?)                   // reads at most 256 KiB
save_clipboard_image(raw body bytes, header x-mime)  // raw request body, not base64

type SendOutcome = { status: "delivered" | "sent" | "unsupported" }
type ChatEvent =
  | { kind: "edit"; id: string; text: string }
  | { kind: "delete"; id: string }
  | { kind: "react"; id: string; emoji: string | null }
```

Errors still reject with a string. A cancelled send rejects with `"cancelled"`.

### Events

```ts
lan_text_received   { peer_id, text, id?, reply_to?, sent_at? }
lan_files_received  { peer_id, files, file_details, id?, reply_to?, partial, expected_files }
lan_chat_event      { peer_id, kind, id, text?, emoji? }
lan_transfer_progress { ...existing, message_id?, peer_id? }
lan_peer_discovered { ...existing, caps: string[] }
```

Receive-side `lan_transfer_progress` carries the batch `id` as `message_id` (when the
sender supplied one) and the sender `peer_id`, so the UI can show a receiving bubble
with progress before the files are complete. Batch progress reports byte totals for
the whole batch where known and is throttled per batch, not per file.

## Media pipeline

- Desktop and Android filesystem paths are displayed with `convertFileSrc()` once
  `allow_media_paths` has added the exact file to the asset protocol scope. Only
  image and video extensions are accepted; directories are never added. The CSP
  gains `asset:` and `http://asset.localhost` for `img-src` and `media-src`.
- Video playback uses the asset protocol's range support instead of loading the
  whole file into a Blob.
- Thumbnails are generated once into the app cache directory, keyed by path,
  size, modification time and requested size; repeated views reuse the file.
- `content://` URIs on Android keep the existing byte path.

## Frontend architecture

- `MessageEntry` gains `status`, `error`, `replyTo`, `editedAt`, `deleted`,
  `reactions { mine?, theirs? }` and `sendPaths` (original paths, to retry a
  failed file send). Persisted state version increments; sanitisation accepts old
  snapshots unchanged.
- Transient per-message progress lives in a separate reactive map, not in history.
- Per-peer drafts (`text`, `files`, `replyTo`, `editing`) replace the single shared
  draft, so switching conversations no longer moves a draft to another person.
- Unread state: `lastReadAt` per device; badge in the device list.
- `App.svelte` orchestration moves into `lib/chat/chat-controller.svelte.ts`
  (send, retry, cancel, edit, delete, react, forward, receive handlers).

## Interface

Adaptive layout by window width:

- **Compact (< 720 px)**: title bar with the device strip (current behaviour,
  refined) and the conversation below.
- **Expanded (≥ 720 px)**: a conversation list pane (avatar, online dot, route,
  last message preview, time, unread badge; offline conversations under "Recent")
  next to the conversation pane.

Conversation: header (avatar, name, status/route, search, saved filter, overflow
menu), date separators, grouped bubbles with Expressive shapes, natively selectable
text, linkified URLs, inline code and highlighted code blocks with Copy. Each
message has a hover action pill (quick reactions, reply, more) and a context menu
(right-click / long-press): Reply, Copy text, Copy image, Edit, Forward, Save,
Show in folder, Star, Delete for me, Delete for everyone. Edited messages show
"edited"; deleted ones leave a tombstone. Sent messages show sending / sent /
delivered / failed with Retry. Attachments show progress and Cancel while moving.

Composer: reply/edit banner, attachment tray with image/video thumbnails, paste of
any number of images, files and videos, drag-and-drop, Enter to send, Shift+Enter
newline, ↑ to edit the last sent message, Esc to cancel reply/edit.

Lightbox: full-resolution image via asset protocol, zoom and pan, previous/next
through the conversation's media, copy image, save, show in folder.

## Verification

- Rust: unit tests for v2 framing compatibility (old frames still parse; new
  optional fields default), ack on loopback, chat events refused for peers without
  `chat2`, cancellation, thumbnail cache reuse, raw IPC commands; fmt, clippy
  `-D warnings`, cargo audit.
- Frontend: unit tests for message reducers (edit/delete/react authorisation,
  merge of text+batch by id, status transitions, draft per peer, sanitize
  migration); svelte-check, ESLint, Prettier, build.
- Native: run the desktop app on Windows and inspect the redesigned UI in compact
  and expanded widths, light and dark themes; loopback send between two
  instances is not possible on one port, so cross-device acceptance stays manual.
