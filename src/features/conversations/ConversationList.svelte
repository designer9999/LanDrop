<!--
  Every device you can reach now, plus conversations kept from earlier.
  Online first by recent activity; offline history under "Earlier".
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import PeerAvatar from "../peers/PeerAvatar.svelte";
  import { getAppState, type Conversation } from "$lib/state/app-state.svelte";
  import { refreshLanDiscovery } from "$lib/api/bridge";
  import { messagePreview } from "$lib/chat/message-reducers";
  import { formatListTime } from "$lib/chat/timeline";
  import { peerRouteLabel } from "$lib/utils/peer-utils";

  interface Props {
    onselect?: (id: string) => void;
    onsnackbar: (message: string) => void;
  }

  let { onselect, onsnackbar }: Props = $props();

  const app = getAppState();
  let refreshing = $state(false);

  const online = $derived(app.conversations.filter((c) => c.device.online));
  const earlier = $derived(app.conversations.filter((c) => !c.device.online));

  async function refresh() {
    if (refreshing) return;
    refreshing = true;
    try {
      await refreshLanDiscovery();
      onsnackbar("Looking for devices…");
    } catch (error) {
      onsnackbar(`Couldn't rescan: ${error}`);
    } finally {
      setTimeout(() => (refreshing = false), 900);
    }
  }

  function preview(conversation: Conversation): string {
    const { last } = conversation;
    if (!last) return conversation.device.online ? peerRouteLabel(conversation.device) : "";
    const text = messagePreview(last);
    return last.direction === "sent" ? `You: ${text}` : text;
  }

  function select(id: string) {
    app.activeView = "transfer";
    app.setActiveDevice(id);
    onselect?.(id);
  }
</script>

{#snippet row(conversation: Conversation)}
  {@const device = conversation.device}
  {@const selected = device.id === app.activeDeviceId}
  {@const draft = app.drafts[device.id]}
  <button
    class="item"
    class:item-selected={selected}
    onclick={() => select(device.id)}
    aria-current={selected ? "true" : undefined}
  >
    <PeerAvatar
      name={device.alias}
      color={device.color}
      icon={device.avatarIcon}
      size="xl"
      {selected}
      online={device.online}
    />
    <span class="body">
      <span class="line">
        <span class="name">{device.alias}</span>
        {#if conversation.last}
          <span class="time" class:time-unread={conversation.unread > 0}
            >{formatListTime(conversation.last.timestamp)}</span
          >
        {/if}
      </span>
      <span class="line">
        {#if draft && (draft.text.trim() || draft.files.length) && !selected}
          <span class="preview"
            ><span class="draft">Draft:</span>
            {draft.text.trim() ||
              `${draft.files.length} attachment${draft.files.length === 1 ? "" : "s"}`}</span
          >
        {:else}
          <span class="preview">{preview(conversation)}</span>
        {/if}
        {#if conversation.unread > 0}
          <span class="badge" aria-label="{conversation.unread} unread"
            >{conversation.unread > 99 ? "99+" : conversation.unread}</span
          >
        {:else if device.online && device.network}
          <span class="route" title={peerRouteLabel(device)}>
            <Icon name={device.network === "tailscale" ? "vpn_lock" : "lan"} size={14} />
          </span>
        {/if}
      </span>
    </span>
  </button>
{/snippet}

<nav class="list" aria-label="Conversations">
  <div class="head">
    <span class="title">Devices</span>
    <button
      class="refresh"
      class:spinning={refreshing}
      onclick={refresh}
      aria-label="Rescan for devices"
      title="Rescan for devices"
    >
      <Icon name="refresh" size={20} />
    </button>
  </div>

  <div class="scroll">
    {#if online.length === 0}
      <div class="searching" role="status">
        <span class="radar"><Icon name="radar" size={22} /></span>
        <span
          >{app.discoveryError ||
            "Looking for devices running LanDrop on your network or Tailscale…"}</span
        >
      </div>
    {/if}
    {#each online as conversation (conversation.device.id)}
      {@render row(conversation)}
    {/each}

    {#if earlier.length > 0}
      <div class="section">Earlier</div>
      {#each earlier as conversation (conversation.device.id)}
        {@render row(conversation)}
      {/each}
    {/if}
  </div>
</nav>

<style>
  .list {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 8px 4px 20px;
  }
  .title {
    font-size: 22px;
    font-weight: 600;
    font-variation-settings: "wdth" 115;
    letter-spacing: -0.01em;
  }
  .refresh {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border: none;
    border-radius: 20px;
    background: transparent;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
  }
  .refresh:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  .spinning :global(.material-symbols-rounded) {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 8px 12px;
  }
  .section {
    padding: 16px 12px 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--md-sys-color-on-surface-variant);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px 12px;
    border: none;
    border-radius: 24px;
    background: transparent;
    color: var(--md-sys-color-on-surface);
    font: inherit;
    text-align: left;
    cursor: pointer;
    --avatar-ring: var(--md-sys-color-surface-container-low);
    transition: background-color 150ms ease-out;
  }
  .item:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 6%, transparent);
  }
  .item-selected,
  .item-selected:hover {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
    --avatar-ring: var(--md-sys-color-secondary-container);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-size: 15px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .time {
    flex-shrink: 0;
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .time-unread {
    color: var(--md-sys-color-primary);
    font-weight: 600;
  }
  .preview {
    flex: 1;
    min-width: 0;
    font-size: 13px;
    color: var(--md-sys-color-on-surface-variant);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item-selected .preview,
  .item-selected .time {
    color: inherit;
    opacity: 0.8;
  }
  .draft {
    color: var(--md-sys-color-error);
    font-weight: 500;
  }
  .badge {
    flex-shrink: 0;
    min-width: 22px;
    height: 22px;
    padding: 0 7px;
    border-radius: 11px;
    font-size: 12px;
    font-weight: 700;
    line-height: 22px;
    text-align: center;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
  .route {
    display: flex;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.8;
  }
  .searching {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 4px 4px 8px;
    padding: 14px 16px;
    border-radius: 20px;
    background: var(--md-sys-color-surface-container);
    font-size: 13px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .radar {
    display: flex;
    color: var(--md-sys-color-primary);
    animation: pulse 1.8s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.4;
    }
  }
</style>
