<!--
  One message: reply quote, media grid, files, selectable rich text, time,
  delivery state, reactions, live transfer progress. Hover shows quick actions;
  right-click or long-press opens the full menu.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import FileCard from "../transfer/FileCard.svelte";
  import MediaThumb from "./MediaThumb.svelte";
  import RichText from "./RichText.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { QUICK_REACTIONS, type MessageEntry, type MessageProgress } from "$lib/state/model";
  import { menu } from "$lib/ui/menu-state.svelte";
  import { isEmojiOnly } from "$lib/chat/rich-text";
  import { messagePreview } from "$lib/chat/message-reducers";
  import { formatTime, type BubblePosition } from "$lib/chat/timeline";
  import { fileExtensionLabel, fileSizeStr } from "$lib/utils/file-utils";
  import { getChatContext } from "./chat-context";
  import { messageMenuEntries } from "./message-actions";

  interface Props {
    message: MessageEntry;
    position: BubblePosition;
    replyTarget?: MessageEntry;
    progress?: MessageProgress;
    highlighted?: boolean;
    peerAlias: string;
  }

  let {
    message,
    position,
    replyTarget,
    progress,
    highlighted = false,
    peerAlias,
  }: Props = $props();

  const app = getAppState();
  const chat = getChatContext();

  const mine = $derived(message.direction === "sent");
  const attachments = $derived(message.attachments ?? []);
  const media = $derived(attachments.filter((a) => a.type === "image" || a.type === "video"));
  const files = $derived(attachments.filter((a) => a.type === "file" || a.type === "folder"));
  const mediaOnly = $derived(media.length > 0 && files.length === 0 && !message.text);
  const bigEmoji = $derived(
    attachments.length === 0 && !message.replyTo && isEmojiOnly(message.text),
  );
  const visibleMedia = $derived(media.slice(0, 4));
  const hiddenMedia = $derived(media.length - visibleMedia.length);
  const statusIcon = $derived(
    message.status === "sending"
      ? "schedule"
      : message.status === "delivered"
        ? "done_all"
        : message.status === "sent"
          ? "check"
          : message.status === "failed"
            ? "error"
            : null,
  );
  const statusLabel = $derived(
    message.status === "sending"
      ? "Sending"
      : message.status === "delivered"
        ? "Delivered"
        : message.status === "sent"
          ? "Sent"
          : message.status === "failed"
            ? "Not delivered"
            : "",
  );
  const reactions = $derived(
    [message.reactions?.theirs, message.reactions?.mine].filter((r): r is string => !!r),
  );
  const pct = $derived(
    progress && progress.total > 0 ? Math.min(100, (progress.bytes / progress.total) * 100) : null,
  );

  let bubbleEl: HTMLDivElement | undefined = $state();
  let pressTimer: ReturnType<typeof setTimeout> | undefined;

  function openMenu(x: number, y: number) {
    menu.open(x, y, messageMenuEntries(message, app, chat), bubbleEl);
  }

  function handleContextMenu(event: MouseEvent) {
    // Keep the browser menu for links inside text selections? No: ours includes Copy.
    event.preventDefault();
    openMenu(event.clientX, event.clientY);
  }

  function handlePointerDown(event: PointerEvent) {
    if (event.pointerType !== "touch") return;
    clearTimeout(pressTimer);
    pressTimer = setTimeout(() => {
      navigator.vibrate?.(10);
      openMenu(event.clientX, event.clientY);
    }, 450);
  }

  function cancelPress() {
    clearTimeout(pressTimer);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
      event.preventDefault();
      const rect = bubbleEl?.getBoundingClientRect();
      if (rect) openMenu(rect.left + 16, rect.bottom);
    }
  }
</script>

<div
  class="row"
  class:row-mine={mine}
  class:row-gap={position === "solo" || position === "first"}
  data-message-id={message.id}
>
  <div class="stack" class:stack-mine={mine}>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      bind:this={bubbleEl}
      class="bubble pos-{position}"
      class:bubble-mine={mine}
      class:bubble-media={mediaOnly}
      class:bubble-emoji={bigEmoji}
      class:bubble-deleted={message.deleted}
      class:bubble-failed={message.status === "failed"}
      class:bubble-highlight={highlighted}
      role="article"
      tabindex="0"
      aria-label="{mine ? 'You' : peerAlias}, {formatTime(message.timestamp)}"
      oncontextmenu={handleContextMenu}
      onpointerdown={handlePointerDown}
      onpointerup={cancelPress}
      onpointercancel={cancelPress}
      onpointermove={cancelPress}
      onkeydown={handleKeydown}
    >
      {#if message.deleted}
        <span class="deleted"><Icon name="block" size={16} />Message deleted</span>
      {:else}
        {#if message.replyTo}
          <button
            class="quote"
            onclick={() => message.replyTo && chat.revealMessage(message.replyTo)}
            disabled={!replyTarget}
          >
            <span class="quote-author"
              >{replyTarget
                ? replyTarget.direction === "sent"
                  ? "You"
                  : peerAlias
                : "Reply"}</span
            >
            <span class="quote-text"
              >{replyTarget
                ? messagePreview(replyTarget)
                : "Original message isn't available"}</span
            >
          </button>
        {/if}

        {#if visibleMedia.length > 0}
          <div class="media media-{Math.min(visibleMedia.length, 4)}">
            {#each visibleMedia as item, index (item.path + index)}
              <div class="media-cell">
                <MediaThumb
                  attachment={item}
                  large={visibleMedia.length === 1}
                  overlay={index === visibleMedia.length - 1 && hiddenMedia > 0
                    ? `+${hiddenMedia}`
                    : undefined}
                  onopen={() => chat.openMedia(message.id, item.path)}
                />
              </div>
            {/each}
          </div>
        {/if}

        {#if files.length > 0}
          <div class="files">
            {#each files as file, index (file.path + index)}
              <FileCard
                icon={file.type === "folder" ? "folder" : "draft"}
                name={file.name}
                size={file.type === "folder" && file.fileCount
                  ? `${file.fileCount} items · ${file.size ?? ""}`
                  : file.size}
                badge={file.type === "folder" ? "FOLDER" : fileExtensionLabel(file.name)}
                onclick={() => chat.openFilePreview(file.path)}
              />
            {/each}
          </div>
        {/if}

        {#if message.status === "receiving" && attachments.length === 0}
          <div class="receiving"><Icon name="downloading" size={18} />Receiving files…</div>
        {/if}

        {#if progress}
          <div
            class="progress"
            role="progressbar"
            aria-valuenow={pct ?? undefined}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <div class="progress-track">
              <div
                class="progress-fill"
                class:progress-indeterminate={pct === null}
                style:width="{pct ?? 30}%"
              ></div>
            </div>
            <div class="progress-meta">
              <span>
                {fileSizeStr(progress.bytes)}{progress.total
                  ? ` of ${fileSizeStr(progress.total)}`
                  : ""}
                {#if progress.totalFiles > 1}· {progress.files}/{progress.totalFiles} files{/if}
                {#if progress.rateBps}· {fileSizeStr(Math.round(progress.rateBps))}/s{/if}
              </span>
              {#if mine && message.status === "sending"}
                <button class="progress-cancel" onclick={() => chat.controller.cancel(message.id)}
                  >Cancel</button
                >
              {/if}
            </div>
          </div>
        {/if}

        {#if message.text}
          <div class="text" class:text-emoji={bigEmoji}>
            <RichText text={message.text} onsnackbar={chat.notify} />
            <span class="meta-spacer" aria-hidden="true"></span>
          </div>
        {/if}
      {/if}

      <span class="meta" class:meta-overlay={mediaOnly}>
        {#if message.editedAt && !message.deleted}<span>edited</span>{/if}
        {#if message.starred}<Icon name="star" size={12} />{/if}
        <time datetime={message.timestamp}>{formatTime(message.timestamp)}</time>
        {#if mine && statusIcon && !message.deleted}
          <span class="status status-{message.status}" title={statusLabel} aria-label={statusLabel}>
            <Icon name={statusIcon} size={14} />
          </span>
        {/if}
      </span>
    </div>

    {#if reactions.length > 0}
      <div class="reactions" class:reactions-mine={mine}>
        {#each reactions as emoji, index (index)}
          <button
            class="reaction"
            class:reaction-mine={emoji === message.reactions?.mine &&
              index === reactions.length - 1}
            onclick={() => chat.controller.react(message.id, emoji)}
            title={index === reactions.length - 1 && message.reactions?.mine
              ? "Your reaction — click to remove"
              : `${peerAlias} reacted`}>{emoji}</button
          >
        {/each}
      </div>
    {/if}

    {#if mine && message.status === "failed" && !message.deleted}
      <div class="failed-line">
        <Icon name="error" size={14} />
        <span>{message.error === "Cancelled" ? "Cancelled" : "Not delivered"}</span>
        <button onclick={() => chat.controller.retry(message.id)}>Retry</button>
      </div>
    {:else if !mine && message.status === "failed"}
      <div class="failed-line failed-line-received">
        <Icon name="warning" size={14} /><span>{message.error ?? "Some files did not arrive"}</span>
      </div>
    {/if}
  </div>

  {#if !message.deleted && message.status !== "receiving"}
    <div class="hover-actions" class:hover-actions-mine={mine}>
      <button
        title="React"
        aria-label="React"
        onclick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          menu.open(
            rect.left,
            rect.bottom + 4,
            [
              {
                reactions: QUICK_REACTIONS,
                selected: message.reactions?.mine,
                onreact: (emoji) => void chat.controller.react(message.id, emoji),
              },
            ],
            event.currentTarget,
          );
        }}><Icon name="add_reaction" size={18} /></button
      >
      <button
        title="Reply"
        aria-label="Reply"
        onclick={() => {
          app.setReplyTo(message.id);
          chat.focusComposer();
        }}><Icon name="reply" size={18} /></button
      >
      <button
        title="More"
        aria-label="More actions"
        onclick={(event) =>
          menu.openAt(event.currentTarget, messageMenuEntries(message, app, chat))}
        ><Icon name="more_vert" size={18} /></button
      >
    </div>
  {/if}
</div>

<style>
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 1px 12px;
    content-visibility: auto;
    contain-intrinsic-size: auto 52px;
  }
  .row-mine {
    flex-direction: row-reverse;
  }
  .row-gap {
    margin-top: 8px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    max-width: min(78%, 560px);
    min-width: 0;
  }
  .stack-mine {
    align-items: flex-end;
  }

  /* ── Bubble: expressive large radius, tighter on the joined side ── */
  .bubble {
    --r: 20px;
    --r-join: 6px;
    position: relative;
    min-width: 64px;
    max-width: 100%;
    padding: 8px 12px 7px;
    border-radius: var(--r);
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
    font-size: 14.5px;
    line-height: 1.45;
    user-select: text;
    cursor: auto;
    outline: none;
    transition:
      box-shadow 240ms ease-out,
      background-color 240ms ease-out;
  }
  .bubble:focus-visible {
    box-shadow: 0 0 0 3px var(--md-sys-color-primary);
  }
  .bubble-mine {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  .bubble:not(.bubble-mine).pos-first {
    border-bottom-left-radius: var(--r-join);
  }
  .bubble:not(.bubble-mine).pos-middle {
    border-top-left-radius: var(--r-join);
    border-bottom-left-radius: var(--r-join);
  }
  .bubble:not(.bubble-mine).pos-last {
    border-top-left-radius: var(--r-join);
  }
  .bubble-mine.pos-first {
    border-bottom-right-radius: var(--r-join);
  }
  .bubble-mine.pos-middle {
    border-top-right-radius: var(--r-join);
    border-bottom-right-radius: var(--r-join);
  }
  .bubble-mine.pos-last {
    border-top-right-radius: var(--r-join);
  }
  .bubble-media {
    padding: 0;
    overflow: hidden;
    width: min(340px, 100%);
  }
  .bubble-emoji {
    background: transparent !important;
    padding: 0 2px;
  }
  .bubble-deleted {
    background: transparent;
    border: 1px dashed var(--md-sys-color-outline-variant);
    color: var(--md-sys-color-on-surface-variant);
  }
  .bubble-failed.bubble-mine {
    background: color-mix(
      in srgb,
      var(--md-sys-color-error-container) 55%,
      var(--md-sys-color-primary-container)
    );
  }
  .bubble-highlight {
    box-shadow: 0 0 0 3px var(--md-sys-color-tertiary);
    animation: flash 1.4s ease-out;
  }
  @keyframes flash {
    0%,
    30% {
      box-shadow: 0 0 0 6px color-mix(in srgb, var(--md-sys-color-tertiary) 60%, transparent);
    }
  }

  .deleted {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-style: italic;
    padding-right: 72px;
  }

  /* Reply quote */
  .quote {
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 100%;
    margin: 0 0 6px;
    padding: 4px 10px;
    border: none;
    border-left: 3px solid var(--md-sys-color-tertiary);
    border-radius: 8px;
    background: color-mix(in srgb, currentColor 7%, transparent);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    min-width: 160px;
  }
  .bubble-media .quote {
    margin: 6px 6px 6px;
    width: calc(100% - 12px);
  }
  .quote:disabled {
    cursor: default;
  }
  .quote-author {
    font-size: 12px;
    font-weight: 600;
    color: var(--md-sys-color-tertiary);
  }
  .bubble-mine .quote-author {
    color: inherit;
  }
  .quote-text {
    font-size: 13px;
    opacity: 0.85;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  /* Media grid */
  .media {
    display: grid;
    gap: 2px;
    margin: -8px -12px 6px;
    border-radius: inherit;
    overflow: hidden;
  }
  .bubble-media .media {
    margin: 0;
  }
  .media-1 {
    grid-template-columns: 1fr;
  }
  .media-2 {
    grid-template-columns: 1fr 1fr;
    grid-auto-rows: 150px;
  }
  .media-3 {
    grid-template-columns: 1fr 1fr;
    grid-auto-rows: 120px;
  }
  .media-3 .media-cell:first-child {
    grid-row: span 2;
  }
  .media-4 {
    grid-template-columns: 1fr 1fr;
    grid-auto-rows: 120px;
  }
  .media-cell {
    min-width: 0;
    min-height: 0;
  }
  .bubble:not(.bubble-media) .media {
    width: calc(100% + 24px);
    min-width: 240px;
  }

  .files {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 4px;
  }

  .receiving {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 64px 2px 0;
    color: var(--md-sys-color-on-surface-variant);
  }

  /* Transfer progress */
  .progress {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 220px;
    margin: 4px 0 2px;
  }
  .bubble-media .progress {
    padding: 0 10px 28px;
  }
  .progress-track {
    height: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, currentColor 18%, transparent);
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    border-radius: 2px;
    background: var(--md-sys-color-primary);
    transition: width 200ms linear;
  }
  .progress-indeterminate {
    animation: indeterminate 1.2s ease-in-out infinite;
  }
  @keyframes indeterminate {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  .progress-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    opacity: 0.85;
    font-variant-numeric: tabular-nums;
  }
  .progress-cancel {
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 8px;
  }
  .progress-cancel:hover {
    background: color-mix(in srgb, currentColor 10%, transparent);
  }

  .text {
    position: relative;
  }
  .text-emoji {
    font-size: 40px;
    line-height: 1.2;
  }
  /* Reserve room on the last line for the floating time/status. */
  .meta-spacer {
    display: inline-block;
    width: 64px;
    height: 1px;
  }
  .meta {
    position: absolute;
    right: 10px;
    bottom: 5px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    line-height: 1;
    opacity: 0.72;
    user-select: none;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .meta-overlay {
    right: 8px;
    bottom: 8px;
    padding: 3px 7px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--md-sys-color-scrim) 55%, transparent);
    color: white;
    opacity: 1;
  }
  .bubble-emoji .meta {
    position: static;
    margin-left: 6px;
  }
  .status-delivered {
    color: var(--md-sys-color-primary);
  }
  .bubble-mine .status-delivered {
    color: inherit;
    opacity: 1;
  }
  .status-failed {
    color: var(--md-sys-color-error);
  }

  /* Reactions */
  .reactions {
    display: flex;
    gap: 2px;
    margin-top: -6px;
    margin-left: 10px;
    z-index: 1;
  }
  .reactions-mine {
    margin-left: 0;
    margin-right: 10px;
  }
  .reaction {
    height: 26px;
    min-width: 30px;
    padding: 0 6px;
    border-radius: 13px;
    border: 2px solid var(--md-sys-color-surface);
    background: var(--md-sys-color-surface-container-highest);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
    transition: transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
    animation: pop var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  .reaction:hover {
    transform: scale(1.12);
  }
  .reaction-mine {
    background: var(--md-sys-color-secondary-container);
  }
  @keyframes pop {
    from {
      transform: scale(0.4);
    }
  }

  .failed-line {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 3px;
    font-size: 12px;
    color: var(--md-sys-color-error);
  }
  .failed-line button {
    border: none;
    background: transparent;
    color: var(--md-sys-color-primary);
    font: inherit;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 8px;
    cursor: pointer;
  }
  .failed-line button:hover {
    background: color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent);
  }
  .failed-line-received {
    color: var(--md-sys-color-on-surface-variant);
  }

  /* Hover actions */
  .hover-actions {
    display: flex;
    gap: 0;
    padding: 2px;
    border-radius: 18px;
    background: var(--md-sys-color-surface-container);
    box-shadow: var(--shadow-level1);
    opacity: 0;
    transform: scale(0.9);
    pointer-events: none;
    transition:
      opacity 120ms ease-out,
      transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  .row:hover .hover-actions,
  .row:focus-within .hover-actions {
    opacity: 1;
    transform: none;
    pointer-events: auto;
  }
  .hover-actions button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: none;
    border-radius: 15px;
    background: transparent;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
  }
  .hover-actions button:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
    color: var(--md-sys-color-on-surface);
  }
  @media (hover: none) {
    .hover-actions {
      display: none;
    }
  }
</style>
