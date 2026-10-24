import { describe, expect, it } from "vitest";
import type { MessageEntry } from "$lib/state/model";
import {
  applyChatEvent,
  applyIncomingFiles,
  applyIncomingText,
  beginIncomingTransfer,
  deleteOwnMessageForEveryone,
  editOwnMessage,
  failIncomingTransfer,
  messagePreview,
  setMessageStatus,
  setMyReaction,
  unreadCount,
} from "./message-reducers";

const T0 = "2026-10-24T10:00:00.000Z";
const T1 = "2026-10-24T10:01:00.000Z";

function received(id: string, peerId = "alice", text = "hi"): MessageEntry {
  return { id, peerId, direction: "received", text, timestamp: T0 };
}

function sent(id: string, peerId = "alice", text = "hello"): MessageEntry {
  return { id, peerId, direction: "sent", text, timestamp: T0 };
}

describe("incoming messages", () => {
  it("keeps the sender's ID so later edits can find the message", () => {
    const next = applyIncomingText([], { peerId: "alice", text: "hi", id: "m1" }, T0);
    expect(next).toEqual([
      { id: "m1", peerId: "alice", direction: "received", text: "hi", timestamp: T0 },
    ]);
  });

  it("keeps a reply's quoted preview for originals this device doesn't have", () => {
    const next = applyIncomingText(
      [],
      { peerId: "alice", text: "yes", id: "m2", replyTo: "unknown", replyPreview: "  the plan  " },
      T0,
    );
    expect(next[0]).toMatchObject({ replyTo: "unknown", replyPreview: "the plan" });
  });

  it("assigns a local ID to messages from older peers", () => {
    const next = applyIncomingText([], { peerId: "alice", text: "legacy" }, T0);
    expect(next[0].id).toMatch(/[0-9a-f-]{36}/);
  });

  it("refuses to reuse an ID that already belongs to another conversation", () => {
    const start = [sent("m1", "bob")];
    const next = applyIncomingText(start, { peerId: "alice", text: "spoof", id: "m1" }, T1);
    expect(next).toHaveLength(2);
    expect(next[0]).toEqual(start[0]);
    expect(next[1].id).not.toBe("m1");
  });

  it("merges a caption and its attachment batch into one message", () => {
    const withText = applyIncomingText([], { peerId: "alice", text: "look", id: "m1" }, T0);
    const merged = applyIncomingFiles(
      withText,
      {
        peerId: "alice",
        id: "m1",
        attachments: [{ name: "a.png", path: "/r/a.png", type: "image" }],
      },
      T1,
    );
    expect(merged).toHaveLength(1);
    expect(merged[0]).toMatchObject({ text: "look", attachments: [{ name: "a.png" }] });
    expect(merged[0].timestamp).toBe(T0);
  });

  it("merges a caption that arrives after its files", () => {
    const withFiles = applyIncomingFiles(
      [],
      { peerId: "alice", id: "m1", attachments: [{ name: "a", path: "/a", type: "file" }] },
      T0,
    );
    const merged = applyIncomingText(withFiles, { peerId: "alice", text: "late", id: "m1" }, T1);
    expect(merged).toHaveLength(1);
    expect(merged[0].text).toBe("late");
  });

  it("shows a receiving placeholder and fills it when the batch completes", () => {
    const placeholder = beginIncomingTransfer([], "alice", "m1", T0);
    expect(placeholder[0].status).toBe("receiving");
    const done = applyIncomingFiles(
      placeholder,
      { peerId: "alice", id: "m1", attachments: [{ name: "a", path: "/a", type: "file" }] },
      T1,
    );
    expect(done).toHaveLength(1);
    expect(done[0].status).toBeUndefined();
  });

  it("replaces, rather than duplicates, files resent by a retry", () => {
    const first = applyIncomingFiles(
      [],
      {
        peerId: "alice",
        id: "m1",
        partial: true,
        attachments: [
          { name: "a.jpg", path: "/r/a.jpg", type: "image" },
          { name: "b.jpg", path: "/r/b.jpg", type: "image" },
        ],
      },
      T0,
    );
    const retried = applyIncomingFiles(
      first,
      {
        peerId: "alice",
        id: "m1",
        attachments: [
          { name: "a.jpg", path: "/r/a (1).jpg", type: "image" },
          { name: "b.jpg", path: "/r/b (1).jpg", type: "image" },
          { name: "c.jpg", path: "/r/c.jpg", type: "image" },
        ],
      },
      T1,
    );
    expect(retried[0].attachments?.map((a) => a.path)).toEqual([
      "/r/a (1).jpg",
      "/r/b (1).jpg",
      "/r/c.jpg",
    ]);
    expect(retried[0].status).toBeUndefined();
  });

  it("marks partial batches and drops empty placeholders on failure", () => {
    const partial = applyIncomingFiles(
      [],
      {
        peerId: "alice",
        id: "m1",
        partial: true,
        attachments: [{ name: "a", path: "/a", type: "file" }],
      },
      T0,
    );
    expect(partial[0]).toMatchObject({ status: "failed" });
    const placeholder = beginIncomingTransfer([], "alice", "m2", T0);
    expect(failIncomingTransfer(placeholder, "alice", "m2")).toEqual([]);
  });
});

describe("peer chat events", () => {
  it("applies an edit to a message that peer sent", () => {
    const next = applyChatEvent(
      [received("m1")],
      "alice",
      { kind: "edit", id: "m1", text: "fixed" },
      T1,
    );
    expect(next[0]).toMatchObject({ text: "fixed", editedAt: T1 });
  });

  it("ignores edits and deletes aimed at our own or another peer's messages", () => {
    const start = [sent("mine"), received("bobs", "bob")];
    expect(applyChatEvent(start, "alice", { kind: "edit", id: "mine", text: "x" }, T1)).toBe(start);
    expect(applyChatEvent(start, "alice", { kind: "delete", id: "bobs" }, T1)).toBe(start);
  });

  it("turns a deleted message into a tombstone without its content", () => {
    const start: MessageEntry[] = [
      { ...received("m1"), attachments: [{ name: "a", path: "/a", type: "file" }], starred: true },
    ];
    const next = applyChatEvent(start, "alice", { kind: "delete", id: "m1" }, T1);
    expect(next[0]).toEqual({
      id: "m1",
      peerId: "alice",
      direction: "received",
      timestamp: T0,
      text: "",
      deleted: true,
    });
  });

  it("records and clears the peer's reaction on any message in that conversation", () => {
    const reacted = applyChatEvent(
      [sent("m1")],
      "alice",
      { kind: "react", id: "m1", emoji: "👍" },
      T1,
    );
    expect(reacted[0].reactions).toEqual({ theirs: "👍" });
    const cleared = applyChatEvent(reacted, "alice", { kind: "react", id: "m1", emoji: null }, T1);
    expect(cleared[0].reactions).toBeUndefined();
    expect(
      applyChatEvent([sent("m1", "bob")], "alice", { kind: "react", id: "m1", emoji: "👍" }, T1)[0]
        .reactions,
    ).toBeUndefined();
  });
});

describe("own message actions", () => {
  it("edits only our own, non-deleted messages", () => {
    expect(editOwnMessage([sent("m1")], "m1", "new", T1)[0]).toMatchObject({
      text: "new",
      editedAt: T1,
    });
    const theirs = [received("m1")];
    expect(editOwnMessage(theirs, "m1", "new", T1)).toBe(theirs);
  });

  it("deletes for everyone as a tombstone", () => {
    expect(deleteOwnMessageForEveryone([sent("m1")], "m1")[0]).toMatchObject({
      deleted: true,
      text: "",
    });
  });

  it("tracks my reaction separately from theirs", () => {
    const start: MessageEntry[] = [{ ...received("m1"), reactions: { theirs: "🔥" } }];
    expect(setMyReaction(start, "m1", "👍")[0].reactions).toEqual({ theirs: "🔥", mine: "👍" });
    expect(setMyReaction(start, "m1", null)[0].reactions).toEqual({ theirs: "🔥" });
  });

  it("moves delivery status forward and clears errors", () => {
    const failed = setMessageStatus([sent("m1")], "m1", "failed", "offline");
    expect(failed[0]).toMatchObject({ status: "failed", error: "offline" });
    expect(setMessageStatus(failed, "m1", "delivered")[0]).toMatchObject({ status: "delivered" });
    expect(setMessageStatus(failed, "m1", "delivered")[0].error).toBeUndefined();
  });
});

describe("summaries", () => {
  it("counts unread received messages after the last read time", () => {
    const messages = [received("a"), { ...received("b"), timestamp: T1 }, sent("c")];
    expect(unreadCount(messages, "alice", undefined)).toBe(2);
    expect(unreadCount(messages, "alice", T0)).toBe(1);
  });

  it("describes attachments in previews", () => {
    expect(
      messagePreview({
        ...received("a", "alice", ""),
        attachments: [{ name: "x.png", path: "", type: "image" }],
      }),
    ).toBe("Photo");
    expect(messagePreview({ ...received("a", "alice", ""), deleted: true })).toBe(
      "Message deleted",
    );
    expect(messagePreview(received("a", "alice", "  multi\n line "))).toBe("multi line");
  });
});
