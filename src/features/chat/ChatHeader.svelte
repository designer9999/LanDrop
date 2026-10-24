<!--
  Conversation header: who you're talking to and how they're reachable,
  search, starred filter and conversation actions.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import PeerAvatar from "../peers/PeerAvatar.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import type { DiscoveredDevice } from "$lib/state/model";
  import { menu } from "$lib/ui/menu-state.svelte";
  import { peerRouteDescription } from "$lib/utils/peer-utils";

  interface Props {
    device: DiscoveredDevice;
    /** Compact windows show identity in the title bar strip already. */
    compact?: boolean;
    onsettings: () => void;
    onopenfolder: () => void;
    onclear: () => void;
    ondelete: () => void;
  }

  let { device, compact = false, onsettings, onopenfolder, onclear, ondelete }: Props = $props();

  const app = getAppState();
  let searching = $state(false);
  let searchField: HTMLInputElement | undefined = $state();

  $effect(() => {
    // A conversation switch closes search (setActiveDevice clears the query).
    void app.activeDeviceId;
    searching = false;
  });

  function openSearch() {
    searching = true;
    queueMicrotask(() => searchField?.focus());
  }

  function closeSearch() {
    searching = false;
    app.messageSearch = "";
  }

  function openMore(anchor: HTMLElement) {
    menu.openAt(anchor, [
      { label: "Device settings", icon: "tune", onselect: onsettings },
      { label: "Open receive folder", icon: "folder_open", onselect: onopenfolder },
      "divider",
      { label: "Clear chat (keep starred)", icon: "mop", onselect: onclear },
      { label: "Delete conversation", icon: "delete_forever", danger: true, onselect: ondelete },
    ]);
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      openSearch();
    }
  }}
/>

<header class="header">
  {#if searching}
    <div class="search">
      <Icon name="search" size={20} />
      <input
        bind:this={searchField}
        bind:value={app.messageSearch}
        placeholder="Search in conversation"
        aria-label="Search in conversation"
        onkeydown={(event) => event.key === "Escape" && closeSearch()}
      />
      <button class="icon" onclick={closeSearch} aria-label="Close search"
        ><Icon name="close" size={20} /></button
      >
    </div>
  {:else}
    {#if !compact}
      <button class="identity" onclick={onsettings} title="Device settings">
        <PeerAvatar
          name={device.alias}
          color={device.color}
          icon={device.avatarIcon}
          size="lg"
          selected
          online={device.online}
        />
        <span class="identity-text">
          <span class="name">{device.alias}</span>
          <span class="status" class:status-online={device.online}
            >{peerRouteDescription(device)}</span
          >
        </span>
      </button>
    {:else}
      <button class="identity identity-compact" onclick={onsettings} title="Device settings">
        <span class="identity-text">
          <span class="name">{device.alias}</span>
          <span class="status status-inline" class:status-online={device.online}>
            <span class="dot"></span>{peerRouteDescription(device)}
          </span>
        </span>
      </button>
    {/if}
    <span class="spacer"></span>
    <button class="icon" onclick={openSearch} aria-label="Search" title="Search (Ctrl+F)"
      ><Icon name="search" size={20} /></button
    >
    <button
      class="icon"
      class:icon-active={app.starredOnly}
      onclick={() => (app.starredOnly = !app.starredOnly)}
      aria-pressed={app.starredOnly}
      aria-label="Starred messages"
      title="Starred messages"
    >
      <Icon name={app.starredOnly ? "star" : "star_outline"} size={20} />
    </button>
    <button
      class="icon"
      onclick={(event) => openMore(event.currentTarget)}
      aria-label="Conversation options"
      title="More"
    >
      <Icon name="more_vert" size={20} />
    </button>
  {/if}
</header>

<style>
  .header {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 56px;
    padding: 4px 8px 4px 8px;
    flex-shrink: 0;
    border-bottom: 1px solid
      color-mix(in srgb, var(--md-sys-color-outline-variant) 50%, transparent);
  }
  .identity {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    padding: 4px 12px 4px 4px;
    border: none;
    border-radius: 28px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    --avatar-ring: var(--md-sys-color-surface);
  }
  .identity:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 6%, transparent);
  }
  .identity-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-size: 16px;
    font-weight: 600;
    font-variation-settings: "wdth" 110;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .status {
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .identity-compact {
    padding: 4px 12px;
    border-radius: 20px;
  }
  .status-inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--md-sys-color-outline);
  }
  .status-online .dot {
    background: #2e9e5b;
  }
  .spacer {
    flex: 1;
  }
  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    border: none;
    border-radius: 20px;
    background: transparent;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
  }
  .icon:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  .icon-active {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 44px;
    padding: 0 4px 0 14px;
    border-radius: 22px;
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--md-sys-color-on-surface);
    font: inherit;
    font-size: 14px;
  }
</style>
