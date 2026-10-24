/**
 * Conversation orchestration: optimistic sends, receipts, retry, cancel, edits,
 * deletions, reactions, forwarding and every incoming chat/transfer event.
 * Components call these methods; native I/O stays in $lib/api/bridge.
 */
import {
  lanCancelTransfer,
  lanSendChatEvent,
  lanSendFiles,
  lanSendText,
  windowShow,
  type ChatEventPayload,
  type ChatEventReceived,
  type FilesReceivedEvent,
  type PreparedSendPath,
  type SendOutcome,
  type TextReceivedEvent,
  type TransferProgress,
} from "$lib/api/bridge";
import type { AppState } from "$lib/state/app-state.svelte";
import { PEER_COLORS, type DiscoveredDevice, type MessageEntry } from "$lib/state/model";
import { sendNativeNotification } from "$lib/utils/native-notifications";
import { playReceiveSound } from "$lib/utils/notification-sound";
import { attachmentsFromReceived, attachmentsFromSelection } from "./attachments";
import { messagePreview } from "./message-reducers";

export interface ChatControllerDeps {
  app: AppState;
  notify: (message: string) => void;
  /** True while the updater is downloading or installing (sending is paused). */
  updateInProgress: () => boolean;
}

/** Transfers from 1.8 peers carry no message ID; they share one global indicator. */
export interface LegacyTransfer {
  direction: "send" | "receive";
  bytes: number;
  total: number;
  currentFile?: string;
}

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error ?? "Unknown error");
}

export class ChatController {
  legacyTransfer = $state<LegacyTransfer | null>(null);
  // Plain Map on purpose: rate-sampling bookkeeping, never rendered.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  #rateSamples = new Map<string, { bytes: number; at: number }>();
  #activeReceives = 0;

  constructor(private deps: ChatControllerDeps) {}

  private get app() {
    return this.deps.app;
  }

  // ── Sending ──

  /** Send (or apply the edit held in) the open conversation's draft. */
  async sendDraft(): Promise<void> {
    const { app } = this;
    const device = app.activeDevice;
    if (!device) return;
    if (this.deps.updateInProgress()) {
      this.deps.notify("Wait for the app update to finish before sending.");
      return;
    }
    if (!device.online) {
      this.deps.notify(`${device.alias} is offline`);
      return;
    }

    const draft = app.draft;
    if (draft.editingId) {
      const text = draft.text.trim();
      const editingId = draft.editingId;
      app.stopEditing();
      if (text) await this.editMessage(editingId, text);
      return;
    }

    const text = draft.text.trim();
    const files = [...draft.files];
    if (!text && files.length === 0) return;

    // Clear the composer immediately: the message now lives in the conversation.
    app.sendTextContent = "";
    app.removeSentFiles(files, device.id);
    app.setReplyTo(null);

    const message = app.addMessage({
      peerId: device.id,
      direction: "sent",
      text,
      attachments: attachmentsFromSelection(files),
      status: "sending",
      replyTo: draft.replyTo,
      sendPaths: files.length > 0 ? files.map((file) => file.path) : undefined,
    });
    await this.deliver(message, device);
  }

  /** Resend a failed message with the same ID so the peer merges rather than duplicates. */
  async retry(messageId: string): Promise<void> {
    const message = this.app.getMessage(messageId);
    if (!message || message.direction !== "sent" || message.status !== "failed") return;
    const device = this.app.devices.find((d) => d.id === message.peerId);
    if (!device?.online) {
      this.deps.notify(`${device?.alias ?? "This device"} is offline`);
      return;
    }
    this.app.setStatus(messageId, "sending");
    await this.deliver(message, device);
  }

  private async deliver(message: MessageEntry, device: DiscoveredDevice): Promise<void> {
    const { app } = this;
    const options = {
      peerIp: device.ip,
      messageId: message.id,
      replyTo: message.replyTo,
    };
    app.activeSends += 1;
    const outcomes: SendOutcome[] = [];
    try {
      if (message.text) {
        outcomes.push(await lanSendText(device.id, message.text, options));
      }
      if (message.sendPaths?.length) {
        let prepared: PreparedSendPath[] = [];
        outcomes.push(
          await lanSendFiles(device.id, message.sendPaths, options, (files) => {
            prepared = files;
          }),
        );
        // Android sends from a private copy; history points at the durable one.
        const moved = prepared.filter((file) => file.historyPath !== file.originalPath);
        for (const file of moved) {
          app.updateAttachmentPath(message.id, file.originalPath, file.historyPath);
        }
      }
      const delivered = outcomes.length > 0 && outcomes.every((o) => o.status === "delivered");
      if (!app.getMessage(message.id)?.deleted) {
        app.setStatus(message.id, delivered ? "delivered" : "sent");
      }
    } catch (error) {
      const detail = errorText(error);
      const cancelled = detail === "cancelled";
      // Deleted for everyone while sending: the tombstone needs no retry state.
      if (app.getMessage(message.id)?.deleted) return;
      app.setStatus(message.id, "failed", cancelled ? "Cancelled" : detail);
      if (!cancelled) {
        this.deps.notify(`Not delivered to ${device.alias}: ${detail}`);
        app.addLog("warn", `Send to ${device.alias} failed: ${detail}`);
      }
    } finally {
      app.activeSends = Math.max(0, app.activeSends - 1);
      app.progress.delete(message.id);
      this.#rateSamples.delete(message.id);
    }
  }

  async cancel(messageId: string): Promise<void> {
    const cancelled = await lanCancelTransfer(messageId).catch(() => false);
    if (!cancelled) this.deps.notify("This transfer can no longer be cancelled");
  }

  /** Send a copy of a message (text and attachments) into another conversation. */
  async forward(messageId: string, toPeerId: string): Promise<void> {
    const source = this.app.getMessage(messageId);
    const device = this.app.devices.find((d) => d.id === toPeerId);
    if (!source || source.deleted || !device) return;
    if (!device.online) {
      this.deps.notify(`${device.alias} is offline`);
      return;
    }
    // Sent messages keep every original path; history attachments are capped at 100.
    const paths = source.sendPaths?.length
      ? source.sendPaths
      : (source.attachments ?? []).map((attachment) => attachment.path).filter(Boolean);
    const message = this.app.addMessage({
      peerId: device.id,
      direction: "sent",
      text: source.text,
      attachments: source.attachments,
      status: "sending",
      sendPaths: paths.length > 0 ? paths : undefined,
    });
    this.deps.notify(`Forwarding to ${device.alias}`);
    await this.deliver(message, device);
  }

  // ── Edits, deletions, reactions ──

  private async sendEvent(peerId: string, event: ChatEventPayload, what: string): Promise<void> {
    const device = this.app.devices.find((d) => d.id === peerId);
    if (!device?.online) {
      this.deps.notify(`${what} on this device only — ${device?.alias ?? "the peer"} is offline`);
      return;
    }
    try {
      const outcome = await lanSendChatEvent(peerId, event, device.ip);
      if (outcome.status === "unsupported") {
        this.deps.notify(`${what} on this device only — ${device.alias} runs an older LanDrop`);
      }
    } catch (error) {
      this.deps.notify(`${what} on this device only: ${errorText(error)}`);
    }
  }

  async editMessage(messageId: string, text: string): Promise<void> {
    const message = this.app.getMessage(messageId);
    if (!message || message.direction !== "sent" || message.deleted) return;
    if (message.text === text) return;
    this.app.editOwn(messageId, text);
    await this.sendEvent(message.peerId, { kind: "edit", id: messageId, text }, "Edited");
  }

  async deleteForEveryone(messageId: string): Promise<void> {
    const message = this.app.getMessage(messageId);
    if (!message || message.direction !== "sent") return;
    if (message.status === "sending") await this.cancel(messageId);
    this.app.deleteOwnForEveryone(messageId);
    await this.sendEvent(message.peerId, { kind: "delete", id: messageId }, "Deleted");
  }

  deleteForMe(messageId: string) {
    const message = this.app.getMessage(messageId);
    if (!message) return;
    if (message.status === "sending") void this.cancel(messageId);
    this.app.deleteForMe(messageId);
  }

  async react(messageId: string, emoji: string | null): Promise<void> {
    const message = this.app.getMessage(messageId);
    if (!message || message.deleted) return;
    const next = message.reactions?.mine === emoji ? null : emoji;
    this.app.react(messageId, next);
    const device = this.app.devices.find((d) => d.id === message.peerId);
    // Reactions are lightweight: skip quietly when the peer cannot receive them.
    // The backend answers "unsupported" for LanDrop 1.8 peers without sending anything.
    if (!device?.online) return;
    await lanSendChatEvent(
      device.id,
      { kind: "react", id: messageId, emoji: next },
      device.ip,
    ).catch(() => {});
  }

  // ── Incoming ──

  private ensurePeerVisible(peerId: string) {
    const { app } = this;
    if (app.devices.some((device) => device.id === peerId)) return;
    app.devices = [
      ...app.devices,
      {
        id: peerId,
        alias: `Device-${peerId.slice(0, 8)}`,
        deviceType: "desktop",
        ip: "",
        online: false,
        color: app.devices.length % PEER_COLORS.length,
      },
    ];
    app.addLog("warn", `Recovered hidden peer ${peerId.slice(0, 8)} from inbound traffic`);
  }

  /** Incoming traffic never retargets an open conversation or a draft. */
  private revealIncomingPeer(peerId: string) {
    this.ensurePeerVisible(peerId);
    const { app } = this;
    if (!app.activeDeviceId && app.activeView === "transfer" && app.unassignedFileCount === 0) {
      app.setActiveDevice(peerId, { adoptUnassigned: false });
    }
  }

  private afterIncoming(peerId: string, body: string) {
    const { app } = this;
    const viewing =
      app.activeView === "transfer" && app.activeDeviceId === peerId && document.hasFocus();
    if (viewing) app.markRead(peerId);
    if (app.notificationsEnabled) playReceiveSound();
    if (app.notificationsEnabled && !document.hasFocus()) {
      const alias = app.devices.find((device) => device.id === peerId)?.alias ?? "LanDrop";
      sendNativeNotification(alias, body || "New message", { kind: "peer", peerId }).catch(
        (error) => app.addLog("warn", `Notification unavailable: ${error}`),
      );
    }
    if (app.popOnReceive) void windowShow();
  }

  handleText(event: TextReceivedEvent) {
    this.revealIncomingPeer(event.peer_id);
    this.app.receiveText({
      peerId: event.peer_id,
      text: event.text,
      id: event.id,
      replyTo: event.reply_to,
    });
    this.app.addLog(
      "info",
      `Received text from ${event.peer_id.slice(0, 8)} (${event.text.length} chars)`,
    );
    this.afterIncoming(event.peer_id, event.text);
  }

  handleFiles(event: FilesReceivedEvent) {
    this.revealIncomingPeer(event.peer_id);
    const attachments = attachmentsFromReceived(event.file_details ?? []);
    if (attachments.length > 0) {
      this.app.receiveFiles({
        peerId: event.peer_id,
        id: event.id,
        replyTo: event.reply_to,
        attachments,
        partial: event.partial,
      });
    }
    if (event.id) this.app.progress.delete(event.id);
    const files = event.files ?? [];
    const body = event.partial
      ? `Received ${files.length} of ${event.expected_files ?? files.length} items (interrupted)`
      : files.length === 1
        ? `Received ${files[0]}`
        : `Received ${files.length} items`;
    this.afterIncoming(event.peer_id, body);
  }

  handleChatEvent(event: ChatEventReceived) {
    const { peer_id, ...payload } = event;
    const before = this.app.getMessage(payload.id);
    this.app.receiveChatEvent(peer_id, payload);
    if (payload.kind === "react" && payload.emoji && before?.direction === "sent") {
      const alias = this.app.devices.find((d) => d.id === peer_id)?.alias ?? "Someone";
      const preview = messagePreview(before);
      if (!document.hasFocus() && this.app.notificationsEnabled) {
        sendNativeNotification(alias, `Reacted ${payload.emoji} to “${preview.slice(0, 60)}”`, {
          kind: "peer",
          peerId: peer_id,
        }).catch(() => {});
      }
    }
  }

  handleProgress(progress: TransferProgress) {
    const { app } = this;
    if (progress.direction === "receive") {
      if (progress.phase === "start") this.#activeReceives += 1;
      if (progress.phase === "done" || progress.phase === "error") {
        this.#activeReceives = Math.max(0, this.#activeReceives - 1);
      }
      app.receivingTransferActive = this.#activeReceives > 0;
    }

    const id = progress.message_id ?? null;
    const bytes = progress.sent_bytes ?? progress.received_bytes ?? 0;
    const total = progress.total_bytes ?? 0;
    const files = progress.sent_files ?? progress.received_files ?? 0;
    const totalFiles = progress.total_files ?? 0;

    if (!id) {
      // A 1.8 peer: one shared indicator, as before.
      if (progress.phase === "done" || progress.phase === "error") {
        if (!this.legacyTransfer || this.legacyTransfer.direction === progress.direction) {
          this.legacyTransfer = null;
        }
      } else {
        this.legacyTransfer = {
          direction: progress.direction,
          bytes,
          total,
          currentFile: progress.current_file,
        };
      }
      return;
    }

    if (progress.direction === "receive" && progress.peer_id) {
      if (progress.phase === "start") {
        this.ensurePeerVisible(progress.peer_id);
        app.beginReceiving(progress.peer_id, id);
      } else if (progress.phase === "error") {
        app.failReceiving(progress.peer_id, id);
        this.#rateSamples.delete(id);
        return;
      }
    }

    if (progress.phase === "done" || progress.phase === "error") {
      app.progress.delete(id);
      this.#rateSamples.delete(id);
      return;
    }

    const now = performance.now();
    const sample = this.#rateSamples.get(id);
    let rateBps = app.progress.get(id)?.rateBps;
    if (sample && now - sample.at >= 400) {
      const instant = ((bytes - sample.bytes) * 1000) / (now - sample.at);
      if (instant >= 0) rateBps = rateBps ? rateBps * 0.6 + instant * 0.4 : instant;
      this.#rateSamples.set(id, { bytes, at: now });
    } else if (!sample) {
      this.#rateSamples.set(id, { bytes, at: now });
    }
    app.progress.set(id, { bytes, total, files, totalFiles, rateBps });
  }
}
