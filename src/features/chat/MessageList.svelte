<!--
  Conversation timeline: date separators, unread divider, grouped bubbles.
  Sticks to the newest message while you're at the bottom; otherwise shows a
  jump-to-latest button with the number of messages that arrived meanwhile.
-->
<script lang="ts">
  import { tick } from "svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import MessageBubble from "./MessageBubble.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import type { MessageEntry } from "$lib/state/model";
  import { buildTimeline } from "$lib/chat/timeline";

  interface Props {
    messages: MessageEntry[];
    peerAlias: string;
    /** Search or starred filter: flat list, no auto-scroll. */
    filtered?: boolean;
    highlightId?: string | null;
  }

  let { messages, peerAlias, filtered = false, highlightId = null }: Props = $props();

  const app = getAppState();
  let scroller: HTMLDivElement | undefined = $state();
  let atBottom = $state(true);
  let unseen = $state(0);
  let lastCount = 0;
  let lastPeer: string | null = null;

  const timeline = $derived(
    buildTimeline(messages, { unreadAfter: filtered ? undefined : app.unreadAfter }),
  );
  const byId = $derived(new Map(app.messages.map((m) => [m.id, m])));

  function scrollToBottom(smooth = false) {
    if (!scroller) return;
    scroller.scrollTo({ top: scroller.scrollHeight, behavior: smooth ? "smooth" : "auto" });
  }

  function handleScroll() {
    if (!scroller) return;
    const { scrollTop, scrollHeight, clientHeight } = scroller;
    atBottom = scrollHeight - scrollTop - clientHeight < 80;
    if (atBottom) unseen = 0;
  }

  // Conversation switch: land on the unread divider, or the newest message.
  $effect(() => {
    const peer = app.activeDeviceId;
    if (peer === lastPeer) return;
    lastPeer = peer;
    lastCount = messages.length;
    unseen = 0;
    void tick().then(() => {
      const divider = scroller?.querySelector<HTMLElement>("[data-unread]");
      if (divider && scroller) {
        scroller.scrollTop = divider.offsetTop - 24;
      } else {
        scrollToBottom();
      }
      // Media heights settle after layout; keep the bottom pinned briefly.
      requestAnimationFrame(() => !divider && scrollToBottom());
    });
  });

  // New messages: follow when at the bottom (or when we sent one), else count them.
  $effect(() => {
    const count = messages.length;
    if (filtered || count === lastCount) {
      lastCount = count;
      return;
    }
    const added = messages.slice(lastCount);
    lastCount = count;
    const sentByMe = added.some((m) => m.direction === "sent");
    if (atBottom || sentByMe) {
      void tick().then(() => scrollToBottom(true));
    } else {
      unseen += added.filter((m) => m.direction === "received").length;
    }
  });

  // Stay pinned to the bottom while images load or the composer grows
  // (reply/edit banners, attachment tray, multi-line text).
  $effect(() => {
    if (!scroller) return;
    const content = scroller.firstElementChild;
    const observer = new ResizeObserver(() => {
      if (atBottom && !filtered) scrollToBottom();
    });
    observer.observe(scroller);
    if (content) observer.observe(content);
    return () => observer.disconnect();
  });

  export function reveal(id: string) {
    const row = scroller?.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(id)}"]`);
    row?.scrollIntoView({ block: "center", behavior: "smooth" });
    return !!row;
  }
</script>

<div class="list-wrap">
  <div bind:this={scroller} class="scroller" onscroll={handleScroll}>
    <div class="content" role="log" aria-live="polite" aria-relevant="additions">
      {#each timeline as item (item.key)}
        {#if item.kind === "date"}
          <div class="separator"><span>{item.label}</span></div>
        {:else if item.kind === "unread"}
          <div class="unread" data-unread>
            <span>{item.count === 1 ? "1 new message" : `${item.count} new messages`}</span>
          </div>
        {:else}
          <MessageBubble
            message={item.message}
            position={item.position}
            replyTarget={item.message.replyTo ? byId.get(item.message.replyTo) : undefined}
            progress={app.progress.get(item.message.id)}
            highlighted={highlightId === item.message.id}
            {peerAlias}
          />
        {/if}
      {/each}
      <div class="tail"></div>
    </div>
  </div>

  {#if !atBottom && !filtered}
    <button class="jump" onclick={() => scrollToBottom(true)} aria-label="Jump to latest message">
      <Icon name="keyboard_arrow_down" size={24} />
      {#if unseen > 0}<span class="jump-badge">{unseen}</span>{/if}
    </button>
  {/if}
</div>

<style>
  .list-wrap {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .scroller {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
  }
  .content {
    display: flex;
    flex-direction: column;
    min-height: 100%;
    justify-content: flex-end;
    padding: 8px 0 4px;
    max-width: 920px;
    margin: 0 auto;
  }
  .tail {
    height: 4px;
    flex-shrink: 0;
  }
  .separator {
    display: flex;
    justify-content: center;
    margin: 14px 0 6px;
    position: sticky;
    top: 6px;
    z-index: 2;
    pointer-events: none;
  }
  .separator span {
    padding: 4px 12px;
    border-radius: 12px;
    font-size: 12px;
    font-weight: 500;
    color: var(--md-sys-color-on-surface-variant);
    background: color-mix(in srgb, var(--md-sys-color-surface-container) 92%, transparent);
    backdrop-filter: blur(6px);
  }
  .unread {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 12px 16px 4px;
    font-size: 12px;
    font-weight: 600;
    color: var(--md-sys-color-tertiary);
  }
  .unread::before,
  .unread::after {
    content: "";
    flex: 1;
    height: 1px;
    background: color-mix(in srgb, var(--md-sys-color-tertiary) 45%, transparent);
  }
  .jump {
    position: absolute;
    right: 16px;
    bottom: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border: none;
    border-radius: 14px;
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
    box-shadow: var(--shadow-level2);
    cursor: pointer;
    animation: jump-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
    z-index: 5;
  }
  .jump:hover {
    border-radius: 22px;
  }
  @keyframes jump-in {
    from {
      transform: scale(0.6);
      opacity: 0;
    }
  }
  .jump-badge {
    position: absolute;
    top: -6px;
    right: -6px;
    min-width: 20px;
    height: 20px;
    padding: 0 5px;
    border-radius: 10px;
    font-size: 11px;
    font-weight: 700;
    line-height: 20px;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
</style>
